// Temporary diagnostic probes for the 2026-09-05 audit, not acceptance tests.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use async_trait::async_trait;
use minimal_agent::domain::AgentId;
use minimal_agent::journal::JournalEvent;
use minimal_agent::provider::{ModelDelta, ModelProvider, ModelRequest, ModelTurn, ProviderFault, ToolCall};
use minimal_agent::runtime::{RuntimeConfig, RuntimeEvent, TeamRuntime};

struct ProbeProvider { calls: AtomicUsize, flood: bool }

#[async_trait]
impl ModelProvider for ProbeProvider {
    fn model_id(&self) -> &str { "local-audit-probe" }
    fn context_limit(&self) -> u64 { 128_000 }
    async fn complete(&self, _: ModelRequest, deltas: Option<tokio::sync::mpsc::UnboundedSender<ModelDelta>>) -> Result<ModelTurn, ProviderFault> {
        let first = self.calls.fetch_add(1, Ordering::SeqCst) == 0;
        if !first && self.flood {
            if let Some(sender) = deltas {
                for _ in 0..2048 { let _ = sender.send(ModelDelta::Text("x".to_owned())); }
            }
        }
        Ok(ModelTurn {
            text: if first { String::new() } else { "done".to_owned() },
            tool_calls: if first { vec![ToolCall {
                id: "probe-read".to_owned(), name: "workspace".to_owned(),
                arguments: serde_json::json!({"op":"read","path":"sample.txt"}),
            }] } else { vec![] },
            usage: None, finish_reason: Some("stop".to_owned()),
        })
    }
}

async fn runtime(root: &std::path::Path, flood: bool) -> TeamRuntime {
    TeamRuntime::create(root.join("run"), root.join("workspace"), "audit local fixture", Arc::new(ProbeProvider {calls: AtomicUsize::new(0), flood}), RuntimeConfig::default()).await.unwrap()
}

#[tokio::test]
async fn audit_nested_spawn_fails_and_leaves_a_live_unstarted_child() {
    let dir = tempfile::tempdir().unwrap();
    let rt = runtime(dir.path(), false).await;
    let lead = rt.spawn_worker(&AgentId::main(), "lead", "local fixture").await.unwrap();
    let result = rt.spawn_worker(&lead, "leaf", "local fixture").await;
    assert!(result.is_err());
    assert_eq!(rt.coordinator().live_team().unwrap().len(), 3);
    println!("CONFIRMED: depth-1 spawn errors but leaves 3 live agents: {}", result.unwrap_err());
    rt.shutdown().await;
}

#[tokio::test]
async fn audit_middle_of_tool_output_is_missing_from_durable_journal() {
    let dir = tempfile::tempdir().unwrap();
    let rt = runtime(dir.path(), false).await;
    let marker = "AUDIT_EXACT_EVIDENCE_IN_MIDDLE";
    let content = format!("{}{}{}", "a".repeat(20_000), marker, "b".repeat(20_000));
    std::fs::write(dir.path().join("workspace/sample.txt"), content).unwrap();
    rt.submit_user("read sample.txt").await.unwrap();
    let replay = rt.coordinator().journal().replay().unwrap();
    let outputs: Vec<_> = replay.iter().filter_map(|entry| match &entry.event {
        JournalEvent::ToolResult { content, .. } => Some(content), _ => None,
    }).collect();
    assert_eq!(outputs.len(), 1);
    assert!(!outputs[0].contains(marker));
    assert!(outputs[0].contains("bytes omitted"));
    println!("CONFIRMED: 40KB tool output middle evidence absent from ToolResult journal");
    rt.shutdown().await;
}

#[tokio::test]
async fn audit_headless_style_delayed_subscription_loses_tool_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let rt = runtime(dir.path(), true).await;
    std::fs::write(dir.path().join("workspace/sample.txt"), "AUDIT_FLAG_EVIDENCE").unwrap();
    let mut events = rt.subscribe();
    rt.submit_user("read sample.txt").await.unwrap();
    let mut lagged = false;
    let mut saw_tool = false;
    loop {
        match events.try_recv() {
            Ok(RuntimeEvent::ToolFinished { .. }) => saw_tool = true,
            Ok(_) => {},
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => lagged = true,
            Err(_) => break,
        }
    }
    assert!(lagged);
    assert!(!saw_tool);
    assert!(rt.coordinator().journal().replay().unwrap().iter().any(|entry| matches!(&entry.event, JournalEvent::ToolResult {content, ..} if content.contains("AUDIT_FLAG_EVIDENCE"))));
    println!("CONFIRMED: headless receive-after-submit pattern loses tool event after 2048 deltas, journal retains evidence");
    rt.shutdown().await;
}
