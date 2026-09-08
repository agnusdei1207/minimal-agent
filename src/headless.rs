//! Non-interactive autonomous "headless" run for external injection (xbow-style
//! benchmarks, INTENT-0002 §3.3): submit the objective once, wait for the team to
//! settle, print a one-line JSON result, and reserve non-zero exits for crashes.

use std::time::Duration;

use serde_json::json;

use minimal_agent::domain::{AgentId, AgentState};
use minimal_agent::engagement::Engagement;
use minimal_agent::journal::{JournalEvent, JournalEventKind, RunJournal};
use minimal_agent::runtime::{RuntimeEvent, TeamRuntime};

const HEADLESS_MAX_WALL: Duration = Duration::from_secs(1800);
/// Idle-settle window before a headless run re-nudges, with and without auto.
const HEADLESS_IDLE_AUTO: Duration = Duration::from_secs(20);
const HEADLESS_IDLE_MANUAL: Duration = Duration::from_secs(2);
/// Max autonomous re-nudges of a waiting main agent in a headless run.
const MAX_HEADLESS_RETRIES: usize = 3;
/// Backoff before re-nudging a waiting main agent in a headless run.
const HEADLESS_RETRY_BACKOFF: Duration = Duration::from_secs(2);

pub(crate) async fn run_headless(
    runtime: TeamRuntime,
    goal: String,
    engagement: Option<Engagement>,
    auto: bool,
) -> anyhow::Result<()> {
    let idle_window = if auto {
        HEADLESS_IDLE_AUTO
    } else {
        HEADLESS_IDLE_MANUAL
    };

    let observation = observe_headless(
        &runtime,
        &goal,
        engagement.as_ref(),
        HEADLESS_MAX_WALL,
        idle_window,
        auto,
    )
    .await;
    runtime.shutdown().await;
    let observation = observation?;
    let flag = observation.flag;
    let flag_required = engagement
        .as_ref()
        .is_some_and(|engagement| engagement.flag_format.is_some());
    println!(
        "{}",
        serde_json::to_string(&json!({
            "goal": goal,
            "flag": flag,
            "flag_required": flag_required,
            "summary": observation.summary,
        }))?
    );

    // Exit 0 even when the flag was not captured: the agent completed its
    // exploration legitimately. The harness classifies exit(0) + no flag as
    // "unsolved" (valid_for_score: true), whereas exit(1) would be recorded
    // as "runtime_fault" (excluded), hiding genuine misses from the benchmark
    // denominator. Reserve non-zero exits for actual crashes / panics.
    Ok(())
}

#[derive(Default)]
struct HeadlessObservation {
    summary: String,
    flag: Option<String>,
}

async fn observe_headless(
    runtime: &TeamRuntime,
    goal: &str,
    engagement: Option<&Engagement>,
    max_wall: Duration,
    idle_window: Duration,
    auto: bool,
) -> anyhow::Result<HeadlessObservation> {
    let deadline = tokio::time::Instant::now() + max_wall;
    let mut events = runtime.subscribe();
    let journal = runtime.coordinator().journal();
    // A resumed run must not inherit an old flag as evidence for this submission.
    let prior_sequence = journal.latest_sequence()?;
    let mut observation = HeadlessObservation::default();
    let submission = runtime.submit_user(goal);
    tokio::pin!(submission);
    let mut submitted = false;
    let mut idle_deadline = tokio::time::Instant::now() + idle_window;
    let mut retry_count = 0_usize;
    loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(deadline) => break,
            turn = &mut submission, if !submitted => {
                submitted = true;
                match turn {
                    Ok(turn) if !turn.text.trim().is_empty() => observation.summary = turn.text.trim().to_owned(),
                    Ok(_) => {},
                    Err(error) => {
                        eprintln!("[headless] submission turn error: {error}");
                        if auto
                            && retry_count < MAX_HEADLESS_RETRIES
                            && runtime
                                .coordinator()
                                .inspect(&AgentId::main())
                                .is_ok_and(|agent| agent.state == AgentState::Waiting)
                        {
                            retry_count += 1;
                            eprintln!("[headless] retrying waiting main agent ({retry_count}/{MAX_HEADLESS_RETRIES})...");
                            tokio::time::sleep(HEADLESS_RETRY_BACKOFF).await;
                            let _ = runtime
                                .submit_user("Continue the goal from the latest brief and team inbox.")
                                .await;
                        }
                    }
                }
                idle_deadline = tokio::time::Instant::now() + idle_window;
            }
            event = events.recv() => {
                idle_deadline = tokio::time::Instant::now() + idle_window;
                match event {
                    Ok(event) => {
                        if minimal_agent::settings::debug_enabled() {
                            log_debug_event(&event);
                        }
                        if matches!(event, RuntimeEvent::TurnFinished { success: true, .. }) {
                            retry_count = 0;
                        }
                        observation.apply(event);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        if minimal_agent::settings::debug_enabled() {
                            eprintln!("[debug] broadcast lagged: {n} messages dropped");
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = tokio::time::sleep_until(idle_deadline), if submitted => {
                // Broadcast lag can lose TurnStarted/TurnFinished. Check the
                // runtime's active-turn counter before declaring the team idle.
                match tokio::time::timeout_at(deadline, runtime.wait_until_idle(Duration::from_millis(100))).await {
                    Ok(Ok(())) => {
                        if auto
                            && retry_count < MAX_HEADLESS_RETRIES
                            && runtime
                                .coordinator()
                                .inspect(&AgentId::main())
                                .is_ok_and(|agent| agent.state == AgentState::Waiting)
                        {
                            retry_count += 1;
                            eprintln!("[headless] main agent is waiting; retrying ({retry_count}/{MAX_HEADLESS_RETRIES})...");
                            tokio::time::sleep(HEADLESS_RETRY_BACKOFF).await;
                            let _ = runtime
                                .submit_user("Continue the goal from the latest brief and team inbox.")
                                .await;
                            idle_deadline = tokio::time::Instant::now() + idle_window;
                        } else {
                            break;
                        }
                    }
                    Err(_) => break,
                    Ok(Err(_)) => idle_deadline = tokio::time::Instant::now() + idle_window,
                }
            }
        }
    }
    // Stop writers before the final replay, including a submission still in
    // flight when the deadline fired. ToolResult is the durable evidence source.
    runtime.shutdown().await;
    if let Some(engagement) = engagement.filter(|value| value.flag_format.is_some()) {
        // Always use the watermark-filtered journal. A resumed worker can emit
        // a broadcast before the watermark is captured; that old event must not
        // become evidence for the new submission merely because it was queued.
        observation.flag = headless_journal_evidence(&journal, engagement, prior_sequence)?;
    }
    Ok(observation)
}

/// Bytes of a tool result / assistant snippet shown on one debug log line.
const LOG_SNIPPET_BYTES: usize = 400;
/// Bytes of a team-message body shown on one debug log line.
const LOG_MESSAGE_SNIPPET_BYTES: usize = 200;

/// Trim and cap a value for a single debug log line, noting the omitted length.
fn truncate_for_log(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.trim().to_owned();
    }
    let end = text
        .char_indices()
        .map(|(index, _)| index)
        .nth(max)
        .unwrap_or(text.len());
    format!(
        "{}... (truncated, total {} bytes)",
        text[..end].trim(),
        text.len()
    )
}

fn log_debug_event(event: &RuntimeEvent) {
    match event {
        RuntimeEvent::TurnStarted { agent_id } => {
            eprintln!("[debug] [{agent_id}] >>> Turn Started");
        }
        RuntimeEvent::TurnFinished { agent_id, success } => {
            eprintln!("[debug] [{agent_id}] <<< Turn Finished (success={success})");
        }
        RuntimeEvent::ToolStarted {
            agent_id,
            name,
            summary,
        } => {
            let target = summary.as_deref().unwrap_or("-");
            eprintln!("[debug] [{agent_id}] Tool Call: {name} | target: {target}");
        }
        RuntimeEvent::ToolFinished {
            agent_id,
            name,
            success,
            output,
        } => {
            let snippet = truncate_for_log(output, LOG_SNIPPET_BYTES);
            eprintln!("[debug] [{agent_id}] Tool Result: {name} (success={success}) -> {snippet}");
        }
        RuntimeEvent::Assistant { agent_id, text } => {
            let snippet = truncate_for_log(text, LOG_SNIPPET_BYTES);
            eprintln!("[debug] [{agent_id}] Assistant: {snippet}");
        }
        RuntimeEvent::AgentMessage {
            sender,
            recipients,
            kind,
            body,
        } => {
            let recips: Vec<String> = recipients.iter().map(|r| r.to_string()).collect();
            // Preserves the original message-line format (no total-bytes note).
            let snippet = if body.len() > LOG_MESSAGE_SNIPPET_BYTES {
                let end = body
                    .char_indices()
                    .map(|(index, _)| index)
                    .nth(LOG_MESSAGE_SNIPPET_BYTES)
                    .unwrap_or(body.len());
                format!("{}...", body[..end].trim())
            } else {
                body.trim().to_string()
            };
            eprintln!(
                "[debug] [{sender}] Msg to {:?} ({kind:?}): {snippet}",
                recips
            );
        }
        RuntimeEvent::Fault { agent_id, message } => {
            eprintln!("[debug] [{agent_id}] FAULT: {message}");
        }
        RuntimeEvent::Delta { .. } | RuntimeEvent::TeamChanged => {}
    }
}

impl HeadlessObservation {
    fn apply(&mut self, event: RuntimeEvent) {
        match event {
            RuntimeEvent::Assistant { text, .. } if !text.trim().is_empty() => {
                self.summary = text.trim().to_owned();
            }
            _ => {}
        }
    }
}

fn headless_journal_evidence(
    journal: &RunJournal,
    engagement: &Engagement,
    after: u64,
) -> anyhow::Result<Option<String>> {
    const MAX_TOOL_EVENT_BYTES: usize = 8 * 1024 * 1024;
    let mut flag = None;
    journal.visit_kind_after(
        after,
        JournalEventKind::ToolResult,
        MAX_TOOL_EVENT_BYTES,
        |entry| {
            if flag.is_none()
                && let JournalEvent::ToolResult { content, .. } = entry.event
            {
                flag = engagement.extract_flag(&content);
            }
        },
    )?;
    Ok(flag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use minimal_agent::journal::JournalConfig;
    use minimal_agent::provider::{
        ModelDelta, ModelProvider, ModelRequest, ModelTurn, ProviderFault, ToolCall,
    };
    use minimal_agent::runtime::RuntimeConfig;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
                    role: minimal_agent::journal::TranscriptRole::Assistant,
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
                    role: minimal_agent::journal::TranscriptRole::Assistant,
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
                role: minimal_agent::journal::TranscriptRole::Assistant,
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
