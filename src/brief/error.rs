use thiserror::Error;

use crate::domain::{AgentId, DomainError};
use crate::journal::JournalError;

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
