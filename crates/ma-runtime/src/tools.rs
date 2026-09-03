use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use ma_context::brief::AgentBriefStore;
use ma_coordinator::{AgentCoordinator, CoordinatorError};
use ma_core::domain::{
    AgentId, AgentState, ContextBudget, Insight, InsightId, InsightLabel, MessageKind,
};
use ma_journal::{JournalError, JournalEvent, RunJournal};
use ma_provider::provider::ToolDefinition;

const MAX_TOOL_RESULT_BYTES: usize = 128 * 1024;
/// Context-friendly bound for a tool result fed to the model. Larger outputs are
/// truncated to a head + tail with an elision marker so one big output cannot
/// bloat the context (ADR-0002 §3.15). Well below `MAX_TOOL_RESULT_BYTES`.
const MAX_TOOL_CONTEXT_BYTES: usize = 16 * 1024;
const MAX_TOOL_ARGUMENT_BYTES: usize = MAX_TOOL_RESULT_BYTES;
const MAX_BASH_STREAM_BYTES: usize = MAX_TOOL_RESULT_BYTES;
const MAX_WORKSPACE_READ_BYTES: u64 = MAX_TOOL_RESULT_BYTES as u64;
const MAX_WORKSPACE_LIST_ENTRIES: usize = 10_000;
const MAX_JOURNAL_REPLAY_BYTES: usize = MAX_TOOL_RESULT_BYTES;

#[async_trait]
pub trait WorkerSpawner: Send + Sync {
    async fn spawn(
        &self,
        caller: &AgentId,
        role: String,
        task: String,
    ) -> Result<AgentId, ToolError>;
}

#[derive(Clone)]
pub struct ToolContext {
    pub agent_id: AgentId,
    pub workspace: PathBuf,
    pub coordinator: AgentCoordinator,
    pub journal: Arc<RunJournal>,
    pub briefs: AgentBriefStore,
    pub cancellation: CancellationToken,
}

impl ToolContext {
    /// Full constructor: shares the runtime's brief store so `brief` notes are
    /// visible to prompt-building and `/status`.
    pub fn with_briefs(
        agent_id: AgentId,
        workspace: impl AsRef<Path>,
        coordinator: AgentCoordinator,
        journal: Arc<RunJournal>,
        briefs: AgentBriefStore,
    ) -> Result<Self, ToolError> {
        std::fs::create_dir_all(workspace.as_ref())?;
        let workspace = workspace.as_ref().canonicalize()?;
        let cancellation = coordinator.cancellation_token(&agent_id)?;
        Ok(Self {
            agent_id,
            workspace,
            coordinator,
            journal,
            briefs,
            cancellation,
        })
    }

    /// Convenience constructor that pairs the context with a private brief store
    /// rooted under the workspace. Used where no shared store is threaded in
    /// (tool-level tests); the runtime uses `with_briefs` to share its own.
    pub fn new(
        agent_id: AgentId,
        workspace: impl AsRef<Path>,
        coordinator: AgentCoordinator,
        journal: Arc<RunJournal>,
    ) -> Result<Self, ToolError> {
        std::fs::create_dir_all(workspace.as_ref())?;
        let budget = ContextBudget::new(128_000, 128_000, 8_000)
            .map_err(|error| ToolError::InvalidArguments(error.to_string()))?;
        let briefs = AgentBriefStore::new(workspace.as_ref().join(".briefs"), journal.clone(), budget);
        Self::with_briefs(agent_id, workspace, coordinator, journal, briefs)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    pub content: String,
    pub success: bool,
}

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
                "bash",
                "Run one bash command in the workspace (bash -lc; the box is Linux with the full offensive toolset).",
                json!({"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}),
            ),
            definition(
                "workspace",
                "Read, write, or list files inside the workspace.",
                json!({"type":"object","properties":{"op":{"enum":["read","write","list"]},"path":{"type":"string"},"content":{"type":"string"}},"required":["op","path"]}),
            ),
            definition(
                "team",
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
                "journal",
                "Explicitly inspect a bounded inclusive journal sequence range.",
                json!({"type":"object","properties":{"start":{"type":"integer","minimum":1},"end":{"type":"integer","minimum":1}},"required":["start","end"]}),
            ),
            definition(
                "report",
                "Record a finding; main may also record the final answer.",
                json!({"type":"object","properties":{"op":{"enum":["finding","final"]},"title":{"type":"string"},"body":{"type":"string"}},"required":["op","body"]}),
            ),
            definition(
                "brief",
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
            "bash" => self.bash(arguments, context).await,
            "workspace" => self.workspace(arguments, context).await,
            "team" => self.team(arguments, context).await,
            "journal" => self.journal(arguments, context),
            "report" => self.report(arguments, context),
            "brief" => self.record_brief(arguments, context),
            other => Err(ToolError::UnknownTool(other.to_owned())),
        }?;
        if output.content.len() > MAX_TOOL_RESULT_BYTES {
            return Err(ToolError::OutputLimit(MAX_TOOL_RESULT_BYTES));
        }
        // Bound the result for the model context so one large output cannot bloat
        // it (ADR-0002 §3.15). The full output is not retained; the agent re-runs
        // with head/tail/grep or a file redirect when it needs more.
        output.content = truncate_tool_content(output.content);
        Ok(output)
    }

    async fn bash(
        &self,
        arguments: Value,
        context: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let input: BashInput = serde_json::from_value(arguments)?;
        if input.command.trim().is_empty() {
            return Err(ToolError::InvalidArguments(
                "shell command cannot be empty".to_owned(),
            ));
        }
        if context.cancellation.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        let mut command = platform_shell(&input.command);
        command
            .current_dir(&context.workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ToolError::Process("shell stdout was not piped".to_owned()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ToolError::Process("shell stderr was not piped".to_owned()))?;
        let deadline = tokio::time::Instant::now() + self.timeout;
        let streams = tokio::select! {
            _ = context.cancellation.cancelled() => {
                stop_child(&mut child).await;
                return Err(ToolError::Cancelled);
            },
            result = tokio::time::timeout_at(deadline, async {
                tokio::try_join!(
                    read_bounded(stdout, MAX_BASH_STREAM_BYTES),
                    read_bounded(stderr, MAX_BASH_STREAM_BYTES),
                )
            }) => match result {
                Ok(Ok(streams)) => streams,
                Ok(Err(error)) => {
                    stop_child(&mut child).await;
                    return Err(error);
                }
                Err(_) => {
                    stop_child(&mut child).await;
                    return Err(ToolError::TimedOut(self.timeout));
                }
            }
        };
        let status = tokio::select! {
            _ = context.cancellation.cancelled() => {
                stop_child(&mut child).await;
                return Err(ToolError::Cancelled);
            },
            result = tokio::time::timeout_at(deadline, child.wait()) => match result {
                Ok(status) => status?,
                Err(_) => {
                    stop_child(&mut child).await;
                    return Err(ToolError::TimedOut(self.timeout));
                }
            }
        };
        let (stdout, stderr) = streams;
        let mut content = String::from_utf8_lossy(&stdout).into_owned();
        if !stderr.is_empty() {
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(&String::from_utf8_lossy(&stderr));
        }
        Ok(ToolOutput {
            content,
            success: status.success(),
        })
    }

    async fn workspace(
        &self,
        arguments: Value,
        context: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let input: WorkspaceInput = serde_json::from_value(arguments)?;
        match input.op.as_str() {
            "read" => {
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
            "write" => {
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
            "list" => {
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
            "create" => {
                let input: CreateInput = serde_json::from_value(arguments)?;
                let id = self
                    .spawner
                    .spawn(&context.agent_id, input.role, input.task)
                    .await?;
                json!({"agent_id": id})
            }
            "assign" => {
                let input: AssignInput = serde_json::from_value(arguments)?;
                let target = AgentId::new(input.agent_id)?;
                context
                    .coordinator
                    .reassign(&context.agent_id, &target, input.role, input.task)?;
                json!({"agent_id":target,"assigned":true})
            }
            "send" => {
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
            "wait" => {
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
            "inspect" => {
                let input: InspectInput = serde_json::from_value(arguments)?;
                match input.agent_id {
                    Some(id) => snapshot_json(&context.coordinator.inspect(&AgentId::new(id)?)?),
                    None => {
                        json!({"team":context.coordinator.live_team()?.iter().map(snapshot_json).collect::<Vec<_>>()})
                    }
                }
            }
            "recall" => {
                let input: RecallInput = serde_json::from_value(arguments)?;
                let target = AgentId::new(input.agent_id)?;
                context
                    .coordinator
                    .recall(&context.agent_id, &target, input.reason)?;
                json!({"agent_id":target,"recalling":true})
            }
            "finish" => {
                if context.agent_id.is_main() {
                    return Err(ToolError::InvalidArguments(
                        "main records its final answer with report".to_owned(),
                    ));
                }
                let input: FinishInput = serde_json::from_value(arguments)?;
                // A worker's final result bubbles up to its direct parent, which
                // synthesizes it upward (ADR-0004 §3.4). Non-main agents always
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
        if input.start == 0 || input.start > input.end || input.end - input.start > 1_000 {
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
            "finding" => JournalEvent::Finding {
                agent_id: context.agent_id.clone(),
                title: input.title.unwrap_or_else(|| "Finding".to_owned()),
                body: input.body,
            },
            "final" if context.agent_id.is_main() => JournalEvent::Final {
                agent_id: context.agent_id.clone(),
                body: input.body,
            },
            "final" => {
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

#[derive(Debug, Deserialize)]
struct BriefInput {
    body: String,
}

struct BoundedJsonCounter {
    bytes: usize,
    exceeded: bool,
}

impl Write for BoundedJsonCounter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let Some(total) = self.bytes.checked_add(buffer.len()) else {
            self.exceeded = true;
            return Err(io::Error::other("tool arguments exceeded their byte limit"));
        };
        if total > MAX_TOOL_ARGUMENT_BYTES {
            self.exceeded = true;
            return Err(io::Error::other("tool arguments exceeded their byte limit"));
        }
        self.bytes = total;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn enforce_argument_limit(arguments: &Value) -> Result<(), ToolError> {
    let mut counter = BoundedJsonCounter {
        bytes: 0,
        exceeded: false,
    };
    let encoded = serde_json::to_writer(&mut counter, arguments);
    if counter.exceeded {
        return Err(ToolError::ArgumentLimit(MAX_TOOL_ARGUMENT_BYTES));
    }
    encoded?;
    Ok(())
}

/// Truncate a large tool result to a head + tail with an elision marker so a
/// single big output cannot bloat the model context (ADR-0002 §3.15). The head
/// keeps the start (errors, setup) and the tail keeps the end (results, flags).
fn truncate_tool_content(content: String) -> String {
    if content.len() <= MAX_TOOL_CONTEXT_BYTES {
        return content;
    }
    let head_budget = MAX_TOOL_CONTEXT_BYTES * 3 / 5;
    let tail_budget = MAX_TOOL_CONTEXT_BYTES - head_budget;
    let head_end = floor_char_boundary(&content, head_budget);
    let tail_start = ceil_char_boundary(&content, content.len().saturating_sub(tail_budget));
    if tail_start <= head_end {
        return content;
    }
    let omitted = tail_start - head_end;
    format!(
        "{}\n\n[... {omitted} bytes omitted; re-run with head/tail/grep or redirect to a file for the full output ...]\n\n{}",
        &content[..head_end],
        &content[tail_start..],
    )
}

fn floor_char_boundary(s: &str, mut index: usize) -> usize {
    index = index.min(s.len());
    while index > 0 && !s.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_char_boundary(s: &str, mut index: usize) -> usize {
    index = index.min(s.len());
    while index < s.len() && !s.is_char_boundary(index) {
        index += 1;
    }
    index
}

async fn read_bounded<R>(mut reader: R, max: usize) -> Result<Vec<u8>, ToolError>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(read) > max {
            return Err(ToolError::OutputLimit(max));
        }
        output.extend_from_slice(&buffer[..read]);
    }
}

async fn stop_child(child: &mut tokio::process::Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

fn definition(name: &str, description: &str, parameters: Value) -> ToolDefinition {
    ToolDefinition {
        name: name.to_owned(),
        description: description.to_owned(),
        parameters,
    }
}

#[cfg(unix)]
fn platform_shell(command: &str) -> Command {
    // bash explicitly (not `sh`, which is dash on the runtime image) — the base
    // image ships bash and the agent's tooling assumes bash features (ADR-0002 §3.16).
    let mut process = Command::new("bash");
    process.arg("-lc").arg(command);
    process
}

#[cfg(windows)]
fn platform_shell(command: &str) -> Command {
    let mut process = Command::new("powershell");
    process
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(command);
    process
}

fn normalized_relative(path: &str) -> Result<PathBuf, ToolError> {
    let path = Path::new(path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ToolError::OutsideWorkspace(path.display().to_string()));
    }
    Ok(path.to_path_buf())
}

fn resolve_existing(root: &Path, input: &str) -> Result<PathBuf, ToolError> {
    let candidate = root.join(normalized_relative(input)?);
    let canonical = candidate.canonicalize()?;
    if !canonical.starts_with(root) {
        return Err(ToolError::OutsideWorkspace(input.to_owned()));
    }
    Ok(canonical)
}

fn resolve_for_write(root: &Path, input: &str) -> Result<PathBuf, ToolError> {
    let relative = normalized_relative(input)?;
    let file_name = relative
        .file_name()
        .ok_or_else(|| ToolError::InvalidArguments("write path needs a file name".to_owned()))?;
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let parent_candidate = root.join(parent);
    std::fs::create_dir_all(&parent_candidate)?;
    let canonical_parent = parent_candidate.canonicalize()?;
    if !canonical_parent.starts_with(root) {
        return Err(ToolError::OutsideWorkspace(input.to_owned()));
    }
    let candidate = canonical_parent.join(file_name);
    if candidate.exists() && !candidate.canonicalize()?.starts_with(root) {
        return Err(ToolError::OutsideWorkspace(input.to_owned()));
    }
    Ok(candidate)
}

fn parse_message_kind(value: &str) -> Result<MessageKind, ToolError> {
    match value.to_ascii_lowercase().as_str() {
        "progress" => Ok(MessageKind::Progress),
        "insight" => Ok(MessageKind::Insight),
        "request" => Ok(MessageKind::Request),
        "final" => Ok(MessageKind::Final),
        _ => Err(ToolError::InvalidArguments(format!(
            "unknown message kind: {value}"
        ))),
    }
}

fn parse_insight(input: InsightInput) -> Result<Insight, ToolError> {
    let label = match input.label.to_ascii_uppercase().as_str() {
        "FACT" => InsightLabel::Fact,
        "HYPOTHESIS" => InsightLabel::Hypothesis,
        "DIRECTION" => InsightLabel::Direction,
        "SUCCESS" => InsightLabel::Success,
        "DEAD_END" => InsightLabel::DeadEnd,
        "BLOCKER" => InsightLabel::Blocker,
        _ => {
            return Err(ToolError::InvalidArguments(format!(
                "unknown insight label: {}",
                input.label
            )));
        }
    };
    Ok(Insight::new(InsightId::new(input.id)?, label, input.text))
}

fn snapshot_json(snapshot: &ma_coordinator::AgentSnapshot) -> Value {
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

#[derive(Deserialize)]
struct BashInput {
    command: String,
}

#[derive(Deserialize)]
struct WorkspaceInput {
    op: String,
    path: String,
    content: Option<String>,
}

#[derive(Deserialize)]
struct CreateInput {
    role: String,
    task: String,
}

#[derive(Deserialize)]
struct AssignInput {
    agent_id: String,
    role: String,
    task: String,
}

#[derive(Deserialize)]
struct SendInput {
    to: Vec<String>,
    kind: String,
    body: String,
    insight: Option<InsightInput>,
}

#[derive(Deserialize)]
struct InsightInput {
    id: String,
    label: String,
    text: String,
}

#[derive(Deserialize)]
struct WaitInput {
    timeout_ms: Option<u64>,
}

#[derive(Deserialize)]
struct InspectInput {
    agent_id: Option<String>,
}

#[derive(Deserialize)]
struct RecallInput {
    agent_id: String,
    reason: String,
}

#[derive(Deserialize)]
struct FinishInput {
    body: String,
}

#[derive(Deserialize)]
struct JournalInput {
    start: u64,
    end: u64,
}

#[derive(Deserialize)]
struct ReportInput {
    op: String,
    title: Option<String>,
    body: String,
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid tool arguments: {0}")]
    InvalidArguments(String),
    #[error("path is outside the workspace: {0}")]
    OutsideWorkspace(String),
    #[error("tool execution timed out after {0:?}")]
    TimedOut(Duration),
    #[error("tool output exceeded the {0}-byte stream limit")]
    OutputLimit(usize),
    #[error("tool arguments exceeded the {0}-byte input limit")]
    ArgumentLimit(usize),
    #[error("tool input is {actual} bytes; maximum direct read is {max}")]
    InputLimit { actual: u64, max: u64 },
    #[error("directory contains more than {0} entries")]
    EntryLimit(usize),
    #[error("tool process failed: {0}")]
    Process(String),
    #[error("tool execution was cancelled")]
    Cancelled,
    #[error("worker spawner failed: {0}")]
    Spawner(String),
    #[error(transparent)]
    Coordinator(#[from] CoordinatorError),
    #[error(transparent)]
    Domain(#[from] ma_core::domain::DomainError),
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
