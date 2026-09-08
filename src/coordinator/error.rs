use thiserror::Error;

use crate::domain::{AgentId, AgentState, DomainError};
use crate::journal::JournalError;

#[derive(Debug, Error)]
pub enum CoordinatorError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error("unknown agent: {0}")]
    UnknownAgent(AgentId),
    #[error("worker is inactive: {0}")]
    InactiveWorker(AgentId),
    #[error("agent is inactive: {0}")]
    InactiveAgent(AgentId),
    #[error("agent inbox is full: {0}")]
    InboxFull(AgentId),
    #[error("state {0:?} is not terminal")]
    NotTerminal(AgentState),
    #[error("main is the stable team root and cannot be marked terminal")]
    CannotTerminateMain,
    #[error("only main may perform this operation: {0}")]
    CallerNotMain(AgentId),
    #[error("state {0:?} is not a running/waiting execution state")]
    InvalidExecutionState(AgentState),
    #[error("coordinator state lock was poisoned")]
    LockPoisoned,
    #[error("run journal does not contain a main agent")]
    MissingMain,
    #[error("a new run requires an empty journal")]
    RunNotEmpty,
    #[error("waiting for an agent message timed out")]
    WaitTimedOut,
    #[error("journal replay violates coordinator invariants: {0}")]
    InvalidReplay(String),
}
