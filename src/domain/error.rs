use thiserror::Error;

use super::state::AgentState;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("agent depth {0} exceeds the maximum team-tree depth")]
    InvalidDepth(u8),
    #[error("agent identifier is invalid: {0}")]
    InvalidIdentifier(String),
    #[error("the active team already has ten agents")]
    TeamFull,
    #[error("a leaf agent at the maximum depth cannot spawn children")]
    MaxDepthReached,
    #[error("invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition { from: AgentState, to: AgentState },
    #[error("message body cannot be empty")]
    EmptyMessage,
    #[error("message audience cannot be empty")]
    EmptyAudience,
    #[error("message recipients must be the sender's parent, children, or siblings")]
    NonNeighborAudience,
    #[error("message audience cannot contain duplicates")]
    DuplicateAudience,
    #[error("message audience exceeds the team limit of {max}")]
    AudienceTooLarge { max: usize },
    #[error("message payload is {actual} bytes; maximum is {max}")]
    MessageTooLarge { actual: usize, max: usize },
    #[error("{field} cannot be empty")]
    EmptyText { field: &'static str },
    #[error("{field} is {actual} bytes; maximum is {max}")]
    TextTooLarge {
        field: &'static str,
        actual: usize,
        max: usize,
    },
    #[error("insight messages require a typed insight")]
    MissingInsight,
    #[error("invalid sequence range {start}..={end}")]
    InvalidRange { start: u64, end: u64 },
    #[error("compaction coverage omits source data")]
    IncompleteCoverage,
    #[error("context budget must leave room after the response reserve")]
    InvalidContextBudget,
}
