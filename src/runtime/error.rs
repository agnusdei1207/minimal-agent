use std::time::Duration;
use thiserror::Error;

use crate::brief::BriefError;
use crate::compaction::CompactionError;
use crate::coordinator::CoordinatorError;
use crate::domain::DomainError;
use crate::engagement::EngagementError;
use crate::journal::JournalError;
use crate::provider::ProviderFault;
use crate::tools::ToolError;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("runtime configuration is invalid")]
    InvalidConfig,
    #[error("runtime has stopped")]
    Stopped,
    #[error("agent turn was cancelled")]
    Cancelled,
    #[error("agent exceeded the configured model-turn limit")]
    ModelTurnLimit,
    #[error("runtime state lock was poisoned")]
    LockPoisoned,
    #[error("runtime did not become idle before the deadline")]
    IdleTimedOut,
    #[error("semantic compaction exceeded its total deadline of {0:?}")]
    CompactionTimedOut(Duration),
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Engagement(#[from] EngagementError),
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Coordinator(#[from] CoordinatorError),
    #[error(transparent)]
    Brief(#[from] BriefError),
    #[error(transparent)]
    Compaction(#[from] CompactionError),
    #[error(transparent)]
    Provider(ProviderFault),
    #[error(transparent)]
    Tool(#[from] ToolError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
