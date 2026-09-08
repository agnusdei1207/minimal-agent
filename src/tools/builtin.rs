use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use crate::coordinator::AgentSnapshot;
use crate::domain::{AgentId, AgentState, Insight, InsightId, InsightLabel, MessageKind};
use crate::journal::JournalEvent;
use crate::provider::ToolDefinition;

use super::constants::{
    MAX_JOURNAL_REPLAY_BYTES, MAX_JOURNAL_REPLAY_EVENT_LIMIT, MAX_TOOL_RESULT_BYTES,
    MAX_WORKSPACE_LIST_ENTRIES, MAX_WORKSPACE_READ_BYTES,
};
use super::context::ToolContext;
use super::error::ToolError;
use super::names;
use super::shell::run_shell;
use super::truncate::{enforce_argument_limit, truncate_tool_content};
use super::types::{
    AssignInput, BashInput, BriefInput, CreateInput, FinishInput, InsightInput, InspectInput,
    JournalInput, RecallInput, ReportInput, SendInput, TmuxInput, ToolOutput, WaitInput,
    WorkerSpawner, WorkspaceInput,
};
use super::workspace::{resolve_existing, resolve_for_write};

#[derive(Clone)]
pub struct BuiltinTools {
    timeout: Duration,
    spawner: Arc<dyn WorkerSpawner>,
}

impl BuiltinTools {
    pub fn new(timeout: Duration, spawner: Arc<dyn WorkerSpawner>) -> Self {
        Self { timeout, spawner }
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        vec![
            definition(
                names::BASH,
                "Run one bash command in the workspace (bash -lc; the box is Linux with the full offensive toolset). Optional timeout_secs overrides the default 300s limit.",
                json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" },
                        "timeout_secs": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Max seconds to wait before timing out (default: 300). Use 5-30s for quick probes or curl, 120-600s for heavy scans or compilation."
                        }
                    },
                    "required": ["command"]
                }),
            ),
            definition(
                names::TMUX,
                "Drive OS-native tmux for interactive or long-lived work a one-shot `bash` \
command cannot hold — a real PTY (sudo/ssh/gdb prompts), a background listener \
(`nc -lvnp`), or a REPL you feed input to across turns. Your `args` string is run \
verbatim as `tmux <args>` through the same shell as `bash` (so pipes and redirects \
work); tmux owns the PTY and session state, keeping the turn loop unblocked. \
Core patterns: \
1) `new-session -d -s <name> \"<command>\"` — start a detached background PTY session; \
2) `send-keys -t <name> \"<input>\" Enter` — inject stdin/keystrokes with no live terminal; \
3) `capture-pane -p -t <name> | tail -n 50` — observe the current screen as clean text; \
4) `kill-session -t <name>` — reclaim the session when done. \
Sessions persist across calls: name them, capture to read output, and kill them when finished.",
                json!({"type":"object","properties":{"args":{"type":"string"}},"required":["args"]}),
            ),
            definition(
                names::WORKSPACE,
                "Read, write, or list files inside the workspace.",
                json!({"type":"object","properties":{"op":{"enum":["read","write","list"]},"path":{"type":"string"},"content":{"type":"string"}},"required":["op","path"]}),
            ),
            definition(
                names::TEAM,
                "Manage the team. Required fields per op: create={role,task}; \
assign={agent_id,role,task}; send={to:[agent_id],kind:progress|insight|request|final,body} \
(insight/final also take an optional insight); wait={} (optional timeout_ms); \
inspect={} (optional agent_id); recall={agent_id,reason}; finish={body}.",
                json!({
                    "type": "object",
                    "properties": {
                        "op": {"enum": ["create", "assign", "send", "wait", "inspect", "recall", "finish"]},
                        "role": {"type": "string"},
                        "task": {"type": "string"},
                        "agent_id": {"type": "string"},
                        "to": {"type": "array", "items": {"type": "string"}},
                        "kind": {"enum": ["progress", "insight", "request", "final"]},
                        "body": {"type": "string"},
                        "reason": {"type": "string"},
                        "timeout_ms": {"type": "integer", "minimum": 0}
                    },
                    "required": ["op"],
                    "additionalProperties": true
                }),
            ),
            definition(
                names::JOURNAL,
                "Explicitly inspect a bounded inclusive journal sequence range.",
                json!({"type":"object","properties":{"start":{"type":"integer","minimum":1},"end":{"type":"integer","minimum":1}},"required":["start","end"]}),
            ),
            definition(
                names::REPORT,
                "Record a finding; main may also record the final answer.",
                json!({"type":"object","properties":{"op":{"enum":["finding","final"]},"title":{"type":"string"},"body":{"type":"string"}},"required":["op","body"]}),
            ),
            definition(
                names::BRIEF,
                "Overwrite your battlefield note: the compact, categorized memory of \
this engagement that survives context compaction and is re-read every turn. \
Maintain it proactively as you work — group attempts by domain/vector, mark each \
tried / working / dead-end with the abstracted reason, keep exact values that \
matter (endpoints, payloads, offsets, credentials, flags, the command that \
worked or failed), and list open blockers and next moves. Replaces the whole \
note each call, so send the full current picture, not a fragment.",
                json!({"type":"object","properties":{"body":{"type":"string"}},"required":["body"]}),
            ),
        ]
    }

    pub async fn execute(
        &self,
        name: &str,
        arguments: Value,
        context: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        enforce_argument_limit(&arguments)?;
        let mut output = match name {
            names::BASH => self.bash(arguments, context).await,
            names::TMUX => self.tmux(arguments, context).await,
            names::WORKSPACE => self.workspace(arguments, context).await,
            names::TEAM => self.team(arguments, context).await,
            names::JOURNAL => self.journal(arguments, context),
            names::REPORT => self.report(arguments, context),
            names::BRIEF => self.record_brief(arguments, context),
            other => Err(ToolError::UnknownTool(other.to_owned())),
        }?;
        if output.content.len() > MAX_TOOL_RESULT_BYTES {
            return Err(ToolError::OutputLimit(MAX_TOOL_RESULT_BYTES));
        }
        // Bound the result for the model context so one large output cannot bloat
        // it (INTENT-0002 §3.15). The full output is not retained; the agent re-runs
        // with head/tail/grep or a file redirect when it needs more.
        output.content = truncate_tool_content(output.content);
        Ok(output)
    }

    async fn bash(&self, arguments: Value, context: &ToolContext) -> Result<ToolOutput, ToolError> {
        let input: BashInput = serde_json::from_value(arguments)?;
        let timeout = input
            .timeout_secs
            .map(Duration::from_secs)
            .unwrap_or(self.timeout);
        run_shell(input.command, context, timeout).await
    }

    /// Drive OS-native tmux with the same one-shot shell mechanism as `bash`.
    /// The agent's string is executed verbatim as `tmux <args>`, so tmux itself
    /// owns the PTY, session state, and lifecycle — the Rust core stays a thin
    /// passthrough and never re-implements a terminal emulator or IPC (INTENT-0003).
    async fn tmux(&self, arguments: Value, context: &ToolContext) -> Result<ToolOutput, ToolError> {
        let input: TmuxInput = serde_json::from_value(arguments)?;
        if input.args.trim().is_empty() {
            return Err(ToolError::InvalidArguments(
                "tmux args cannot be empty".to_owned(),
            ));
        }
        run_shell(format!("tmux {}", input.args), context, self.timeout).await
    }

    async fn workspace(
        &self,
        arguments: Value,
        context: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let input: WorkspaceInput = serde_json::from_value(arguments)?;
        match input.op.as_str() {
            names::OP_READ => {
                let path = resolve_existing(&context.workspace, &input.path)?;
                let bytes = tokio::fs::metadata(&path).await?.len();
                if bytes > MAX_WORKSPACE_READ_BYTES {
                    return Err(ToolError::InputLimit {
                        actual: bytes,
                        max: MAX_WORKSPACE_READ_BYTES,
                    });
                }
                let content = tokio::fs::read_to_string(path).await?;
                Ok(ToolOutput {
                    content,
                    success: true,
                })
            }
            names::OP_WRITE => {
                let content = input.content.ok_or_else(|| {
                    ToolError::InvalidArguments("workspace write requires content".to_owned())
                })?;
                let path = resolve_for_write(&context.workspace, &input.path)?;
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::write(&path, content.as_bytes()).await?;
                Ok(ToolOutput {
                    content: path.display().to_string(),
                    success: true,
                })
            }
            names::OP_LIST => {
                let path = resolve_existing(&context.workspace, &input.path)?;
                let mut reader = tokio::fs::read_dir(path).await?;
                let mut names = Vec::new();
                let mut name_bytes = 0_usize;
                while let Some(entry) = reader.next_entry().await? {
                    if names.len() >= MAX_WORKSPACE_LIST_ENTRIES {
                        return Err(ToolError::EntryLimit(MAX_WORKSPACE_LIST_ENTRIES));
                    }
                    let name = entry.file_name().to_string_lossy().into_owned();
                    name_bytes = name_bytes
                        .checked_add(name.len().saturating_add(4))
                        .filter(|bytes| *bytes <= MAX_TOOL_RESULT_BYTES)
                        .ok_or(ToolError::OutputLimit(MAX_TOOL_RESULT_BYTES))?;
                    names.push(name);
                }
                names.sort();
                Ok(ToolOutput {
                    content: serde_json::to_string(&names)?,
                    success: true,
                })
            }
            other => Err(ToolError::InvalidArguments(format!(
                "unknown workspace op: {other}"
            ))),
        }
    }

    async fn team(&self, arguments: Value, context: &ToolContext) -> Result<ToolOutput, ToolError> {
        let op = arguments
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidArguments("team op is required".to_owned()))?;
        let content = match op {
            names::OP_CREATE => {
                let input: CreateInput = serde_json::from_value(arguments)?;
                let id = self
                    .spawner
                    .spawn(&context.agent_id, input.role, input.task)
                    .await?;
                json!({"agent_id": id})
            }
            names::OP_ASSIGN => {
                let input: AssignInput = serde_json::from_value(arguments)?;
                let target = AgentId::new(input.agent_id)?;
                context
                    .coordinator
                    .reassign(&context.agent_id, &target, input.role, input.task)?;
                json!({"agent_id":target,"assigned":true})
            }
            names::OP_SEND => {
                let input: SendInput = serde_json::from_value(arguments)?;
                let audience = input
                    .to
                    .into_iter()
                    .map(AgentId::new)
                    .collect::<Result<Vec<_>, _>>()?;
                let kind = parse_message_kind(&input.kind)?;
                let insight = input.insight.map(parse_insight).transpose()?;
                let receipt = context.coordinator.send(
                    context.agent_id.clone(),
                    audience,
                    kind,
                    input.body,
                    insight,
                )?;
                json!({
                    "message_id":receipt.message_id,
                    "sequence":receipt.sequence,
                    "audience":receipt.audience,
                })
            }
            names::OP_WAIT => {
                let input: WaitInput = serde_json::from_value(arguments)?;
                let requested = Duration::from_millis(input.timeout_ms.unwrap_or(60_000));
                let timeout = requested.min(self.timeout);
                let messages = tokio::select! {
                    biased;
                    _ = context.cancellation.cancelled() => return Err(ToolError::Cancelled),
                    messages = context.coordinator.wait_for_messages(&context.agent_id, timeout) => {
                        messages?
                    }
                };
                json!({"ready":true,"unread":messages.len()})
            }
            names::OP_INSPECT => {
                let input: InspectInput = serde_json::from_value(arguments)?;
                match input.agent_id {
                    Some(id) => snapshot_json(&context.coordinator.inspect(&AgentId::new(id)?)?),
                    None => {
                        json!({"team":context.coordinator.live_team()?.iter().map(snapshot_json).collect::<Vec<_>>()})
                    }
                }
            }
            names::OP_RECALL => {
                let input: RecallInput = serde_json::from_value(arguments)?;
                let target = AgentId::new(input.agent_id)?;
                context
                    .coordinator
                    .recall(&context.agent_id, &target, input.reason)?;
                json!({"agent_id":target,"recalling":true})
            }
            names::OP_FINISH => {
                if context.agent_id.is_main() {
                    return Err(ToolError::InvalidArguments(
                        "main records its final answer with report".to_owned(),
                    ));
                }
                let input: FinishInput = serde_json::from_value(arguments)?;
                // A worker's final result bubbles up to its direct parent, which
                // synthesizes it upward (INTENT-0004 §3.4). Non-main agents always
                // have a parent.
                let parent = context
                    .coordinator
                    .inspect(&context.agent_id)?
                    .parent
                    .into_iter()
                    .collect::<Vec<_>>();
                context.coordinator.send(
                    context.agent_id.clone(),
                    parent,
                    MessageKind::Final,
                    input.body,
                    None,
                )?;
                context.coordinator.mark_terminal(
                    &context.agent_id,
                    &context.agent_id,
                    AgentState::Finished,
                    "worker finished",
                )?;
                json!({"finished":true})
            }
            other => {
                return Err(ToolError::InvalidArguments(format!(
                    "unknown team op: {other}"
                )));
            }
        };
        Ok(ToolOutput {
            content: content.to_string(),
            success: true,
        })
    }

    fn journal(&self, arguments: Value, context: &ToolContext) -> Result<ToolOutput, ToolError> {
        let input: JournalInput = serde_json::from_value(arguments)?;
        if input.start == 0
            || input.start > input.end
            || input.end - input.start > MAX_JOURNAL_REPLAY_EVENT_LIMIT
        {
            return Err(ToolError::InvalidArguments(
                "journal range must be inclusive, ordered, and at most 1,001 events".to_owned(),
            ));
        }
        let events = context
            .journal
            .replay_range(input.start, input.end, MAX_JOURNAL_REPLAY_BYTES)?
            .into_iter()
            .map(|entry| {
                json!({
                    "sequence":entry.sequence,
                    "recorded_at":entry.recorded_at,
                    "event":entry.event,
                })
            })
            .collect::<Vec<_>>();
        Ok(ToolOutput {
            content: serde_json::to_string(&events)?,
            success: true,
        })
    }

    fn report(&self, arguments: Value, context: &ToolContext) -> Result<ToolOutput, ToolError> {
        let input: ReportInput = serde_json::from_value(arguments)?;
        let event = match input.op.as_str() {
            names::OP_FINDING => JournalEvent::Finding {
                agent_id: context.agent_id.clone(),
                title: input.title.unwrap_or_else(|| "Finding".to_owned()),
                body: input.body,
            },
            names::OP_FINAL if context.agent_id.is_main() => JournalEvent::Final {
                agent_id: context.agent_id.clone(),
                body: input.body,
            },
            names::OP_FINAL => {
                return Err(ToolError::InvalidArguments(
                    "workers deliver final summaries with team finish".to_owned(),
                ));
            }
            other => {
                return Err(ToolError::InvalidArguments(format!(
                    "unknown report op: {other}"
                )));
            }
        };
        let ack = context.journal.append_sync(event)?;
        Ok(ToolOutput {
            content: json!({"sequence":ack.sequence,"recorded":true}).to_string(),
            success: true,
        })
    }

    fn record_brief(
        &self,
        arguments: Value,
        context: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let input: BriefInput = serde_json::from_value(arguments)?;
        context
            .briefs
            .write_note(&context.agent_id, &context.agent_id, &input.body)
            .map_err(|error| {
                ToolError::InvalidArguments(format!("battlefield note rejected: {error}"))
            })?;
        Ok(ToolOutput {
            content: json!({"recorded": true, "battlefield": "updated"}).to_string(),
            success: true,
        })
    }
}

fn definition(name: &str, description: &str, parameters: Value) -> ToolDefinition {
    ToolDefinition {
        name: name.to_owned(),
        description: description.to_owned(),
        parameters,
    }
}

fn parse_message_kind(value: &str) -> Result<MessageKind, ToolError> {
    MessageKind::parse(value)
        .ok_or_else(|| ToolError::InvalidArguments(format!("unknown message kind: {value}")))
}

fn parse_insight(input: InsightInput) -> Result<Insight, ToolError> {
    let label = InsightLabel::parse(&input.label).ok_or_else(|| {
        ToolError::InvalidArguments(format!("unknown insight label: {}", input.label))
    })?;
    Ok(Insight::new(InsightId::new(input.id)?, label, input.text))
}

fn snapshot_json(snapshot: &AgentSnapshot) -> Value {
    json!({
        "id":snapshot.id,
        "role":snapshot.role,
        "task":snapshot.task,
        "state":format!("{:?}",snapshot.state).to_ascii_uppercase(),
        "unread":snapshot.unread,
        "latest_insight":snapshot.latest_insight,
        "waiting_on":snapshot.waiting_on,
    })
}
