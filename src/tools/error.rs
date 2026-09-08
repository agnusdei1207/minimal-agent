use std::time::Duration;
use thiserror::Error;

use crate::coordinator::CoordinatorError;
use crate::domain::DomainError;
use crate::journal::JournalError;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid tool arguments: {0}")]
    InvalidArguments(String),
    #[error("path is outside the workspace: {0}")]
    OutsideWorkspace(String),
    #[error("tool execution timed out after {0:?}")]
    TimedOut(Duration),
    #[error("tool output exceeded the {0}-byte stream limit")]
    OutputLimit(usize),
    #[error("tool arguments exceeded the {0}-byte input limit")]
    ArgumentLimit(usize),
    #[error("tool input is {actual} bytes; maximum direct read is {max}")]
    InputLimit { actual: u64, max: u64 },
    #[error("directory contains more than {0} entries")]
    EntryLimit(usize),
    #[error("tool process failed: {0}")]
    Process(String),
    #[error("tool execution was cancelled")]
    Cancelled,
    #[error("worker spawner failed: {0}")]
    Spawner(String),
    #[error(transparent)]
    Coordinator(#[from] CoordinatorError),
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
