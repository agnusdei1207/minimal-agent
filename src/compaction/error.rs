use thiserror::Error;

use crate::domain::DomainError;
use crate::provider::ProviderFault;

#[derive(Debug, Error)]
pub enum CompactionError {
    #[error("semantic compaction configuration is invalid")]
    InvalidConfig,
    #[error("semantic compaction is blocked; first: {first}; second: {second}")]
    ContextCompactionBlocked { first: String, second: String },
    #[error(transparent)]
    Provider(ProviderFault),
    #[error("failed to serialize compaction source: {message}")]
    Serialization { message: String },
    #[error("compaction coverage is invalid: {0}")]
    Coverage(#[from] DomainError),
}

#[derive(Debug)]
pub enum AttemptError {
    Provider(ProviderFault),
    Invalid(String),
}

impl From<ProviderFault> for AttemptError {
    fn from(value: ProviderFault) -> Self {
        Self::Provider(value)
    }
}
