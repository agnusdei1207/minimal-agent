use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::coordinator::AgentSnapshot;
use crate::domain::{AgentId, ContextBudget, InsightId, estimate_tokens};
use crate::journal::{JournalEvent, RunJournal};

use super::constants::{
    AGENTS_DIR_NAME, BATTLEFIELD_FILE_NAME, BRIEF_FILE_NAME, MAX_PROJECTION_BYTES_LIMIT,
};
use super::error::BriefError;
use super::io::{atomic_replace, digest};
use super::template::{
    render_main_template, render_runtime_block, render_worker_template, replace_runtime_block,
};
use super::types::BriefDraft;
use super::validate::{validate_knowledge, validate_source_metadata, validate_structure};

struct BriefStoreInner {
    root: PathBuf,
    journal: Arc<RunJournal>,
    budget: ContextBudget,
    write_lock: Mutex<()>,
}

#[derive(Clone)]
pub struct AgentBriefStore {
    inner: Arc<BriefStoreInner>,
}

impl AgentBriefStore {
    pub fn new(
        run_root: impl AsRef<Path>,
        journal: Arc<RunJournal>,
        budget: ContextBudget,
    ) -> Self {
        Self {
            inner: Arc::new(BriefStoreInner {
                root: run_root.as_ref().to_path_buf(),
                journal,
                budget,
                write_lock: Mutex::new(()),
            }),
        }
    }

    pub fn initialize(&self, agent: &AgentSnapshot) -> Result<(), BriefError> {
        let path = self.path(&agent.id);
        if path.exists() {
            self.read(&agent.id)?;
            return Ok(());
        }
        let markdown = if agent.id.is_main() {
            render_main_template(std::slice::from_ref(agent))
        } else {
            render_worker_template(agent)
        };
        self.write_projection(&path, markdown.as_bytes())
    }

    pub fn read(&self, agent: &AgentId) -> Result<String, BriefError> {
        let markdown = self.read_projection(&self.path(agent))?;
        self.validate_projection(agent, &markdown)?;
        Ok(markdown)
    }

    pub fn sync_main_runtime(
        &self,
        actor: &AgentId,
        team: &[AgentSnapshot],
    ) -> Result<(), BriefError> {
        let main = AgentId::main();
        if actor != &main {
            return Err(BriefError::Ownership {
                actor: actor.clone(),
                target: main,
            });
        }
        let path = self.path(&AgentId::main());
        let current = self.read(&main)?;
        let runtime = render_runtime_block(team);
        let updated = replace_runtime_block(&current, &runtime)?;
        self.validate_projection(&main, &updated)?;
        self.write_projection(&path, updated.as_bytes())
    }

    pub fn commit(
        &self,
        actor: &AgentId,
        target: &AgentId,
        draft: BriefDraft,
        required_insights: &[InsightId],
    ) -> Result<(), BriefError> {
        if actor != target {
            return Err(BriefError::Ownership {
                actor: actor.clone(),
                target: target.clone(),
            });
        }
        self.validate_projection(target, &draft.markdown)?;
        draft
            .coverage
            .validate(&draft.source_ranges, required_insights)
            .map_err(BriefError::Coverage)?;
        validate_source_metadata(&draft.source_sha256, &draft.source_ranges)?;
        let markdown_sha256 = digest(draft.markdown.as_bytes());
        self.inner
            .journal
            .append_sync(JournalEvent::BriefCheckpoint {
                agent_id: target.clone(),
                markdown: draft.markdown.clone(),
                markdown_sha256,
                source_sha256: draft.source_sha256,
                source_ranges: draft.source_ranges,
                coverage: draft.coverage,
            })
            .map_err(BriefError::Journal)?;
        self.write_projection(&self.path(target), draft.markdown.as_bytes())
    }

    pub fn recover(&self) -> Result<(), BriefError> {
        self.recover_matching(None)
    }

    pub fn recover_selected(&self, agents: &HashSet<AgentId>) -> Result<(), BriefError> {
        self.recover_matching(Some(agents))
    }

    fn recover_matching(&self, selected: Option<&HashSet<AgentId>>) -> Result<(), BriefError> {
        let mut latest = HashMap::<AgentId, String>::new();
        let mut latest_note = HashMap::<AgentId, String>::new();
        for entry in self.inner.journal.replay()? {
            match entry.event {
                JournalEvent::BriefCheckpoint {
                    agent_id,
                    markdown,
                    markdown_sha256,
                    source_sha256,
                    source_ranges,
                    coverage,
                    ..
                } => {
                    if selected.is_some_and(|agents| !agents.contains(&agent_id)) {
                        continue;
                    }
                    if digest(markdown.as_bytes()) != markdown_sha256 {
                        return Err(BriefError::CheckpointDigest(agent_id));
                    }
                    self.validate_projection(&agent_id, &markdown)?;
                    validate_source_metadata(&source_sha256, &source_ranges)?;
                    let accounted = coverage
                        .covered_insight_ids
                        .iter()
                        .chain(coverage.superseded_insight_ids.iter())
                        .cloned()
                        .collect::<Vec<_>>();
                    coverage
                        .validate(&source_ranges, &accounted)
                        .map_err(BriefError::Coverage)?;
                    latest.insert(agent_id, markdown);
                }
                JournalEvent::BriefNote { agent_id, markdown } => {
                    if selected.is_some_and(|agents| !agents.contains(&agent_id)) {
                        continue;
                    }
                    latest_note.insert(agent_id, markdown);
                }
                _ => {}
            }
        }
        for (agent, markdown) in latest {
            self.write_projection(&self.path(&agent), markdown.as_bytes())?;
        }
        for (agent, markdown) in latest_note {
            self.write_projection(&self.note_path(&agent), markdown.as_bytes())?;
        }
        Ok(())
    }

    pub fn remove_projection(&self, agent: &AgentId) -> Result<(), BriefError> {
        let _guard = self
            .inner
            .write_lock
            .lock()
            .map_err(|_| BriefError::LockPoisoned)?;
        let path = self.path(agent);
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(BriefError::Io(error)),
        }
    }

    pub fn path(&self, agent: &AgentId) -> PathBuf {
        self.inner
            .root
            .join(AGENTS_DIR_NAME)
            .join(agent.as_str())
            .join(BRIEF_FILE_NAME)
    }

    /// The agent-authored battlefield note (see `write_note`). Kept beside the
    /// coverage-proven brief projection so neither overwrites the other.
    fn note_path(&self, agent: &AgentId) -> PathBuf {
        self.inner
            .root
            .join(AGENTS_DIR_NAME)
            .join(agent.as_str())
            .join(BATTLEFIELD_FILE_NAME)
    }

    /// Record the agent's own battlefield note. Unlike `commit`, this carries no
    /// coverage proof and imposes no template structure, so a weak model can
    /// always keep its strategy current (the `brief` tool, INTENT-0001 §9.1). Only
    /// the owning agent may write its own note; size is bounded, last write wins.
    pub fn write_note(
        &self,
        actor: &AgentId,
        target: &AgentId,
        markdown: &str,
    ) -> Result<(), BriefError> {
        if actor != target {
            return Err(BriefError::Ownership {
                actor: actor.clone(),
                target: target.clone(),
            });
        }
        self.validate_projection_bytes(markdown.len())?;
        self.inner
            .journal
            .append_sync(JournalEvent::BriefNote {
                agent_id: target.clone(),
                markdown: markdown.to_owned(),
            })
            .map_err(BriefError::Journal)?;
        self.write_projection(&self.note_path(target), markdown.as_bytes())
    }

    /// The agent's battlefield note if it has written one, else `None` (it has
    /// only the default structured template so far).
    pub fn read_note(&self, agent: &AgentId) -> Result<Option<String>, BriefError> {
        match self.read_projection(&self.note_path(agent)) {
            Ok(markdown) => Ok(Some(markdown)),
            Err(BriefError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// The brief to inject and display: the agent's own battlefield note when it
    /// has written one, otherwise the coverage-proven / template projection.
    pub fn read_effective(&self, agent: &AgentId) -> Result<String, BriefError> {
        match self.read_note(agent)? {
            Some(note) if !note.trim().is_empty() => Ok(note),
            _ => self.read(agent),
        }
    }

    fn write_projection(&self, path: &Path, bytes: &[u8]) -> Result<(), BriefError> {
        self.validate_projection_bytes(bytes.len())?;
        let _guard = self
            .inner
            .write_lock
            .lock()
            .map_err(|_| BriefError::LockPoisoned)?;
        atomic_replace(path, bytes).map_err(BriefError::Io)
    }

    fn read_projection(&self, path: &Path) -> Result<String, BriefError> {
        let max_bytes = self.max_projection_bytes();
        let file = File::open(path)?;
        let mut bytes = Vec::with_capacity(
            usize::try_from(file.metadata()?.len())
                .unwrap_or(max_bytes)
                .min(max_bytes),
        );
        file.take((max_bytes as u64).saturating_add(1))
            .read_to_end(&mut bytes)?;
        self.validate_projection_bytes(bytes.len())?;
        String::from_utf8(bytes).map_err(|error| {
            BriefError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
        })
    }

    fn validate_projection(&self, agent: &AgentId, markdown: &str) -> Result<(), BriefError> {
        validate_structure(agent, markdown)?;
        validate_knowledge(markdown)?;
        let actual_tokens = estimate_tokens(markdown);
        let max_tokens = self.inner.budget.brief_target_tokens();
        if actual_tokens > max_tokens {
            return Err(BriefError::TooLarge {
                actual_tokens,
                max_tokens,
            });
        }
        Ok(())
    }

    fn validate_projection_bytes(&self, actual: usize) -> Result<(), BriefError> {
        let max = self.max_projection_bytes();
        if actual > max {
            return Err(BriefError::TooManyBytes { actual, max });
        }
        Ok(())
    }

    fn max_projection_bytes(&self) -> usize {
        usize::try_from(self.inner.budget.brief_target_tokens().saturating_mul(4))
            .unwrap_or(usize::MAX)
            .min(MAX_PROJECTION_BYTES_LIMIT)
    }
}
