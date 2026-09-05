use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use ma_coordinator::AgentSnapshot;
use ma_core::domain::{
    AgentId, CompactionCoverage, ContextBudget, DomainError, InsightId, SequenceRange,
    estimate_tokens,
};
use ma_journal::{JournalError, JournalEvent, RunJournal};

const RUNTIME_START: &str = "<!-- minimal-agent:runtime:start -->";
const RUNTIME_END: &str = "<!-- minimal-agent:runtime:end -->";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BriefDraft {
    pub markdown: String,
    pub source_sha256: String,
    pub source_ranges: Vec<SequenceRange>,
    pub coverage: CompactionCoverage,
}

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
            .join("agents")
            .join(agent.as_str())
            .join("brief.md")
    }

    /// The agent-authored battlefield note (see `write_note`). Kept beside the
    /// coverage-proven brief projection so neither overwrites the other.
    fn note_path(&self, agent: &AgentId) -> PathBuf {
        self.inner
            .root
            .join("agents")
            .join(agent.as_str())
            .join("battlefield.md")
    }

    /// Record the agent's own battlefield note. Unlike `commit`, this carries no
    /// coverage proof and imposes no template structure, so a weak model can
    /// always keep its strategy current (the `brief` tool, ADR-0001 §9.1). Only
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
            .min(4 * 1_024 * 1_024)
    }
}

fn render_main_template(team: &[AgentSnapshot]) -> String {
    // Seed the battlefield root with the actual objective so the note is
    // meaningful from turn zero, even before the model writes to it. An empty
    // placeholder after real work reads as broken; the goal read as the root of
    // the arc tree does not.
    let goal_root = team
        .iter()
        .find(|agent| agent.id.is_main())
        .map(|agent| agent.task.trim())
        .filter(|task| !task.is_empty())
        .unwrap_or("Goal");
    format!(
        "# Main Agent Brief\n{}\n\
## Goal & Constraints\nNo goal refinement yet\n\
## Battlefield\n{goal_root}\n└─ No active arc yet\n\
## Curated Knowledge\n\
### Facts & Successes\nNo durable insight yet\n\
### Hypotheses & Directions\nNo durable insight yet\n\
### Dead Ends\nNo durable insight yet\n\
## Blockers\nNone\n\
## Next Moves\nAssign the first useful task\n",
        render_runtime_block(team)
    )
}

fn render_worker_template(agent: &AgentSnapshot) -> String {
    format!(
        "# Worker Agent Brief\n\
## Assignment\n{}\n\
## Current State\n{}\n\
## Attempts by Domain\nNo attempts yet\n\
## Curated Knowledge\n\
### Facts & Successes\nNo durable insight yet\n\
### Hypotheses & Directions\nNo durable insight yet\n\
### Dead Ends\nNo durable insight yet\n\
## Integrated Messages\nNone\n\
## Blockers\nNone\n\
## Next Move\nStart the assigned task\n",
        agent.task,
        state_name(agent.state)
    )
}

fn render_runtime_block(team: &[AgentSnapshot]) -> String {
    let mut team = team.to_vec();
    team.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
    let mut output = String::from(RUNTIME_START);
    output.push_str(
        "\n## Team\n| ID | Role | Current Task | State | Latest Insight | Waiting On |\n",
    );
    output.push_str("|---|---|---|---|---|---|\n");
    for agent in team {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            agent.id,
            table_cell(&agent.role),
            table_cell(&agent.task),
            state_name(agent.state),
            table_cell(agent.latest_insight.as_deref().unwrap_or("")),
            table_cell(agent.waiting_on.as_deref().unwrap_or("")),
        ));
    }
    output.push_str(RUNTIME_END);
    output
}

fn replace_runtime_block(current: &str, runtime: &str) -> Result<String, BriefError> {
    validate_runtime_block(current)?;
    let start = current
        .find(RUNTIME_START)
        .ok_or(BriefError::MissingRuntimeBlock)?;
    let relative_end = current[start..]
        .find(RUNTIME_END)
        .ok_or(BriefError::MissingRuntimeBlock)?;
    let end = start + relative_end + RUNTIME_END.len();
    let mut output = String::with_capacity(current.len() + runtime.len());
    output.push_str(&current[..start]);
    output.push_str(runtime);
    output.push_str(&current[end..]);
    Ok(output)
}

fn validate_structure(agent: &AgentId, markdown: &str) -> Result<(), BriefError> {
    let required: &[&str] = if agent.is_main() {
        &[
            "# Main Agent Brief",
            "## Goal & Constraints",
            "## Battlefield",
            "## Curated Knowledge",
            "### Facts & Successes",
            "### Hypotheses & Directions",
            "### Dead Ends",
            "## Blockers",
            "## Next Moves",
        ]
    } else {
        &[
            "# Worker Agent Brief",
            "## Assignment",
            "## Current State",
            "## Attempts by Domain",
            "## Curated Knowledge",
            "### Facts & Successes",
            "### Hypotheses & Directions",
            "### Dead Ends",
            "## Integrated Messages",
            "## Blockers",
            "## Next Move",
        ]
    };
    if markdown.trim().is_empty() || required.iter().any(|heading| !markdown.contains(heading)) {
        return Err(BriefError::InvalidStructure);
    }
    if agent.is_main() {
        validate_runtime_block(markdown)?;
    }
    Ok(())
}

fn validate_runtime_block(markdown: &str) -> Result<(), BriefError> {
    if markdown.matches(RUNTIME_START).count() != 1 || markdown.matches(RUNTIME_END).count() != 1 {
        return Err(BriefError::MissingRuntimeBlock);
    }
    let start = markdown
        .find(RUNTIME_START)
        .ok_or(BriefError::MissingRuntimeBlock)?;
    let end = markdown
        .find(RUNTIME_END)
        .ok_or(BriefError::MissingRuntimeBlock)?;
    if start >= end {
        return Err(BriefError::MissingRuntimeBlock);
    }
    Ok(())
}

fn validate_knowledge(markdown: &str) -> Result<(), BriefError> {
    let mut section = "";
    for line in markdown.lines() {
        if line.starts_with("### ") {
            section = line;
            continue;
        }
        let trimmed = line.trim_start();
        if !trimmed.starts_with("- [") {
            continue;
        }
        let label_end = trimmed
            .find(']')
            .ok_or_else(|| BriefError::InvalidKnowledge(line.to_owned()))?;
        let label = &trimmed[3..label_end];
        let valid_label = matches!(
            label,
            "FACT" | "HYPOTHESIS" | "DIRECTION" | "SUCCESS" | "DEAD_END" | "BLOCKER"
        );
        if !valid_label
            || (section.contains("Hypotheses") && matches!(label, "FACT" | "SUCCESS"))
            || (section.contains("Facts") && matches!(label, "HYPOTHESIS" | "DIRECTION"))
            || (matches!(label, "FACT" | "SUCCESS") && !trimmed.contains("journal:"))
            || (label == "DEAD_END" && !trimmed.contains("reason:"))
        {
            return Err(BriefError::InvalidKnowledge(line.to_owned()));
        }
    }
    Ok(())
}

fn validate_source_metadata(
    source_sha256: &str,
    source_ranges: &[SequenceRange],
) -> Result<(), BriefError> {
    let digest_valid =
        source_sha256.len() == 64 && source_sha256.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !digest_valid || source_ranges.is_empty() {
        return Err(BriefError::InvalidSourceMetadata);
    }
    Ok(())
}

fn state_name(state: ma_core::domain::AgentState) -> &'static str {
    match state {
        ma_core::domain::AgentState::Running => "RUNNING",
        ma_core::domain::AgentState::Waiting => "WAITING",
        ma_core::domain::AgentState::Recalling => "RECALLING",
        ma_core::domain::AgentState::Finished => "FINISHED",
        ma_core::domain::AgentState::Stopped => "STOPPED",
        ma_core::domain::AgentState::Faulted => "FAULTED",
    }
}

fn table_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace(['\r', '\n'], " ")
        .trim()
        .to_owned()
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "brief path has no parent")
    })?;
    fs::create_dir_all(parent)?;
    let backup = parent.join(".brief.backup");
    if backup.exists() && !path.exists() {
        fs::rename(&backup, path)?;
    } else if backup.exists() {
        fs::remove_file(&backup)?;
    }

    let temp = parent.join(format!(".brief.{}.tmp", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()?;
    drop(file);

    let had_previous = path.exists();
    if had_previous {
        fs::rename(path, &backup)?;
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        if had_previous {
            let _ = fs::rename(&backup, path);
        }
        return Err(error);
    }
    if had_previous {
        fs::remove_file(&backup)?;
    }
    sync_directory(parent)?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[derive(Debug, Error)]
pub enum BriefError {
    #[error("agent {actor} cannot curate {target}'s brief")]
    Ownership { actor: AgentId, target: AgentId },
    #[error("brief is missing required sections")]
    InvalidStructure,
    #[error("brief knowledge entry is invalid: {0}")]
    InvalidKnowledge(String),
    #[error("brief source metadata is invalid")]
    InvalidSourceMetadata,
    #[error("brief coverage is invalid: {0}")]
    Coverage(DomainError),
    #[error("brief uses about {actual_tokens} tokens; target is {max_tokens}")]
    TooLarge { actual_tokens: u64, max_tokens: u64 },
    #[error("brief projection is {actual} bytes; maximum is {max}")]
    TooManyBytes { actual: usize, max: usize },
    #[error("brief does not contain its runtime projection block")]
    MissingRuntimeBlock,
    #[error("brief checkpoint digest mismatch for {0}")]
    CheckpointDigest(AgentId),
    #[error("brief store lock was poisoned")]
    LockPoisoned,
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
