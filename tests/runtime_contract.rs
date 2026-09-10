use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use pentesting::domain::{
    AgentId, AgentState, DomainError, Insight, InsightId, InsightLabel, MAX_USER_INPUT_BYTES,
    MessageKind,
};
use pentesting::engagement::{Engagement, EngagementKind};
use pentesting::journal::JournalEvent;
use pentesting::provider::{ModelProvider, ModelRequest, ModelTurn, ProviderFault, ToolCall};
use pentesting::runtime::{RuntimeConfig, RuntimeError, RuntimeEvent, TeamRuntime};
use sha2::{Digest, Sha256};
use tempfile::tempdir;
use tokio::sync::Notify;

#[derive(Clone)]
struct Gate {
    started: Arc<Notify>,
    release: Arc<Notify>,
}

enum Script {
    Panic,
    Turn { turn: ModelTurn, delay_ms: u64 },
    Gated { gate: Gate, turn: ModelTurn },
    Fault(ProviderFault),
}

struct ScriptedProvider {
    scripts: Mutex<HashMap<String, VecDeque<Script>>>,
    requests: Mutex<Vec<(String, ModelRequest)>>,
    active: AtomicUsize,
    max_active: AtomicUsize,
}

struct HangingCompactionProvider;

struct SuccessfulMainCompactionProvider;

#[derive(Default)]
struct RetainingDeltaProvider {
    senders: Mutex<Vec<tokio::sync::mpsc::UnboundedSender<pentesting::provider::ModelDelta>>>,
}

#[async_trait]
impl ModelProvider for RetainingDeltaProvider {
    fn context_limit(&self) -> u64 {
        20_000
    }

    async fn complete(
        &self,
        _request: ModelRequest,
        deltas: Option<tokio::sync::mpsc::UnboundedSender<pentesting::provider::ModelDelta>>,
    ) -> Result<ModelTurn, ProviderFault> {
        if let Some(sender) = deltas {
            let _ = sender.send(pentesting::provider::ModelDelta::Text(
                "complete despite retained sender".to_owned(),
            ));
            self.senders.lock().unwrap().push(sender);
        }
        Ok(text_turn("complete despite retained sender"))
    }
}

#[async_trait]
impl ModelProvider for HangingCompactionProvider {
    fn context_limit(&self) -> u64 {
        20_000
    }

    async fn complete(
        &self,
        request: ModelRequest,
        _deltas: Option<tokio::sync::mpsc::UnboundedSender<pentesting::provider::ModelDelta>>,
    ) -> Result<ModelTurn, ProviderFault> {
        if request.tools_enabled {
            Ok(text_turn("normal turn"))
        } else {
            std::future::pending::<Result<ModelTurn, ProviderFault>>().await
        }
    }
}

#[async_trait]
impl ModelProvider for SuccessfulMainCompactionProvider {
    fn context_limit(&self) -> u64 {
        20_000
    }

    async fn complete(
        &self,
        request: ModelRequest,
        _deltas: Option<tokio::sync::mpsc::UnboundedSender<pentesting::provider::ModelDelta>>,
    ) -> Result<ModelTurn, ProviderFault> {
        if request.tools_enabled {
            return Ok(text_turn("normal turn"));
        }
        let input: serde_json::Value =
            serde_json::from_str(&request.messages.last().unwrap().content).unwrap();
        let current = input["current_brief"].as_str().unwrap();
        let runtime_start = current
            .find("<!-- minimal-agent:runtime:start -->")
            .unwrap();
        let runtime_end = current.find("<!-- minimal-agent:runtime:end -->").unwrap()
            + "<!-- minimal-agent:runtime:end -->".len();
        let runtime = &current[runtime_start..runtime_end];
        let markdown = format!(
            "# Main Agent Brief\n{runtime}\n\
## Goal & Constraints\nbound compaction\n\
## Battlefield\none active arc\n\
## Curated Knowledge\n\
### Facts & Successes\nNo durable insight yet\n\
### Hypotheses & Directions\nNo durable insight yet\n\
### Dead Ends\nNo durable insight yet\n\
## Blockers\nNone\n\
## Next Moves\nContinue\n"
        );
        Ok(text_turn(
            &serde_json::json!({
                "markdown": markdown,
                "source_sha256": input["source_sha256"],
                "covered_ranges": input["required_ranges"],
                "covered_insight_ids": input["required_insight_ids"],
                "superseded_insight_ids": [],
            })
            .to_string(),
        ))
    }
}

impl ScriptedProvider {
    fn new(scripts: HashMap<String, VecDeque<Script>>) -> Self {
        Self {
            scripts: Mutex::new(scripts),
            requests: Mutex::new(Vec::new()),
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
        }
    }

    fn requests_for(&self, agent: &str) -> Vec<ModelRequest> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|(id, _)| id == agent)
            .map(|(_, request)| request.clone())
            .collect()
    }

    fn max_active(&self) -> usize {
        self.max_active.load(Ordering::Acquire)
    }
}

#[async_trait]
impl ModelProvider for ScriptedProvider {
    fn context_limit(&self) -> u64 {
        20_000
    }

    async fn complete(
        &self,
        request: ModelRequest,
        deltas: Option<tokio::sync::mpsc::UnboundedSender<pentesting::provider::ModelDelta>>,
    ) -> Result<ModelTurn, ProviderFault> {
        let system = &request.messages[0].content;
        let agent = system
            .lines()
            .find_map(|line| line.strip_prefix("agent_id: "))
            .unwrap()
            .to_owned();
        self.requests.lock().unwrap().push((agent.clone(), request));
        let script = self
            .scripts
            .lock()
            .unwrap()
            .get_mut(&agent)
            .and_then(VecDeque::pop_front)
            .unwrap_or_else(|| Script::Turn {
                turn: text_turn("waiting"),
                delay_ms: 0,
            });
        let active = self.active.fetch_add(1, Ordering::AcqRel) + 1;
        self.max_active.fetch_max(active, Ordering::AcqRel);
        let result = match script {
            Script::Panic => panic!("worker fixture panic"),
            Script::Turn { turn, delay_ms } => {
                if delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                Ok(turn)
            }
            Script::Gated { gate, turn } => {
                gate.started.notify_one();
                gate.release.notified().await;
                Ok(turn)
            }
            Script::Fault(error) => Err(error),
        };
        self.active.fetch_sub(1, Ordering::AcqRel);
        if let (Ok(turn), Some(sender)) = (&result, deltas)
            && !turn.text.is_empty()
        {
            let _ = sender.send(pentesting::provider::ModelDelta::Text(turn.text.clone()));
        }
        result
    }
}

fn text_turn(text: &str) -> ModelTurn {
    ModelTurn {
        text: text.into(),
        tool_calls: vec![],
        usage: None,
        finish_reason: Some("stop".into()),
    }
}

fn without_runtime_projection(markdown: &str) -> String {
    const START: &str = "<!-- minimal-agent:runtime:start -->";
    const END: &str = "<!-- minimal-agent:runtime:end -->";
    let start = markdown.find(START).unwrap();
    let end = markdown.find(END).unwrap() + END.len();
    format!(
        "{}<runtime-projection>{}",
        &markdown[..start],
        &markdown[end..]
    )
}

fn tool_turn(calls: Vec<(&str, &str, serde_json::Value)>) -> ModelTurn {
    ModelTurn {
        text: String::new(),
        tool_calls: calls
            .into_iter()
            .map(|(id, name, arguments)| ToolCall {
                id: id.into(),
                name: name.into(),
                arguments,
            })
            .collect(),
        usage: None,
        finish_reason: Some("tool_calls".into()),
    }
}

fn runtime_config() -> RuntimeConfig {
    RuntimeConfig {
        configured_context_tokens: 20_000,
        reserved_response_tokens: 2_000,
        max_model_turns: 8,
        max_parallel_requests: 4,
        tool_timeout: Duration::from_secs(3),
        auto: false,
        ..RuntimeConfig::default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn main_spawns_independent_workers_and_their_results_converge() {
    let mut scripts = HashMap::new();
    scripts.insert(
        "main".into(),
        VecDeque::from(vec![
            Script::Turn {
                turn: tool_turn(vec![
                    (
                        "create-a",
                        "team",
                        serde_json::json!({"op":"create","role":"recon","task":"map service"}),
                    ),
                    (
                        "create-b",
                        "team",
                        serde_json::json!({"op":"create","role":"analysis","task":"check lead"}),
                    ),
                ]),
                delay_ms: 0,
            },
            Script::Turn {
                turn: text_turn("workers dispatched"),
                delay_ms: 100,
            },
        ]),
    );
    scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![Script::Turn {
            turn: tool_turn(vec![
                (
                    "share",
                    "team",
                    serde_json::json!({
                        "op":"send","to":["worker-02"],"kind":"insight",
                        "body":"route discovered",
                        "insight":{"id":"I-route","label":"FACT","text":"/health returned 200"}
                    }),
                ),
                (
                    "finish-a",
                    "team",
                    serde_json::json!({"op":"finish","body":"recon complete"}),
                ),
            ]),
            delay_ms: 40,
        }]),
    );
    scripts.insert(
        "worker-02".into(),
        VecDeque::from(vec![Script::Turn {
            turn: tool_turn(vec![(
                "finish-b",
                "team",
                serde_json::json!({"op":"finish","body":"analysis complete"}),
            )]),
            delay_ms: 40,
        }]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let mut config = runtime_config();
    config.auto = true;
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "solve target",
        provider.clone(),
        config,
    )
    .await
    .unwrap();
    let response = runtime.submit_user("begin").await.unwrap();
    assert_eq!(response.text, "workers dispatched");
    runtime
        .wait_until_idle(Duration::from_secs(3))
        .await
        .unwrap();

    let coordinator = runtime.coordinator();
    assert_eq!(
        coordinator
            .inspect(&AgentId::new("worker-01").unwrap())
            .unwrap()
            .state,
        AgentState::Finished
    );
    assert_eq!(
        coordinator
            .inspect(&AgentId::new("worker-02").unwrap())
            .unwrap()
            .state,
        AgentState::Finished
    );
    let final_messages = coordinator
        .journal()
        .replay()
        .unwrap()
        .into_iter()
        .filter_map(|entry| match entry.event {
            JournalEvent::AgentMessage { message } if message.kind == MessageKind::Final => {
                Some(message)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(final_messages.len(), 2);
    assert!(coordinator.journal().replay().unwrap().iter().any(|entry| {
        match &entry.event {
            JournalEvent::AgentMessage { message } => message
                .insight
                .as_ref()
                .is_some_and(|insight| insight.id.as_str() == "I-route"),
            _ => false,
        }
    }));
    assert!(provider.max_active() >= 2);
    assert!(!dir.path().join("run/agents/worker-01/brief.md").exists());
    assert!(!dir.path().join("run/agents/worker-02/brief.md").exists());
    assert!(
        runtime
            .brief(&AgentId::new("worker-01").unwrap())
            .unwrap()
            .contains("no live brief projection")
    );
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queued_user_input_wins_over_auto_continuation() {
    let mut scripts = HashMap::new();
    scripts.insert(
        "main".into(),
        VecDeque::from(vec![
            Script::Turn {
                turn: text_turn("first done"),
                delay_ms: 80,
            },
            Script::Turn {
                turn: text_turn("second done"),
                delay_ms: 0,
            },
            Script::Turn {
                turn: text_turn("auto done"),
                delay_ms: 0,
            },
        ]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let mut config = runtime_config();
    config.auto = true;
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider.clone(),
        config,
    )
    .await
    .unwrap();
    let first_runtime = runtime.clone();
    let first = tokio::spawn(async move { first_runtime.submit_user("USER-FIRST").await.unwrap() });
    tokio::time::sleep(Duration::from_millis(10)).await;
    let second_runtime = runtime.clone();
    let second =
        tokio::spawn(async move { second_runtime.submit_user("USER-SECOND").await.unwrap() });
    first.await.unwrap();
    second.await.unwrap();
    runtime.set_auto(false);
    let requests = provider.requests_for("main");
    let flattened = requests
        .iter()
        .map(|request| {
            request
                .messages
                .iter()
                .map(|message| message.content.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect::<Vec<_>>();
    assert!(flattened[0].contains("USER-FIRST"));
    assert!(flattened[1].contains("USER-SECOND"));
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn message_during_provider_stream_is_seen_at_the_next_safe_boundary() {
    let gate = Gate {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let mut scripts = HashMap::new();
    scripts.insert("main".into(), VecDeque::from(vec![text_script("idle")]));
    scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![
            Script::Gated {
                gate: gate.clone(),
                turn: text_turn("first boundary complete"),
            },
            Script::Turn {
                turn: tool_turn(vec![(
                    "finish",
                    "team",
                    serde_json::json!({"op":"finish","body":"message integrated"}),
                )]),
                delay_ms: 0,
            },
        ]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let mut config = runtime_config();
    config.auto = true;
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider.clone(),
        config,
    )
    .await
    .unwrap();
    let worker = runtime
        .spawn_worker(&AgentId::main(), "worker", "initial assignment")
        .await
        .unwrap();
    gate.started.notified().await;
    runtime
        .coordinator()
        .send(
            AgentId::main(),
            vec![worker.clone()],
            MessageKind::Progress,
            "MID-STREAM-MESSAGE",
            None,
        )
        .unwrap();
    assert!(
        !runtime
            .coordinator()
            .cancellation_token(&worker)
            .unwrap()
            .is_cancelled()
    );
    gate.release.notify_waiters();
    runtime
        .wait_until_idle(Duration::from_secs(3))
        .await
        .unwrap();
    let requests = provider.requests_for(worker.as_str());
    assert!(
        !requests[0]
            .messages
            .iter()
            .any(|message| message.content.contains("MID-STREAM-MESSAGE"))
    );
    assert!(
        requests[1]
            .messages
            .iter()
            .any(|message| message.content.contains("MID-STREAM-MESSAGE"))
    );
    runtime.shutdown().await;
}

fn text_script(text: &str) -> Script {
    Script::Turn {
        turn: text_turn(text),
        delay_ms: 0,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn restart_restores_graph_unread_inbox_brief_and_live_context_without_replaying_tools() {
    let mut first_scripts = HashMap::new();
    first_scripts.insert(
        "main".into(),
        VecDeque::from(vec![text_script("BEFORE-CRASH")]),
    );
    first_scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![text_script("worker parked")]),
    );
    let first_provider = Arc::new(ScriptedProvider::new(first_scripts));
    let dir = tempdir().unwrap();
    let run_root = dir.path().join("run");
    let workspace = dir.path().join("workspace");
    let first = TeamRuntime::create(
        &run_root,
        &workspace,
        "durable goal",
        first_provider,
        runtime_config(),
    )
    .await
    .unwrap();
    first.submit_user("PRE-RESTART-INPUT").await.unwrap();
    let worker = first
        .spawn_worker(&AgentId::main(), "research", "hold context")
        .await
        .unwrap();
    first.wait_until_idle(Duration::from_secs(2)).await.unwrap();
    first
        .coordinator()
        .send(
            AgentId::main(),
            vec![worker.clone()],
            MessageKind::Progress,
            "UNREAD-ACROSS-RESTART",
            None,
        )
        .unwrap();
    let journal = first.coordinator().journal();
    journal
        .append_sync(JournalEvent::ToolCall {
            agent_id: AgentId::main(),
            call_id: "interrupted-write".into(),
            name: "workspace".into(),
            arguments: serde_json::json!({
                "op":"write",
                "path":"must-not-exist.txt",
                "content":"side effect"
            }),
        })
        .unwrap();
    let mut markdown = std::fs::read_to_string(run_root.join("agents/main/brief.md")).unwrap();
    markdown = markdown.replace("No goal refinement yet", "CURATED-ACROSS-RESTART");
    let markdown_sha256 = hex::encode(Sha256::digest(markdown.as_bytes()));
    journal
        .append_sync(JournalEvent::BriefCheckpoint {
            agent_id: AgentId::main(),
            markdown,
            markdown_sha256,
            source_sha256: "a".repeat(64),
            source_ranges: vec![pentesting::domain::SequenceRange::new(1, 1).unwrap()],
            coverage: pentesting::domain::CompactionCoverage {
                covered_ranges: vec![pentesting::domain::SequenceRange::new(1, 1).unwrap()],
                covered_insight_ids: vec![],
                superseded_insight_ids: vec![],
            },
        })
        .unwrap();
    drop(journal);
    first.shutdown().await;
    drop(first);
    std::fs::remove_file(run_root.join("agents/main/brief.md")).unwrap();

    let mut resumed_scripts = HashMap::new();
    resumed_scripts.insert("main".into(), VecDeque::from(vec![text_script("resumed")]));
    let resumed_provider = Arc::new(ScriptedProvider::new(resumed_scripts));
    let resumed = TeamRuntime::resume(
        &run_root,
        &workspace,
        resumed_provider.clone(),
        runtime_config(),
    )
    .await
    .unwrap();

    assert_eq!(
        resumed.coordinator().inspect(&worker).unwrap().state,
        AgentState::Running
    );
    let inbox = resumed.coordinator().inbox(&worker).unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].message.body, "UNREAD-ACROSS-RESTART");
    assert!(
        resumed
            .brief(&AgentId::main())
            .unwrap()
            .contains("CURATED-ACROSS-RESTART")
    );

    resumed.submit_user("AFTER-RESTART-INPUT").await.unwrap();
    let request = resumed_provider.requests_for("main").pop().unwrap();
    let request_text = request
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(request_text.contains("PRE-RESTART-INPUT"));
    assert!(request_text.contains("CURATED-ACROSS-RESTART"));
    assert!(request_text.contains("was not replayed"));
    assert!(!workspace.join("must-not-exist.txt").exists());
    resumed.shutdown().await;
}

fn fault_script() -> Script {
    Script::Fault(ProviderFault::Upstream {
        status: 502,
        message: "upstream".into(),
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn provider_cannot_hold_the_runtime_delta_collector_open_after_returning() {
    let provider = Arc::new(RetainingDeltaProvider::default());
    let dir = tempdir().unwrap();
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider,
        runtime_config(),
    )
    .await
    .unwrap();

    let result = tokio::time::timeout(
        Duration::from_millis(250),
        runtime.submit_user("do one turn"),
    )
    .await;
    runtime.shutdown().await;

    assert!(
        result.is_ok(),
        "a provider-retained delta sender kept the completed turn open"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn auto_off_holds_a_new_worker_until_auto_is_enabled() {
    let gate = Gate {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let mut scripts = HashMap::new();
    scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![Script::Gated {
            gate: gate.clone(),
            turn: text_turn("started after auto"),
        }]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider.clone(),
        runtime_config(),
    )
    .await
    .unwrap();
    runtime
        .spawn_worker(&AgentId::main(), "worker", "held assignment")
        .await
        .unwrap();

    assert!(
        tokio::time::timeout(Duration::from_millis(150), gate.started.notified())
            .await
            .is_err()
    );
    assert!(provider.requests_for("worker-01").is_empty());

    runtime.set_auto(true);
    tokio::time::timeout(Duration::from_secs(2), gate.started.notified())
        .await
        .unwrap();
    gate.release.notify_waiters();
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn auto_drives_the_goal_turn_after_turn_until_a_turn_finalizes_it() {
    let mut scripts = HashMap::new();
    scripts.insert(
        "main".into(),
        VecDeque::from(vec![
            text_script("begin"),
            text_script("progress 1"),
            Script::Turn {
                turn: tool_turn(vec![(
                    "final",
                    "report",
                    serde_json::json!({"op": "final", "body": "objective complete"}),
                )]),
                delay_ms: 0,
            },
            // Must NOT run: the loop stops once a turn finalizes the goal.
            text_script("after finalize"),
        ]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let mut config = runtime_config();
    config.auto = true;
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider.clone(),
        config,
    )
    .await
    .unwrap();

    // The initial user turn kicks off the autonomous loop, which then continues
    // on its own: begin -> progress 1 -> report final (finalized) -> stop.
    runtime.submit_user("begin").await.unwrap();
    runtime
        .wait_until_idle(Duration::from_secs(3))
        .await
        .unwrap();

    // Exactly three model calls; the loop stopped at the finalizing turn and did
    // not run the guard turn.
    assert_eq!(provider.requests_for("main").len(), 3);
    runtime.set_auto(false);
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn main_runtime_projection_tracks_team_messages_even_while_auto_is_off() {
    let provider = Arc::new(ScriptedProvider::new(HashMap::new()));
    let dir = tempdir().unwrap();
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider,
        runtime_config(),
    )
    .await
    .unwrap();
    let worker = runtime
        .spawn_worker(&AgentId::main(), "research", "inspect the target")
        .await
        .unwrap();
    runtime
        .coordinator()
        .send(
            worker,
            vec![AgentId::main()],
            MessageKind::Insight,
            "important update",
            Some(Insight::new(
                InsightId::new("I-live-team").unwrap(),
                InsightLabel::Fact,
                "endpoint returned 200",
            )),
        )
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        runtime
            .brief(&AgentId::main())
            .unwrap()
            .contains("endpoint returned 200")
    );
    runtime
        .coordinator()
        .mark_waiting(&AgentId::new("worker-01").unwrap(), "external blocker")
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let brief = runtime.brief(&AgentId::main()).unwrap();
    runtime.shutdown().await;

    assert!(brief.contains("| worker-01 | research | inspect the target | WAITING"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn acknowledged_team_messages_remain_in_context_until_semantic_compaction() {
    let mut scripts = HashMap::new();
    scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![
            text_script("first observed"),
            text_script("second observed"),
        ]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider.clone(),
        runtime_config(),
    )
    .await
    .unwrap();
    let worker = runtime
        .spawn_worker(&AgentId::main(), "worker", "integrate messages")
        .await
        .unwrap();
    runtime
        .coordinator()
        .send(
            AgentId::main(),
            vec![worker.clone()],
            MessageKind::Progress,
            "FIRST-DURABLE-MESSAGE",
            None,
        )
        .unwrap();
    runtime.set_auto(true);
    runtime
        .wait_until_idle(Duration::from_secs(2))
        .await
        .unwrap();
    runtime
        .coordinator()
        .send(
            AgentId::main(),
            vec![worker.clone()],
            MessageKind::Progress,
            "SECOND-TRIGGER",
            None,
        )
        .unwrap();
    runtime
        .wait_until_idle(Duration::from_secs(2))
        .await
        .unwrap();

    let requests = provider.requests_for(worker.as_str());
    runtime.shutdown().await;
    assert_eq!(requests.len(), 2);
    assert!(
        requests[1]
            .messages
            .iter()
            .any(|message| message.content.contains("FIRST-DURABLE-MESSAGE"))
    );
    // A worker's system prompt carries the worker role, never the main role.
    let worker_system = &requests[0].messages[0].content;
    assert!(worker_system.contains("YOUR ROLE: WORKER"));
    assert!(!worker_system.contains("YOUR ROLE: MAIN"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn team_wait_wakes_for_a_message_without_duplicating_its_payload_in_context() {
    let mut scripts = HashMap::new();
    scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![
            Script::Turn {
                turn: tool_turn(vec![(
                    "wait-once",
                    "team",
                    serde_json::json!({"op":"wait","timeout_ms":1000}),
                )]),
                delay_ms: 0,
            },
            text_script("message integrated once"),
        ]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let mut config = runtime_config();
    config.auto = true;
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider.clone(),
        config,
    )
    .await
    .unwrap();
    let mut events = runtime.subscribe();
    let worker = runtime
        .spawn_worker(&AgentId::main(), "worker", "wait for a direct message")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if matches!(
                events.recv().await,
                Ok(RuntimeEvent::ToolStarted { agent_id, name, .. })
                    if agent_id == worker && name == "team"
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();
    runtime
        .coordinator()
        .send(
            AgentId::main(),
            vec![worker.clone()],
            MessageKind::Progress,
            "WAIT-PAYLOAD-ONCE",
            None,
        )
        .unwrap();
    runtime
        .wait_until_idle(Duration::from_secs(2))
        .await
        .unwrap();

    let requests = provider.requests_for(worker.as_str());
    runtime.shutdown().await;
    assert_eq!(requests.len(), 2);
    let second_context = requests[1]
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(second_context.matches("WAIT-PAYLOAD-ONCE").count(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn restart_restores_acknowledged_team_messages_that_are_not_compacted() {
    let mut first_scripts = HashMap::new();
    first_scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![text_script("observed before restart")]),
    );
    let first_provider = Arc::new(ScriptedProvider::new(first_scripts));
    let dir = tempdir().unwrap();
    let run_root = dir.path().join("run");
    let workspace = dir.path().join("workspace");
    let first = TeamRuntime::create(
        &run_root,
        &workspace,
        "goal",
        first_provider,
        runtime_config(),
    )
    .await
    .unwrap();
    let worker = first
        .spawn_worker(&AgentId::main(), "worker", "retain acknowledged input")
        .await
        .unwrap();
    first
        .coordinator()
        .send(
            AgentId::main(),
            vec![worker.clone()],
            MessageKind::Progress,
            "ACKNOWLEDGED-BEFORE-RESTART",
            None,
        )
        .unwrap();
    first.set_auto(true);
    first.wait_until_idle(Duration::from_secs(2)).await.unwrap();
    first.shutdown().await;
    drop(first);

    let mut resumed_scripts = HashMap::new();
    resumed_scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![text_script("observed after restart")]),
    );
    let resumed_provider = Arc::new(ScriptedProvider::new(resumed_scripts));
    let resumed = TeamRuntime::resume(
        &run_root,
        &workspace,
        resumed_provider.clone(),
        runtime_config(),
    )
    .await
    .unwrap();
    resumed
        .coordinator()
        .send(
            AgentId::main(),
            vec![worker.clone()],
            MessageKind::Progress,
            "AFTER-RESTART-TRIGGER",
            None,
        )
        .unwrap();
    resumed.set_auto(true);
    resumed
        .wait_until_idle(Duration::from_secs(2))
        .await
        .unwrap();

    let requests = resumed_provider.requests_for(worker.as_str());
    resumed.shutdown().await;
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0]
            .messages
            .iter()
            .any(|message| message.content.contains("ACKNOWLEDGED-BEFORE-RESTART"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn provider_fault_pauses_worker_until_new_activity_without_killing_it() {
    let mut scripts = HashMap::new();
    scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![
            fault_script(),
            Script::Turn {
                turn: tool_turn(vec![(
                    "finish",
                    "team",
                    serde_json::json!({"op":"finish","body":"recovered"}),
                )]),
                delay_ms: 0,
            },
        ]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let mut config = runtime_config();
    config.auto = true;
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider.clone(),
        config,
    )
    .await
    .unwrap();
    let worker = runtime
        .spawn_worker(&AgentId::main(), "worker", "fault once")
        .await
        .unwrap();
    runtime
        .wait_until_idle(Duration::from_secs(2))
        .await
        .unwrap();

    let waiting = runtime.coordinator().inspect(&worker).unwrap();
    assert_eq!(waiting.state, AgentState::Waiting);
    assert_eq!(
        waiting.waiting_on.as_deref(),
        Some("provider unavailable: upstream")
    );
    assert_eq!(provider.requests_for(worker.as_str()).len(), 1);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(provider.requests_for(worker.as_str()).len(), 1);
    assert_eq!(
        runtime
            .coordinator()
            .journal()
            .replay()
            .unwrap()
            .iter()
            .filter(|entry| matches!(entry.event, JournalEvent::Fault { .. }))
            .count(),
        1
    );

    runtime
        .coordinator()
        .send(
            AgentId::main(),
            vec![worker.clone()],
            MessageKind::Progress,
            "provider is back; continue",
            None,
        )
        .unwrap();
    runtime
        .wait_until_idle(Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(
        runtime.coordinator().inspect(&worker).unwrap().state,
        AgentState::Finished
    );
    assert_eq!(provider.requests_for(worker.as_str()).len(), 2);
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn provider_fault_with_an_unread_inbox_still_waits_for_new_activity() {
    let gate = Gate {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let mut scripts = HashMap::new();
    scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![
            text_script("ready"),
            fault_script(),
            Script::Gated {
                gate: gate.clone(),
                turn: text_turn("must wait for new activity"),
            },
        ]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let mut config = runtime_config();
    config.auto = true;
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider.clone(),
        config,
    )
    .await
    .unwrap();
    let worker = runtime
        .spawn_worker(&AgentId::main(), "worker", "wait after fault")
        .await
        .unwrap();
    runtime
        .wait_until_idle(Duration::from_secs(2))
        .await
        .unwrap();
    runtime
        .coordinator()
        .send(
            AgentId::main(),
            vec![worker.clone()],
            MessageKind::Progress,
            "process once",
            None,
        )
        .unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while provider.requests_for(worker.as_str()).len() < 2 && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    let requests = provider.requests_for(worker.as_str()).len();
    gate.release.notify_waiters();
    runtime.shutdown().await;

    assert_eq!(
        requests, 2,
        "stale unread input triggered an automatic retry"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_auto_resume_retries_a_provider_blocked_worker() {
    let mut scripts = HashMap::new();
    scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![
            fault_script(),
            Script::Turn {
                turn: tool_turn(vec![(
                    "finish",
                    "team",
                    serde_json::json!({"op":"finish","body":"resumed"}),
                )]),
                delay_ms: 0,
            },
        ]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let mut config = runtime_config();
    config.auto = true;
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider.clone(),
        config,
    )
    .await
    .unwrap();
    let worker = runtime
        .spawn_worker(&AgentId::main(), "worker", "retry when auto resumes")
        .await
        .unwrap();
    runtime
        .wait_until_idle(Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(
        runtime.coordinator().inspect(&worker).unwrap().state,
        AgentState::Waiting
    );
    assert_eq!(provider.requests_for(worker.as_str()).len(), 1);

    runtime.set_auto(false);
    runtime.set_auto(true);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if runtime.coordinator().inspect(&worker).unwrap().state == AgentState::Finished {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "explicit auto resume did not wake the provider-blocked worker: state={:?}, requests={}, revision={}, auto={}",
            runtime.coordinator().inspect(&worker).unwrap(),
            provider.requests_for(worker.as_str()).len(),
            runtime.coordinator().activity_revision(&worker).unwrap(),
            runtime.auto_enabled(),
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(provider.requests_for(worker.as_str()).len(), 2);
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reassign_is_new_activity_that_wakes_a_waiting_worker() {
    let gate = Gate {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let mut scripts = HashMap::new();
    scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![
            text_script("first assignment paused"),
            Script::Gated {
                gate: gate.clone(),
                turn: text_turn("reassigned"),
            },
        ]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let mut config = runtime_config();
    config.auto = true;
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider,
        config,
    )
    .await
    .unwrap();
    let worker = runtime
        .spawn_worker(&AgentId::main(), "first", "first assignment")
        .await
        .unwrap();
    runtime
        .wait_until_idle(Duration::from_secs(2))
        .await
        .unwrap();
    runtime
        .coordinator()
        .reassign(&AgentId::main(), &worker, "second", "new assignment")
        .unwrap();

    tokio::time::timeout(Duration::from_secs(2), gate.started.notified())
        .await
        .unwrap();
    gate.release.notify_waiters();
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_messages_update_only_the_main_runtime_projection() {
    let gate = Gate {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let mut scripts = HashMap::new();
    scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![Script::Gated {
            gate: gate.clone(),
            turn: tool_turn(vec![(
                "share",
                "team",
                serde_json::json!({
                    "op":"send",
                    "to":["main"],
                    "kind":"insight",
                    "body":"new worker insight",
                    "insight":{"id":"I-no-main-write","label":"FACT","text":"worker observed 200"}
                }),
            )]),
        }]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let run_root = dir.path().join("run");
    let mut config = runtime_config();
    config.auto = true;
    let runtime = TeamRuntime::create(
        &run_root,
        dir.path().join("workspace"),
        "goal",
        provider,
        config,
    )
    .await
    .unwrap();
    let mut events = runtime.subscribe();
    runtime
        .spawn_worker(&AgentId::main(), "worker", "share once")
        .await
        .unwrap();
    gate.started.notified().await;
    runtime.set_auto(false);
    let before = std::fs::read_to_string(run_root.join("agents/main/brief.md")).unwrap();

    gate.release.notify_waiters();
    runtime
        .wait_until_idle(Duration::from_secs(2))
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let after = loop {
        let current = std::fs::read_to_string(run_root.join("agents/main/brief.md")).unwrap();
        if current.contains("worker observed 200") || tokio::time::Instant::now() >= deadline {
            break current;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert!(after.contains("worker observed 200"));
    assert_eq!(
        without_runtime_projection(&after),
        without_runtime_projection(&before)
    );
    let message_event = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(RuntimeEvent::AgentMessage {
                sender,
                recipients,
                body,
                ..
            }) = events.recv().await
            {
                break (sender, recipients, body);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(message_event.0, AgentId::new("worker-01").unwrap());
    assert_eq!(message_event.1, vec![AgentId::main()]);
    assert_eq!(message_event.2, "new worker insight");
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_request_directs_agents_to_execute_latest_human_instructions() {
    let mut scripts = HashMap::new();
    scripts.insert("main".into(), VecDeque::from(vec![text_script("ack")]));
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "complete the requested task",
        provider.clone(),
        runtime_config(),
    )
    .await
    .unwrap();

    runtime
        .submit_user("inspect the authorized target and solve the task")
        .await
        .unwrap();
    let request = provider.requests_for("main").pop().unwrap();
    let system = &request.messages[0].content;

    assert!(system.contains("latest explicit human instruction"));
    assert!(system.contains("Use the available tools to act"));
    assert!(system.contains("Do not stop at advice or a plan"));
    assert!(system.contains("complete or genuinely blocked"));
    assert!(request.tools_enabled);
    assert!(!request.tools.is_empty());
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failed_main_turn_does_not_start_an_automatic_retry_storm() {
    let mut scripts = HashMap::new();
    scripts.insert(
        "main".into(),
        VecDeque::from(vec![fault_script(), text_script("explicit retry worked")]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let mut config = runtime_config();
    config.auto = true;
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider.clone(),
        config,
    )
    .await
    .unwrap();

    assert!(runtime.submit_user("first attempt").await.is_err());
    let waiting = runtime.coordinator().inspect(&AgentId::main()).unwrap();
    assert_eq!(waiting.state, AgentState::Waiting);
    assert_eq!(
        waiting.waiting_on.as_deref(),
        Some("provider unavailable: upstream")
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(provider.requests_for("main").len(), 1);
    assert_eq!(
        runtime.submit_user("retry now").await.unwrap().text,
        "explicit retry worked"
    );
    assert_eq!(
        runtime
            .coordinator()
            .inspect(&AgentId::main())
            .unwrap()
            .state,
        AgentState::Running
    );
    runtime.set_auto(false);
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn recall_cancels_an_inflight_provider_turn_and_releases_the_worker_permit() {
    let gate = Gate {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let mut scripts = HashMap::new();
    scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![Script::Gated {
            gate: gate.clone(),
            turn: text_turn("must be cancelled"),
        }]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let mut config = runtime_config();
    config.auto = true;
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider,
        config,
    )
    .await
    .unwrap();
    let mut workers = Vec::new();
    for index in 0..9 {
        workers.push(
            runtime
                .spawn_worker(
                    &AgentId::main(),
                    format!("role-{index}"),
                    format!("task-{index}"),
                )
                .await
                .unwrap(),
        );
    }
    gate.started.notified().await;
    runtime
        .coordinator()
        .recall(&AgentId::main(), &workers[0], "replace task")
        .unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if runtime.coordinator().inspect(&workers[0]).unwrap().state == AgentState::Stopped {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "recall did not stop worker"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        runtime
            .spawn_worker(&AgentId::main(), "replacement", "reuse permit")
            .await
            .is_ok()
    );
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn interrupt_main_cancels_the_current_turn_and_accepts_the_next_input() {
    let gate = Gate {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let mut scripts = HashMap::new();
    scripts.insert(
        "main".into(),
        VecDeque::from(vec![
            Script::Gated {
                gate: gate.clone(),
                turn: text_turn("must be cancelled"),
            },
            Script::Turn {
                turn: text_turn("recovered"),
                delay_ms: 0,
            },
        ]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider,
        runtime_config(),
    )
    .await
    .unwrap();

    let first_runtime = runtime.clone();
    let first = tokio::spawn(async move { first_runtime.submit_user("first").await });
    gate.started.notified().await;

    assert!(runtime.interrupt_main());
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), first)
            .await
            .unwrap()
            .unwrap(),
        Err(RuntimeError::Cancelled)
    ));
    assert_eq!(
        runtime.submit_user("second").await.unwrap().text,
        "recovered"
    );
    assert!(!runtime.interrupt_main());

    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn recall_cancels_a_worker_waiting_for_the_provider_semaphore() {
    let gate = Gate {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let mut scripts = HashMap::new();
    scripts.insert(
        "worker-01".into(),
        VecDeque::from(vec![Script::Gated {
            gate: gate.clone(),
            turn: text_turn("first done"),
        }]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let mut config = runtime_config();
    config.auto = true;
    config.max_parallel_requests = 1;
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider,
        config,
    )
    .await
    .unwrap();
    let _first = runtime
        .spawn_worker(&AgentId::main(), "first", "hold request slot")
        .await
        .unwrap();
    gate.started.notified().await;
    let waiting = runtime
        .spawn_worker(&AgentId::main(), "second", "wait for request slot")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    runtime
        .coordinator()
        .recall(&AgentId::main(), &waiting, "cancel queued request")
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    loop {
        if runtime.coordinator().inspect(&waiting).unwrap().state == AgentState::Stopped {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "worker waiting on request semaphore ignored recall"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    gate.release.notify_waiters();
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_startup_failure_does_not_leave_a_live_agent_or_leak_its_permit() {
    let provider = Arc::new(ScriptedProvider::new(HashMap::new()));
    let dir = tempdir().unwrap();
    let run_root = dir.path().join("run");
    let runtime = TeamRuntime::create(
        &run_root,
        dir.path().join("workspace"),
        "goal",
        provider,
        runtime_config(),
    )
    .await
    .unwrap();
    std::fs::write(run_root.join("agents/worker-01"), b"path obstruction").unwrap();

    assert!(
        runtime
            .spawn_worker(&AgentId::main(), "broken", "cannot initialize")
            .await
            .is_err()
    );
    assert!(
        runtime
            .coordinator()
            .inspect(&AgentId::new("worker-01").unwrap())
            .unwrap()
            .state
            .is_terminal()
    );
    assert_eq!(runtime.coordinator().active_team_size(), 1);
    assert!(
        runtime
            .spawn_worker(&AgentId::main(), "replacement", "starts cleanly")
            .await
            .is_ok()
    );
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_startup_failure_removes_a_projection_created_before_the_failure() {
    let provider = Arc::new(ScriptedProvider::new(HashMap::new()));
    let dir = tempdir().unwrap();
    let run_root = dir.path().join("run");
    let runtime = TeamRuntime::create(
        &run_root,
        dir.path().join("workspace"),
        "goal",
        provider,
        runtime_config(),
    )
    .await
    .unwrap();
    std::fs::write(run_root.join("agents/main/brief.md"), b"invalid main brief").unwrap();

    assert!(
        runtime
            .spawn_worker(&AgentId::main(), "broken", "fails after initialization")
            .await
            .is_err()
    );
    assert!(
        !run_root.join("agents/worker-01/brief.md").exists(),
        "a terminal startup artifact remained addressable as a live brief"
    );
    assert_eq!(runtime.coordinator().active_team_size(), 1);
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn goal_update_is_durable_but_does_not_enable_or_call_the_model() {
    let provider = Arc::new(ScriptedProvider::new(HashMap::new()));
    let dir = tempdir().unwrap();
    let run_root = dir.path().join("run");
    let runtime = TeamRuntime::create(
        &run_root,
        dir.path().join("workspace"),
        "old goal",
        provider.clone(),
        runtime_config(),
    )
    .await
    .unwrap();

    runtime
        .set_goal(Some("new durable goal".to_owned()))
        .await
        .unwrap();
    assert_eq!(
        runtime
            .coordinator()
            .inspect(&AgentId::main())
            .unwrap()
            .task,
        "new durable goal"
    );
    assert!(provider.requests_for("main").is_empty());
    runtime.shutdown().await;
    drop(runtime);

    let journal = Arc::new(
        pentesting::journal::RunJournal::open(
            &run_root,
            pentesting::journal::JournalConfig::default(),
        )
        .unwrap(),
    );
    assert_eq!(
        pentesting::coordinator::AgentCoordinator::recover(journal)
            .unwrap()
            .inspect(&AgentId::main())
            .unwrap()
            .task,
        "new durable goal"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manual_compaction_check_and_shell_shortcut_share_the_main_fifo() {
    let mut scripts = HashMap::new();
    scripts.insert(
        "main".into(),
        VecDeque::from(vec![text_script("after shell")]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider.clone(),
        runtime_config(),
    )
    .await
    .unwrap();

    assert!(!runtime.compact_main().await.unwrap());
    let shell = runtime.run_bash("printf shell-visible").await.unwrap();
    assert!(shell.contains("shell-visible"));
    assert_eq!(
        runtime.submit_user("continue").await.unwrap().text,
        "after shell"
    );

    let request = provider.requests_for("main").pop().unwrap();
    let request_text = request
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(request_text.contains("shell-visible"));
    let replay = runtime.coordinator().journal().replay().unwrap();
    assert!(replay.iter().any(|entry| matches!(
        &entry.event,
        JournalEvent::ToolCall { name, .. } if name == "bash"
    )));
    assert!(replay.iter().any(|entry| matches!(
        &entry.event,
        JournalEvent::ToolResult { content, success, .. }
            if *success && content.contains("shell-visible")
    )));
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oversized_user_input_is_rejected_before_queue_journal_and_provider() {
    let provider = Arc::new(ScriptedProvider::new(HashMap::new()));
    let dir = tempdir().unwrap();
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider.clone(),
        runtime_config(),
    )
    .await
    .unwrap();
    let before = runtime.coordinator().journal().replay().unwrap().len();

    assert!(matches!(
        runtime
            .submit_user("x".repeat(MAX_USER_INPUT_BYTES + 1))
            .await,
        Err(RuntimeError::Domain(DomainError::TextTooLarge {
            field: "user input",
            ..
        }))
    ));
    assert_eq!(
        runtime.coordinator().journal().replay().unwrap().len(),
        before
    );
    assert!(provider.requests_for("main").is_empty());
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_successful_main_final_stops_auto_continuation() {
    let mut scripts = HashMap::new();
    scripts.insert(
        "main".into(),
        VecDeque::from(vec![Script::Turn {
            turn: tool_turn(vec![(
                "final",
                "report",
                serde_json::json!({"op":"final","body":"done"}),
            )]),
            delay_ms: 0,
        }]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let mut config = runtime_config();
    config.auto = true;
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "finish once",
        provider.clone(),
        config,
    )
    .await
    .unwrap();
    let mut events = runtime.subscribe();

    let result = runtime.submit_user("finish").await.unwrap();
    assert!(result.finalized);
    let finished = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Ok(RuntimeEvent::TurnFinished { agent_id, success }) = events.recv().await {
                break (agent_id, success);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(finished, (AgentId::main(), true));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(provider.requests_for("main").len(), 1);
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_final_tool_is_not_treated_as_completion() {
    let mut scripts = HashMap::new();
    scripts.insert(
        "main".into(),
        VecDeque::from(vec![
            Script::Turn {
                turn: tool_turn(vec![(
                    "invalid-final",
                    "report",
                    serde_json::json!({"op":"final"}),
                )]),
                delay_ms: 0,
            },
            Script::Turn {
                turn: text_turn("recovered after the rejected tool"),
                delay_ms: 0,
            },
        ]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "do not fake completion",
        provider,
        runtime_config(),
    )
    .await
    .unwrap();

    let result = runtime
        .submit_user("continue through tool failure")
        .await
        .unwrap();
    assert!(!result.finalized);
    assert_eq!(result.text, "recovered after the rejected tool");
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_compaction_has_one_total_deadline_and_leaves_main_recoverable() {
    let dir = tempdir().unwrap();
    let run_root = dir.path().join("run");
    let workspace = dir.path().join("workspace");
    let provider = Arc::new(HangingCompactionProvider);
    let mut config = runtime_config();
    config.compaction_timeout = Duration::from_millis(50);
    let first = TeamRuntime::create(
        &run_root,
        &workspace,
        "bound compaction",
        provider.clone(),
        config.clone(),
    )
    .await
    .unwrap();
    let journal = first.coordinator().journal();
    for marker in ["OLDER", "LATEST"] {
        journal
            .append_sync(JournalEvent::Transcript {
                agent_id: AgentId::main(),
                role: pentesting::journal::TranscriptRole::User,
                content: format!("{marker}-{}", "x".repeat(30_000)),
                complete: true,
                atomic_group: None,
            })
            .unwrap();
    }
    drop(journal);
    first.shutdown().await;
    drop(first);

    let resumed = TeamRuntime::resume(&run_root, &workspace, provider, config)
        .await
        .unwrap();
    assert!(matches!(
        resumed.compact_main().await,
        Err(RuntimeError::CompactionTimedOut(_))
    ));
    let main = resumed.coordinator().inspect(&AgentId::main()).unwrap();
    assert_eq!(main.state, AgentState::Waiting);
    assert_eq!(
        main.waiting_on.as_deref(),
        Some("context curation timed out")
    );
    let source = resumed
        .coordinator()
        .journal()
        .replay()
        .unwrap()
        .into_iter()
        .filter_map(|entry| match entry.event {
            JournalEvent::Transcript { content, .. } => Some(content),
            _ => None,
        })
        .collect::<String>();
    assert!(source.contains("OLDER-"));
    assert!(source.contains("LATEST-"));
    assert_eq!(
        resumed
            .coordinator()
            .journal()
            .replay()
            .unwrap()
            .iter()
            .filter(|entry| matches!(
                &entry.event,
                JournalEvent::Fault { code, .. } if code == "context_curation"
            ))
            .count(),
        1
    );
    resumed.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn main_semantic_compaction_commits_a_valid_single_runtime_brief() {
    let dir = tempdir().unwrap();
    let run_root = dir.path().join("run");
    let workspace = dir.path().join("workspace");
    let provider = Arc::new(SuccessfulMainCompactionProvider);
    // A large OLDER message forces compaction; a small LATEST keeps the protected
    // live tail tiny, so the post-compaction projection stays well under target
    // regardless of how large the (content-rich) system prompt grows.
    let config = runtime_config();
    let first = TeamRuntime::create(
        &run_root,
        &workspace,
        "compact main",
        provider.clone(),
        config.clone(),
    )
    .await
    .unwrap();
    let journal = first.coordinator().journal();
    let older = journal
        .append_sync(JournalEvent::Transcript {
            agent_id: AgentId::main(),
            role: pentesting::journal::TranscriptRole::User,
            content: format!("OLDER-{}", "x".repeat(80_000)),
            complete: true,
            atomic_group: None,
        })
        .unwrap();
    let latest = journal
        .append_sync(JournalEvent::Transcript {
            agent_id: AgentId::main(),
            role: pentesting::journal::TranscriptRole::User,
            content: format!("LATEST-{}", "x".repeat(1_000)),
            complete: true,
            atomic_group: None,
        })
        .unwrap();
    drop(journal);
    first.shutdown().await;
    drop(first);

    let resumed = TeamRuntime::resume(&run_root, &workspace, provider, config)
        .await
        .unwrap();
    assert!(resumed.compact_main().await.unwrap());
    let brief = resumed.brief(&AgentId::main()).unwrap();
    assert!(brief.contains("one active arc"));
    assert_eq!(
        brief
            .matches("<!-- minimal-agent:runtime:start -->")
            .count(),
        1
    );
    let checkpoint = resumed
        .coordinator()
        .journal()
        .replay()
        .unwrap()
        .into_iter()
        .rev()
        .find_map(|entry| match entry.event {
            JournalEvent::BriefCheckpoint { source_ranges, .. } => Some(source_ranges),
            _ => None,
        })
        .unwrap();
    assert!(checkpoint.iter().any(|range| range.start == older.sequence));
    assert!(
        !checkpoint
            .iter()
            .any(|range| latest.sequence >= range.start && latest.sequence <= range.end)
    );
    resumed.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_skips_only_exactly_compacted_ranges_and_preserves_live_holes() {
    let dir = tempdir().unwrap();
    let run_root = dir.path().join("run");
    let workspace = dir.path().join("workspace");
    let first_provider = Arc::new(ScriptedProvider::new(HashMap::new()));
    let first = TeamRuntime::create(
        &run_root,
        &workspace,
        "preserve holes",
        first_provider,
        runtime_config(),
    )
    .await
    .unwrap();
    let journal = first.coordinator().journal();
    let partial = journal
        .append_sync(JournalEvent::Transcript {
            agent_id: AgentId::main(),
            role: pentesting::journal::TranscriptRole::Assistant,
            content: "LIVE-PARTIAL-HOLE".into(),
            complete: false,
            atomic_group: None,
        })
        .unwrap();
    let covered = journal
        .append_sync(JournalEvent::Transcript {
            agent_id: AgentId::main(),
            role: pentesting::journal::TranscriptRole::User,
            content: "EXACTLY-COVERED-LATER".into(),
            complete: true,
            atomic_group: None,
        })
        .unwrap();
    assert!(partial.sequence < covered.sequence);
    let markdown = first.brief(&AgentId::main()).unwrap();
    journal
        .append_sync(JournalEvent::BriefCheckpoint {
            agent_id: AgentId::main(),
            markdown_sha256: hex::encode(Sha256::digest(markdown.as_bytes())),
            markdown,
            source_sha256: "c".repeat(64),
            source_ranges: vec![
                pentesting::domain::SequenceRange::new(covered.sequence, covered.sequence).unwrap(),
            ],
            coverage: pentesting::domain::CompactionCoverage {
                covered_ranges: vec![
                    pentesting::domain::SequenceRange::new(covered.sequence, covered.sequence)
                        .unwrap(),
                ],
                covered_insight_ids: vec![],
                superseded_insight_ids: vec![],
            },
        })
        .unwrap();
    drop(journal);
    first.shutdown().await;
    drop(first);

    let mut scripts = HashMap::new();
    scripts.insert("main".into(), VecDeque::from(vec![text_script("resumed")]));
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let resumed = TeamRuntime::resume(&run_root, &workspace, provider.clone(), runtime_config())
        .await
        .unwrap();
    resumed
        .submit_user("inspect recovered context")
        .await
        .unwrap();
    let text = provider.requests_for("main")[0]
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("LIVE-PARTIAL-HOLE"));
    assert!(!text.contains("EXACTLY-COVERED-LATER"));
    resumed.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_cancels_inflight_tools_without_detaching_driver_tasks() {
    let provider = Arc::new(ScriptedProvider::new(HashMap::new()));
    let dir = tempdir().unwrap();
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider,
        runtime_config(),
    )
    .await
    .unwrap();
    let mut events = runtime.subscribe();
    let shell_runtime = runtime.clone();
    let shell = tokio::spawn(async move { shell_runtime.run_bash("sleep 30").await });
    loop {
        if matches!(
            events.recv().await.unwrap(),
            pentesting::runtime::RuntimeEvent::ToolStarted { ref name, .. } if name == "bash"
        ) {
            break;
        }
    }

    tokio::time::timeout(Duration::from_millis(750), runtime.shutdown())
        .await
        .expect("shutdown left an inflight tool or detached driver");
    assert!(shell.await.unwrap().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn safe_compaction_refusal_preserves_context_and_waits_instead_of_faulting() {
    let provider = Arc::new(ScriptedProvider::new(HashMap::new()));
    let dir = tempdir().unwrap();
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider.clone(),
        runtime_config(),
    )
    .await
    .unwrap();

    assert!(runtime.submit_user("x".repeat(60_000)).await.is_err());
    let main = runtime.coordinator().inspect(&AgentId::main()).unwrap();
    assert_eq!(main.state, AgentState::Waiting);
    assert_eq!(main.waiting_on.as_deref(), Some("context curation blocked"));
    assert!(provider.requests_for("main").is_empty());
    assert!(
        runtime
            .coordinator()
            .journal()
            .replay()
            .unwrap()
            .iter()
            .any(|entry| matches!(
                &entry.event,
                JournalEvent::Transcript { content, .. } if content.len() == 60_000
            ))
    );
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn engagement_is_persisted_and_recovered_on_resume() {
    let engagement = Engagement {
        kind: EngagementKind::Ctf,
        scope: Some("10.10.10.5".to_owned()),
        flag_format: Some(r"flag\{[^}]+\}".to_owned()),
        ..Engagement::default()
    };
    let mut config = runtime_config();
    config.engagement = Some(engagement);

    let mut first_scripts = HashMap::new();
    first_scripts.insert("main".into(), VecDeque::from(vec![text_script("parked")]));
    let first_provider = Arc::new(ScriptedProvider::new(first_scripts));
    let dir = tempdir().unwrap();
    let run_root = dir.path().join("run");
    let workspace = dir.path().join("workspace");
    let first = TeamRuntime::create(
        &run_root,
        &workspace,
        "capture the flag on the box",
        first_provider,
        config,
    )
    .await
    .unwrap();
    first.shutdown().await;
    drop(first);

    // Resume WITHOUT re-supplying the engagement; it must be recovered.
    let mut resumed_scripts = HashMap::new();
    resumed_scripts.insert("main".into(), VecDeque::from(vec![text_script("resumed")]));
    let resumed_provider = Arc::new(ScriptedProvider::new(resumed_scripts));
    let resumed = TeamRuntime::resume(
        &run_root,
        &workspace,
        resumed_provider.clone(),
        runtime_config(),
    )
    .await
    .unwrap();
    resumed.submit_user("continue").await.unwrap();

    let system = resumed_provider
        .requests_for("main")
        .pop()
        .unwrap()
        .messages[0]
        .content
        .clone();
    // Standing authorization is always present; the recovered engagement adds
    // its scope and the CTF solve-loop doctrine.
    assert!(system.contains("EXECUTION MANDATE"));
    assert!(system.contains("10.10.10.5"));
    assert!(system.contains(r"flag\{[^}]+\}"));
    assert!(system.contains("CTF SOLVE LOOP"));
    resumed.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn standing_authorization_is_present_without_any_engagement() {
    let mut scripts = HashMap::new();
    scripts.insert("main".into(), VecDeque::from(vec![text_script("ok")]));
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let runtime = TeamRuntime::create(
        &dir.path().join("run"),
        &dir.path().join("workspace"),
        "ordinary goal",
        provider.clone(),
        runtime_config(),
    )
    .await
    .unwrap();
    runtime.submit_user("go").await.unwrap();

    let system = provider.requests_for("main").pop().unwrap().messages[0]
        .content
        .clone();
    assert!(system.contains("EXECUTION MANDATE"));
    // The terse, action-first operating style is always in effect.
    assert!(system.contains("OPERATING STYLE"));
    // Main gets the main role prompt, not the worker one.
    assert!(system.contains("YOUR ROLE: MAIN"));
    assert!(!system.contains("YOUR ROLE: WORKER"));
    // No engagement => no target block and no CTF doctrine.
    assert!(!system.contains("AUTHORIZED ENGAGEMENT"));
    assert!(!system.contains("CTF SOLVE LOOP"));
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn steering_input_folds_into_a_running_main_turn_without_restart() {
    let gate = Gate {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let mut scripts = HashMap::new();
    scripts.insert(
        "main".into(),
        VecDeque::from(vec![
            Script::Gated {
                gate: gate.clone(),
                turn: tool_turn(vec![(
                    "s1",
                    "bash",
                    serde_json::json!({"command": "echo hi"}),
                )]),
            },
            text_script("acknowledged the new instruction"),
        ]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "solve the task",
        provider.clone(),
        runtime_config(),
    )
    .await
    .unwrap();

    // No main turn is active yet, so steering is declined (caller starts a turn).
    assert!(!runtime.steer_main("too early").unwrap());

    let submit = {
        let runtime = runtime.clone();
        tokio::spawn(async move { runtime.submit_user("start working").await })
    };
    gate.started.notified().await;
    // A main turn is running now; steering is accepted and queued for injection.
    assert!(runtime.steer_main("ALSO check the login page").unwrap());
    gate.release.notify_waiters();

    let _ = submit.await.unwrap();
    runtime
        .wait_until_idle(Duration::from_secs(3))
        .await
        .unwrap();

    let requests = provider.requests_for("main");
    // The in-flight model-turn never saw the steer; a later request folds it in
    // without a restart, alongside the original objective.
    assert!(
        !requests[0]
            .messages
            .iter()
            .any(|message| message.content.contains("ALSO check the login page"))
    );
    assert!(requests.iter().skip(1).any(|request| {
        let text = request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        text.contains("ALSO check the login page") && text.contains("start working")
    }));
    runtime.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn setting_a_target_live_injects_scope_into_the_next_prompt() {
    let mut scripts = HashMap::new();
    scripts.insert(
        "main".into(),
        VecDeque::from(vec![text_script("ok"), text_script("ok")]),
    );
    let provider = Arc::new(ScriptedProvider::new(scripts));
    let dir = tempdir().unwrap();
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "goal",
        provider.clone(),
        runtime_config(),
    )
    .await
    .unwrap();

    // No engagement yet: the first prompt has no target block.
    runtime.submit_user("first").await.unwrap();
    let before = provider.requests_for("main").pop().unwrap().messages[0]
        .content
        .clone();
    assert!(!before.contains("AUTHORIZED ENGAGEMENT"));

    // Set a target live; the next prompt must carry it.
    runtime
        .set_engagement_scope(Some("10.10.11.20:8080".to_owned()))
        .unwrap();
    runtime.submit_user("second").await.unwrap();
    let after = provider.requests_for("main").pop().unwrap().messages[0]
        .content
        .clone();
    assert!(after.contains("AUTHORIZED ENGAGEMENT"));
    assert!(after.contains("10.10.11.20:8080"));
    runtime.shutdown().await;
}

#[tokio::test]
async fn nested_worker_runs_and_failed_startup_releases_its_slot() {
    let dir = tempdir().unwrap();
    let provider = Arc::new(ScriptedProvider::new(HashMap::new()));
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "generic task",
        provider.clone(),
        RuntimeConfig::default(),
    )
    .await
    .unwrap();
    runtime.set_auto(true);
    let lead = runtime
        .spawn_worker(&AgentId::main(), "lead", "coordinate")
        .await
        .unwrap();
    let leaf = runtime
        .spawn_worker(&lead, "leaf", "execute")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while provider.requests_for(leaf.as_str()).is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    runtime.set_auto(false);
    runtime
        .wait_until_idle(Duration::from_secs(3))
        .await
        .unwrap();
    assert!(!provider.requests_for(leaf.as_str()).is_empty());
    assert_eq!(
        runtime.coordinator().inspect(&leaf).unwrap().parent,
        Some(lead.clone())
    );
    let before = runtime.coordinator().live_team().unwrap().len();
    // A deterministic projection failure after create_worker must release the child.
    std::fs::write(
        dir.path().join("run/agents/main/brief.md"),
        "invalid projection",
    )
    .unwrap();
    assert!(
        runtime
            .spawn_worker(&lead, "failed", "cannot start")
            .await
            .is_err()
    );
    assert_eq!(runtime.coordinator().live_team().unwrap().len(), before);
    runtime.shutdown().await;
}

#[tokio::test]
async fn invalid_scope_update_preserves_previous_context() {
    let dir = tempdir().unwrap();
    let provider = Arc::new(ScriptedProvider::new(HashMap::new()));
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "generic task",
        provider.clone(),
        RuntimeConfig::default(),
    )
    .await
    .unwrap();
    runtime
        .set_engagement_scope(Some("local fixture".to_owned()))
        .unwrap();
    let before = runtime.coordinator().journal().latest_sequence().unwrap();
    assert!(
        runtime
            .set_engagement_scope(Some("x".repeat(1_000_000)))
            .is_err()
    );
    assert_eq!(
        runtime.coordinator().journal().latest_sequence().unwrap(),
        before
    );
    runtime.submit_user("inspect context").await.unwrap();
    assert!(
        provider.requests_for("main").last().unwrap().messages[0]
            .content
            .contains("scope: local fixture")
    );
    runtime.shutdown().await;
}

#[tokio::test]
async fn worker_panic_releases_the_subtree() {
    let dir = tempdir().unwrap();
    let provider = Arc::new(ScriptedProvider::new(HashMap::from([(
        "worker-01".to_owned(),
        VecDeque::from([Script::Panic]),
    )])));
    let runtime = TeamRuntime::create(
        dir.path().join("run"),
        dir.path().join("workspace"),
        "generic task",
        provider,
        RuntimeConfig::default(),
    )
    .await
    .unwrap();
    let lead = runtime
        .spawn_worker(&AgentId::main(), "lead", "coordinate")
        .await
        .unwrap();
    let leaf = runtime
        .spawn_worker(&lead, "leaf", "execute")
        .await
        .unwrap();
    runtime.set_auto(true);
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if runtime
                .coordinator()
                .inspect(&lead)
                .unwrap()
                .state
                .is_terminal()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    runtime.shutdown().await;
    assert!(result.is_ok(), "panicked worker must become terminal");
    assert!(
        runtime
            .coordinator()
            .inspect(&leaf)
            .unwrap()
            .state
            .is_terminal()
    );
    assert_eq!(runtime.coordinator().live_team().unwrap().len(), 1);
}
