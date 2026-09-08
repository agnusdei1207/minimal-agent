pub mod budget;
pub mod constants;
pub mod error;
pub mod ids;
pub mod messages;
pub mod state;

pub use budget::{
    CompactionCoverage, ContextBudget, SequenceRange, estimate_tokens, range_is_covered,
    validate_bounded_text, validate_goal, validate_reason, validate_role, validate_task,
    validate_user_input,
};
pub use constants::*;
pub use error::DomainError;
pub use ids::{AgentId, InsightId};
pub use messages::{AgentMessage, Insight, InsightLabel, MessageKind};
pub use state::{AgentDepth, AgentState, TeamLimits};
