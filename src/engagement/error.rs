use super::constants::{MAX_ENGAGEMENT_TEXT_BYTES, MAX_OFF_LIMITS};

#[derive(Debug, thiserror::Error)]
pub enum EngagementError {
    #[error("unknown engagement kind '{0}'; expected ctf, pentest, or lab")]
    UnknownKind(String),
    #[error("engagement {field} exceeds {max} bytes")]
    TextTooLong { field: &'static str, max: usize },
    #[error("engagement off-limits list exceeds {MAX_OFF_LIMITS} entries")]
    TooManyOffLimits,
    #[error("rendered engagement context exceeds {MAX_ENGAGEMENT_TEXT_BYTES} bytes")]
    ContextTooLong,
    #[error("engagement {field} cannot be empty when set")]
    EmptyField { field: &'static str },
}
