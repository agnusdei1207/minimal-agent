use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use super::error::ToolError;
use crate::brief::AgentBriefStore;
use crate::coordinator::AgentCoordinator;
use crate::domain::{AgentId, ContextBudget};
use crate::journal::RunJournal;

#[derive(Clone)]
pub struct ToolContext {
    pub agent_id: AgentId,
    pub workspace: PathBuf,
    pub coordinator: AgentCoordinator,
    pub journal: Arc<RunJournal>,
    pub briefs: AgentBriefStore,
    pub cancellation: CancellationToken,
}

impl ToolContext {
    /// Full constructor: shares the runtime's brief store so `brief` notes are
    /// visible to prompt-building and `/status`.
    pub fn with_briefs(
        agent_id: AgentId,
        workspace: impl AsRef<Path>,
        coordinator: AgentCoordinator,
        journal: Arc<RunJournal>,
        briefs: AgentBriefStore,
    ) -> Result<Self, ToolError> {
        std::fs::create_dir_all(workspace.as_ref())?;
        let workspace = workspace.as_ref().canonicalize()?;
        let cancellation = coordinator.cancellation_token(&agent_id)?;
        Ok(Self {
            agent_id,
            workspace,
            coordinator,
            journal,
            briefs,
            cancellation,
        })
    }

    /// Convenience constructor that pairs the context with a private brief store
    /// rooted under the workspace. Used where no shared store is threaded in
    /// (tool-level tests); the runtime uses `with_briefs` to share its own.
    pub fn new(
        agent_id: AgentId,
        workspace: impl AsRef<Path>,
        coordinator: AgentCoordinator,
        journal: Arc<RunJournal>,
    ) -> Result<Self, ToolError> {
        std::fs::create_dir_all(workspace.as_ref())?;
        let budget = ContextBudget::new(128_000, 128_000, 8_000)
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;
        let briefs =
            AgentBriefStore::new(workspace.as_ref().join(".briefs"), journal.clone(), budget);
        Self::with_briefs(agent_id, workspace, coordinator, journal, briefs)
    }
}
