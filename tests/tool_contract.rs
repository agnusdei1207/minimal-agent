use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use pentesting::coordinator::AgentCoordinator;
use pentesting::domain::AgentId;
use pentesting::journal::{JournalConfig, RunJournal};
use pentesting::tools::{BuiltinTools, ToolContext, ToolError, WorkerSpawner};
use tempfile::tempdir;

struct DirectSpawner {
    coordinator: AgentCoordinator,
}

#[async_trait]
impl WorkerSpawner for DirectSpawner {
    async fn spawn(
        &self,
        caller: &AgentId,
        role: String,
        task: String,
    ) -> Result<AgentId, ToolError> {
        Ok(self.coordinator.create_worker(caller, role, task)?)
    }
}

fn setup() -> (
    tempfile::TempDir,
    Arc<RunJournal>,
    AgentCoordinator,
    BuiltinTools,
) {
    let dir = tempdir().unwrap();
    let journal =
        Arc::new(RunJournal::open(dir.path().join("run"), JournalConfig::default()).unwrap());
    let coordinator = AgentCoordinator::new(journal.clone(), "goal").unwrap();
    let spawner = Arc::new(DirectSpawner {
        coordinator: coordinator.clone(),
    });
    let tools = BuiltinTools::new(Duration::from_secs(3), spawner);
    (dir, journal, coordinator, tools)
}

#[tokio::test]
async fn workspace_tool_reads_and_writes_inside_the_workspace_only() {
    let (dir, journal, coordinator, tools) = setup();
    let context = ToolContext::new(
        AgentId::main(),
        dir.path().join("workspace"),
        coordinator,
        journal,
    )
    .unwrap();
    let write = tools
        .execute(
            "workspace",
            serde_json::json!({"op":"write", "path":"notes/result.txt", "content":"hello"}),
            &context,
        )
        .await
        .unwrap();
    assert!(write.success);
    let read = tools
        .execute(
            "workspace",
            serde_json::json!({"op":"read", "path":"notes/result.txt"}),
            &context,
        )
        .await
        .unwrap();
    assert_eq!(read.content, "hello");
    assert!(matches!(
        tools
            .execute(
                "workspace",
                serde_json::json!({"op":"read", "path":"../outside.txt"}),
                &context,
            )
            .await,
        Err(ToolError::OutsideWorkspace(_))
    ));
}

#[tokio::test]
async fn shell_runs_in_the_workspace_with_timeout_and_cancellation() {
    let (dir, journal, coordinator, tools) = setup();
    let context = ToolContext::new(
        AgentId::main(),
        dir.path().join("workspace"),
        coordinator,
        journal,
    )
    .unwrap();
    let output = tools
        .execute(
            "bash",
            serde_json::json!({"command":"printf minimal-agent"}),
            &context,
        )
        .await
        .unwrap();
    assert!(output.success);
    assert_eq!(output.content, "minimal-agent");

    context.cancellation.cancel();
    assert!(matches!(
        tools
            .execute(
                "bash",
                serde_json::json!({"command":"printf never"}),
                &context,
            )
            .await,
        Err(ToolError::Cancelled)
    ));
}

#[tokio::test]
async fn bash_honors_dynamic_timeout_secs_override() {
    let (dir, journal, coordinator, tools) = setup();
    let context = ToolContext::new(
        AgentId::main(),
        dir.path().join("workspace"),
        coordinator,
        journal,
    )
    .unwrap();

    let res = tools
        .execute(
            "bash",
            serde_json::json!({
                "command": "sleep 3",
                "timeout_secs": 1
            }),
            &context,
        )
        .await;

    assert!(matches!(res, Err(ToolError::TimedOut(dur)) if dur == Duration::from_secs(1)));
}

#[tokio::test]
async fn tmux_tool_is_registered_as_a_thin_args_passthrough_with_the_core_patterns() {
    let (_dir, _journal, _coordinator, tools) = setup();
    let definition = tools
        .definitions()
        .into_iter()
        .find(|definition| definition.name == "tmux")
        .expect("tmux tool is registered");
    // Minimal, general schema: a single `args` string executed as `tmux <args>`.
    let parameters = definition.parameters.to_string();
    assert!(parameters.contains("args"));
    assert!(!parameters.contains("command"));
    // The description carries the four load-bearing patterns, bash-tool quality.
    let description = definition.description;
    assert!(description.contains("new-session"));
    assert!(description.contains("send-keys"));
    assert!(description.contains("capture-pane"));
    assert!(description.contains("kill-session"));
}

#[tokio::test]
async fn tmux_rejects_empty_args_before_spawning_and_honors_cancellation() {
    let (dir, journal, coordinator, tools) = setup();
    let context = ToolContext::new(
        AgentId::main(),
        dir.path().join("workspace"),
        coordinator,
        journal,
    )
    .unwrap();
    // Empty args are rejected before any process is spawned, so this holds even
    // where the tmux binary is absent (e.g. the build image).
    assert!(matches!(
        tools
            .execute("tmux", serde_json::json!({ "args": "   " }), &context)
            .await,
        Err(ToolError::InvalidArguments(_))
    ));

    // tmux reuses the shared shell mechanism, so cancellation is observed before spawn.
    context.cancellation.cancel();
    assert!(matches!(
        tools
            .execute(
                "tmux",
                serde_json::json!({ "args": "kill-server" }),
                &context,
            )
            .await,
        Err(ToolError::Cancelled)
    ));
}

#[tokio::test]
async fn team_wait_obeys_recall_and_cannot_hold_a_worker_until_its_requested_timeout() {
    let (dir, journal, coordinator, tools) = setup();
    let worker = coordinator
        .create_worker(&AgentId::main(), "waiter", "wait for message")
        .unwrap();
    let context = ToolContext::new(
        worker.clone(),
        dir.path().join("workspace"),
        coordinator.clone(),
        journal,
    )
    .unwrap();
    let waiting_tools = tools.clone();
    let waiting = tokio::spawn(async move {
        waiting_tools
            .execute(
                "team",
                serde_json::json!({"op":"wait","timeout_ms":3_600_000_u64}),
                &context,
            )
            .await
    });
    tokio::task::yield_now().await;
    coordinator
        .recall(&AgentId::main(), &worker, "stop waiting")
        .unwrap();

    assert!(matches!(
        tokio::time::timeout(Duration::from_millis(500), waiting)
            .await
            .unwrap()
            .unwrap(),
        Err(ToolError::Cancelled)
    ));
}

#[tokio::test]
async fn shell_output_limit_stops_the_command_instead_of_buffering_without_bound() {
    let (dir, journal, coordinator, tools) = setup();
    let context = ToolContext::new(
        AgentId::main(),
        dir.path().join("workspace"),
        coordinator,
        journal,
    )
    .unwrap();
    let command = if cfg!(windows) {
        "$s='x' * 1100000; [Console]::Out.Write($s)"
    } else {
        "yes x | head -c 1100000"
    };

    assert!(
        tools
            .execute("bash", serde_json::json!({"command":command}), &context,)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_large_but_within_limit_output_is_truncated_head_and_tail_not_rejected() {
    let (dir, journal, coordinator, tools) = setup();
    let context = ToolContext::new(
        AgentId::main(),
        dir.path().join("workspace"),
        coordinator,
        journal,
    )
    .unwrap();
    // ~50 KB: under the 128 KiB hard cap but over the context bound, so it is
    // truncated to head + tail with an elision marker rather than fed raw or rejected.
    let command = if cfg!(windows) {
        "$s='x' * 50000; [Console]::Out.Write($s)"
    } else {
        "yes x | head -c 50000"
    };
    let output = tools
        .execute("bash", serde_json::json!({ "command": command }), &context)
        .await
        .unwrap();
    assert!(output.success);
    assert!(output.content.contains("omitted"));
    assert!(output.content.len() < 50_000);
}

#[tokio::test]
async fn workspace_large_read_is_rejected_instead_of_loading_the_whole_file() {
    let (dir, journal, coordinator, tools) = setup();
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::write(workspace.join("large.txt"), vec![b'x'; 1024 * 1024 + 1]).unwrap();
    let context = ToolContext::new(AgentId::main(), &workspace, coordinator, journal).unwrap();

    assert!(
        tools
            .execute(
                "workspace",
                serde_json::json!({"op":"read", "path":"large.txt"}),
                &context,
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn oversized_tool_arguments_are_rejected_before_side_effects() {
    let (dir, journal, coordinator, tools) = setup();
    let workspace = dir.path().join("workspace");
    let context = ToolContext::new(AgentId::main(), &workspace, coordinator, journal).unwrap();

    assert!(matches!(
        tools
            .execute(
                "workspace",
                serde_json::json!({
                    "op":"write",
                    "path":"must-not-exist.txt",
                    "content":"x".repeat(128 * 1024 + 1),
                }),
                &context,
            )
            .await,
        Err(ToolError::ArgumentLimit(131_072))
    ));
    assert!(!workspace.join("must-not-exist.txt").exists());
}

#[tokio::test]
async fn one_team_tool_creates_and_connects_sibling_agents() {
    let (dir, journal, coordinator, tools) = setup();
    let main_context = ToolContext::new(
        AgentId::main(),
        dir.path().join("workspace"),
        coordinator.clone(),
        journal.clone(),
    )
    .unwrap();
    let a: serde_json::Value = serde_json::from_str(
        &tools
            .execute(
                "team",
                serde_json::json!({"op":"create", "role":"recon", "task":"map service"}),
                &main_context,
            )
            .await
            .unwrap()
            .content,
    )
    .unwrap();
    let b: serde_json::Value = serde_json::from_str(
        &tools
            .execute(
                "team",
                serde_json::json!({"op":"create", "role":"analysis", "task":"check lead"}),
                &main_context,
            )
            .await
            .unwrap()
            .content,
    )
    .unwrap();
    let worker_a = AgentId::new(a["agent_id"].as_str().unwrap()).unwrap();
    let worker_b = AgentId::new(b["agent_id"].as_str().unwrap()).unwrap();
    let a_context = ToolContext::new(
        worker_a.clone(),
        dir.path().join("workspace"),
        coordinator.clone(),
        journal,
    )
    .unwrap();
    tools
        .execute(
            "team",
            serde_json::json!({
                "op":"send",
                "to":[worker_b.as_str()],
                "kind":"insight",
                "body":"alternate route",
                "insight":{"id":"I-route", "label":"DIRECTION", "text":"try /admin"}
            }),
            &a_context,
        )
        .await
        .unwrap();
    assert_eq!(coordinator.inbox(&worker_b).unwrap().len(), 1);
    // INTENT-0004: a sibling insight is delivered only to the addressed sibling, not
    // auto-copied to main; main learns of it through the parent's synthesis.
    assert_eq!(coordinator.inbox(&AgentId::main()).unwrap().len(), 0);
}

#[tokio::test]
async fn journal_and_report_are_explicit_structured_tools() {
    let (dir, journal, coordinator, tools) = setup();
    let context = ToolContext::new(
        AgentId::main(),
        dir.path().join("workspace"),
        coordinator,
        journal.clone(),
    )
    .unwrap();
    tools
        .execute(
            "report",
            serde_json::json!({"op":"finding", "title":"reachable", "body":"service responded"}),
            &context,
        )
        .await
        .unwrap();
    let inspected = tools
        .execute(
            "journal",
            serde_json::json!({"start":1, "end":100}),
            &context,
        )
        .await
        .unwrap();
    assert!(inspected.content.contains("\"type\":\"finding\""));
    assert!(journal.replay().unwrap().len() >= 2);
}

#[tokio::test]
async fn workers_cannot_bypass_team_delivery_with_report_final() {
    let (dir, journal, coordinator, tools) = setup();
    let worker = coordinator
        .create_worker(&AgentId::main(), "worker", "report through the team")
        .unwrap();
    let context = ToolContext::new(
        worker,
        dir.path().join("workspace"),
        coordinator,
        journal.clone(),
    )
    .unwrap();

    assert!(matches!(
        tools
            .execute(
                "report",
                serde_json::json!({"op":"final", "body":"must reach main"}),
                &context,
            )
            .await,
        Err(ToolError::InvalidArguments(_))
    ));
    assert!(
        !journal
            .replay()
            .unwrap()
            .iter()
            .any(|entry| matches!(entry.event, pentesting::journal::JournalEvent::Final { .. }))
    );
}

#[tokio::test]
async fn brief_replacement_is_not_a_tool_side_door_around_semantic_compaction() {
    let (dir, journal, coordinator, tools) = setup();
    let context = ToolContext::new(
        AgentId::main(),
        dir.path().join("workspace"),
        coordinator,
        journal,
    )
    .unwrap();

    assert!(matches!(
        tools
            .execute(
                "report",
                serde_json::json!({"op":"brief", "body":"unverified replacement"}),
                &context,
            )
            .await,
        Err(ToolError::InvalidArguments(_))
    ));
    assert!(
        tools
            .definitions()
            .iter()
            .find(|definition| definition.name == "report")
            .unwrap()
            .parameters
            .to_string()
            .contains("finding")
    );
    assert!(
        !tools
            .definitions()
            .iter()
            .find(|definition| definition.name == "report")
            .unwrap()
            .parameters
            .to_string()
            .contains("brief")
    );
}

#[tokio::test]
async fn brief_tool_persists_agent_battlefield_note_for_injection() {
    let (dir, journal, coordinator, tools) = setup();
    let budget = pentesting::domain::ContextBudget::new(128_000, 128_000, 8_000).unwrap();
    let briefs = pentesting::brief::AgentBriefStore::new(
        dir.path().join("run-briefs"),
        journal.clone(),
        budget,
    );
    let context = ToolContext::with_briefs(
        AgentId::main(),
        dir.path().join("workspace"),
        coordinator,
        journal,
        briefs.clone(),
    )
    .unwrap();

    // No note yet: read_effective falls back to the structured template.
    assert!(briefs.read_note(&AgentId::main()).unwrap().is_none());

    let note = "## Battlefield\nauth/session: login IDOR via user_id -> WORKING\n\
injection: sqli on /search -> dead end (WAF 403)\nnext: escalate IDOR to admin";
    let out = tools
        .execute("brief", serde_json::json!({ "body": note }), &context)
        .await
        .unwrap();
    assert!(out.success);

    // The agent-authored note is exactly what gets injected as CURRENT BRIEF and
    // shown in /status, decoupled from coverage-proven compaction.
    assert_eq!(briefs.read_effective(&AgentId::main()).unwrap(), note);
    assert_eq!(
        briefs.read_note(&AgentId::main()).unwrap().as_deref(),
        Some(note)
    );
}
