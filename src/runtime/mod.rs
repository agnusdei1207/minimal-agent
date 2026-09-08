pub mod config;
pub mod constants;
pub mod engine;
pub mod error;
pub mod events;
pub mod recovery;
pub mod session;
pub mod spawner;
pub mod worker;

pub use config::RuntimeConfig;
pub use engine::TeamRuntime;
pub use error::RuntimeError;
pub use events::{RuntimeEvent, TurnResult};

#[cfg(test)]
mod tests {
    use super::session::{AgentSession, SessionRecord};
    use crate::compaction::{ContextEntry, LiveReason};
    use crate::domain::SequenceRange;
    use crate::provider::{ModelMessage, ModelRole};

    #[test]
    fn successful_model_observation_releases_only_recovery_protection() {
        let mut session = AgentSession {
            records: vec![
                record(1, LiveReason::PartialOutput),
                record(2, LiveReason::IncompleteTool),
                record(3, LiveReason::UnreadInbox),
            ],
            ..AgentSession::default()
        };

        session.release_recovery_protection();

        assert_eq!(session.records[0].context.live_reason, None);
        assert_eq!(session.records[1].context.live_reason, None);
        assert_eq!(
            session.records[2].context.live_reason,
            Some(LiveReason::UnreadInbox)
        );
    }

    fn record(sequence: u64, reason: LiveReason) -> SessionRecord {
        SessionRecord {
            message: ModelMessage::new(ModelRole::Assistant, "source"),
            context: ContextEntry::completed(
                SequenceRange::new(sequence, sequence).unwrap(),
                "source",
            )
            .protect(reason),
        }
    }
}
