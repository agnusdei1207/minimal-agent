use tokio::sync::broadcast;

use crate::domain::{AgentId, MessageKind};
use crate::provider::ModelDelta;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnResult {
    pub text: String,
    pub finalized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEvent {
    TurnStarted {
        agent_id: AgentId,
    },
    TurnFinished {
        agent_id: AgentId,
        success: bool,
    },
    Delta {
        agent_id: AgentId,
        delta: ModelDelta,
    },
    ToolStarted {
        agent_id: AgentId,
        name: String,
        /// Short, bounded human summary of the call target (e.g. the shell
        /// command or the workspace path) for clean transcript rendering.
        summary: Option<String>,
    },
    ToolFinished {
        agent_id: AgentId,
        name: String,
        success: bool,
        output: String,
    },
    Assistant {
        agent_id: AgentId,
        text: String,
    },
    AgentMessage {
        sender: AgentId,
        recipients: Vec<AgentId>,
        kind: MessageKind,
        body: String,
    },
    TeamChanged,
    Fault {
        agent_id: AgentId,
        message: String,
    },
}

pub fn record_model_delta(
    partial: &mut String,
    events: &broadcast::Sender<RuntimeEvent>,
    agent_id: &AgentId,
    delta: ModelDelta,
) {
    if let ModelDelta::Text(text) = &delta {
        partial.push_str(text);
    }
    record_usage_telemetry(&delta);
    let _ = events.send(RuntimeEvent::Delta {
        agent_id: agent_id.clone(),
        delta,
    });
}

/// Benchmark-only, env-gated token telemetry (INTENT-0002 §3.13). When
/// `PENTESTING_TELEMETRY_FILE` names a file, append one JSONL record per model
/// response so an external benchmark harness can total tokens; a no-op otherwise.
/// This is removable benchmark support — delete this fn and its one call site.
pub fn record_usage_telemetry(delta: &ModelDelta) {
    let ModelDelta::Usage(usage) = delta else {
        return;
    };
    if crate::settings::debug_enabled() {
        eprintln!(
            "[debug] [telemetry] Model tokens: prompt={}, completion={}",
            usage.input_tokens, usage.output_tokens
        );
    }
    let Ok(path) = std::env::var("PENTESTING_TELEMETRY_FILE") else {
        return;
    };
    let line = format!(
        "{{\"event\":\"response\",\"prompt_tokens\":{},\"completion_tokens\":{}}}\n",
        usage.input_tokens, usage.output_tokens,
    );
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write;
        let _ = file.write_all(line.as_bytes());
    }
}
