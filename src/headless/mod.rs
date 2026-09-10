pub mod constants;
pub mod observation;
pub mod runner;

pub use runner::run_headless;

#[cfg(test)]
mod tests {
    use super::observation::{HeadlessObservation, headless_journal_evidence};
    use super::runner::observe_headless;
    use async_trait::async_trait;
    use pentesting::domain::AgentId;
    use pentesting::engagement::Engagement;
    use pentesting::journal::{JournalConfig, JournalEvent, RunJournal};
    use pentesting::provider::{
        ModelDelta, ModelProvider, ModelRequest, ModelTurn, ProviderFault, ToolCall,
    };
    use pentesting::runtime::{RuntimeConfig, RuntimeEvent, TeamRuntime};
    use serde_json::json;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    struct HeadlessFixtureProvider {
        calls: AtomicUsize,
        pending: bool,
    }

    #[async_trait]
    impl ModelProvider for HeadlessFixtureProvider {
        fn context_limit(&self) -> u64 {
            128_000
        }
        async fn complete(
            &self,
            _: ModelRequest,
            deltas: Option<tokio::sync::mpsc::UnboundedSender<ModelDelta>>,
        ) -> Result<ModelTurn, ProviderFault> {
            if self.pending {
                std::future::pending::<()>().await;
            }
            let first = self.calls.fetch_add(1, Ordering::SeqCst) == 0;
            if !first && let Some(sender) = deltas {
                for _ in 0..4096 {
                    let _ = sender.send(ModelDelta::Text("x".to_owned()));
                }
            }
            Ok(ModelTurn {
                text: if first {
                    String::new()
                } else {
                    "flag{model-invented}".to_owned()
                },
                tool_calls: if first {
                    vec![ToolCall {
                        id: "read-fixture".to_owned(),
                        name: "workspace".to_owned(),
                        arguments: json!({"op":"read","path":"evidence.txt"}),
                    }]
                } else {
                    vec![]
                },
                usage: None,
                finish_reason: Some("stop".to_owned()),
            })
        }
    }

    async fn headless_fixture(directory: &Path, pending: bool, evidence: &str) -> TeamRuntime {
        let runtime = TeamRuntime::create(
            directory.join("run"),
            directory.join("workspace"),
            "read local evidence",
            Arc::new(HeadlessFixtureProvider {
                calls: AtomicUsize::new(0),
                pending,
            }),
            RuntimeConfig::default(),
        )
        .await
        .unwrap();
        std::fs::write(directory.join("workspace/evidence.txt"), evidence).unwrap();
        runtime
    }

    #[tokio::test]
    async fn headless_recovers_tool_flag_despite_a_broadcast_flood() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = headless_fixture(directory.path(), false, "flag{durable-evidence}").await;
        let engagement = Engagement {
            flag_format: Some(r"flag\{[^}]+\}".to_owned()),
            ..Default::default()
        };
        let observation = observe_headless(
            &runtime,
            "read local evidence",
            Some(&engagement),
            Duration::from_secs(3),
            Duration::from_millis(10),
            false,
        )
        .await
        .unwrap();
        runtime.shutdown().await;
        assert_eq!(observation.flag.as_deref(), Some("flag{durable-evidence}"));
    }

    #[tokio::test]
    async fn headless_wall_deadline_includes_the_initial_submission() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = headless_fixture(directory.path(), true, "unused").await;
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            observe_headless(
                &runtime,
                "read local evidence",
                None,
                Duration::from_millis(40),
                Duration::from_millis(10),
                false,
            ),
        )
        .await;
        runtime.shutdown().await;
        assert!(
            result.is_ok(),
            "the initial model call must obey the headless wall deadline"
        );
    }

    #[tokio::test]
    async fn headless_does_not_score_prior_run_or_model_only_flags() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = headless_fixture(directory.path(), false, "no flag in target output").await;
        runtime
            .coordinator()
            .journal()
            .append_sync(JournalEvent::ToolResult {
                agent_id: AgentId::main(),
                call_id: "prior-run".to_owned(),
                content: "flag{stale}".to_owned(),
                success: true,
            })
            .unwrap();
        let engagement = Engagement {
            flag_format: Some(r"flag\{[^}]+\}".to_owned()),
            ..Default::default()
        };
        let observation = observe_headless(
            &runtime,
            "read local evidence",
            Some(&engagement),
            Duration::from_secs(3),
            Duration::from_millis(10),
            false,
        )
        .await
        .unwrap();
        assert!(observation.flag.is_none());
        assert_eq!(observation.summary, "flag{model-invented}");
    }

    #[test]
    fn headless_durable_recovery_scans_long_history_and_respects_the_start_sequence() {
        let directory = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(directory.path(), JournalConfig::default()).unwrap();
        let old = journal
            .append_sync(JournalEvent::ToolResult {
                agent_id: AgentId::main(),
                call_id: "old".to_owned(),
                content: "flag{old}".to_owned(),
                success: true,
            })
            .unwrap();
        for _ in 0..260 {
            journal
                .append_sync(JournalEvent::Transcript {
                    agent_id: AgentId::main(),
                    role: pentesting::journal::TranscriptRole::Assistant,
                    content: "flag{invented}".to_owned(),
                    complete: true,
                    atomic_group: None,
                })
                .unwrap();
        }
        let last = journal
            .append_sync(JournalEvent::ToolResult {
                agent_id: AgentId::main(),
                call_id: "new".to_owned(),
                content: "flag{recovered}".to_owned(),
                success: true,
            })
            .unwrap();
        let engagement = Engagement {
            flag_format: Some(r"flag\{[^}]+\}".to_owned()),
            ..Default::default()
        };
        let flag = headless_journal_evidence(&journal, &engagement, old.sequence).unwrap();
        assert_eq!(flag.as_deref(), Some("flag{recovered}"));
        assert_eq!(
            headless_journal_evidence(&journal, &engagement, last.sequence).unwrap(),
            None
        );
    }

    #[test]
    fn headless_durable_recovery_skips_large_unrelated_blobs() {
        let directory = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(directory.path(), JournalConfig::default()).unwrap();
        for _ in 0..2 {
            journal
                .append_sync(JournalEvent::Transcript {
                    agent_id: AgentId::main(),
                    role: pentesting::journal::TranscriptRole::Assistant,
                    content: "x".repeat(5 * 1024 * 1024),
                    complete: true,
                    atomic_group: None,
                })
                .unwrap();
        }
        journal
            .append_sync(JournalEvent::ToolResult {
                agent_id: AgentId::main(),
                call_id: "large-history".to_owned(),
                content: "flag{after-large-records}".to_owned(),
                success: true,
            })
            .unwrap();
        let engagement = Engagement {
            flag_format: Some(r"flag\{[^}]+\}".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            headless_journal_evidence(&journal, &engagement, 0)
                .unwrap()
                .as_deref(),
            Some("flag{after-large-records}")
        );
    }

    #[test]
    fn headless_durable_recovery_skips_an_unrelated_record_above_the_response_budget() {
        let directory = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(directory.path(), JournalConfig::default()).unwrap();
        journal
            .append_sync(JournalEvent::Transcript {
                agent_id: AgentId::main(),
                role: pentesting::journal::TranscriptRole::Assistant,
                content: "x".repeat(9 * 1024 * 1024),
                complete: true,
                atomic_group: None,
            })
            .unwrap();
        journal
            .append_sync(JournalEvent::ToolResult {
                agent_id: AgentId::main(),
                call_id: "after-large-record".to_owned(),
                content: "flag{after-large-record}".to_owned(),
                success: true,
            })
            .unwrap();
        let engagement = Engagement {
            flag_format: Some(r"flag\{[^}]+\}".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            headless_journal_evidence(&journal, &engagement, 0)
                .unwrap()
                .as_deref(),
            Some("flag{after-large-record}")
        );
    }

    #[test]
    fn headless_live_events_cannot_score_broadcast_only_or_stale_tool_flags() {
        let mut observation = HeadlessObservation::default();
        observation.apply(RuntimeEvent::Assistant {
            agent_id: AgentId::main(),
            text: "flag{invented}".to_owned(),
        });
        assert!(observation.flag.is_none());

        observation.apply(RuntimeEvent::ToolFinished {
            agent_id: AgentId::main(),
            name: "shell".to_owned(),
            success: true,
            output: "queued before current submission: flag{old}".to_owned(),
        });
        assert!(observation.flag.is_none());
    }
}
