use std::collections::{HashMap, VecDeque};
use std::io::{self, Stdout, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, Event, EventStream, KeyCode, KeyEvent,
    KeyEventKind, KeyModifiers, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;

use crate::coordinator::AgentSnapshot;
use crate::domain::{AgentId, AgentState, MAX_USER_INPUT_BYTES, MessageKind};
use crate::provider::{ModelDelta, ProviderSlot};
use crate::runtime::{RuntimeError, RuntimeEvent, TeamRuntime};
use crate::settings::ProviderSettingsStore;

mod command;
mod markdown;
mod model_setup;
pub mod theme;
mod view;

pub use command::{UiCommand, command_help, parse_command};
use model_setup::{
    ModelSetup, advance as advance_model_setup, begin as begin_model_setup,
    cancel as cancel_model_setup,
};
pub use view::render;
#[cfg(test)]
use view::{display_lines, input_projection, tone_style};

const MAX_INPUT_BYTES: usize = MAX_USER_INPUT_BYTES;
const MAX_TRANSCRIPT_LINES: usize = 1_000;
const MAX_TRANSCRIPT_DISPLAY_LINES: usize = 4_000;
const MAX_TRANSCRIPT_BYTES: usize = 2 * 1_024 * 1_024;
const MAX_DISPLAY_ENTRY_BYTES: usize = 128 * 1_024;
const MAX_DISPLAY_ENTRY_LINES: usize = 1_000;
const MAX_PARTIAL_BYTES: usize = 128 * 1_024;
const MAX_PENDING_SUBMISSIONS: usize = 8;
const MAX_RESUME_SESSIONS: usize = 8;
const ANIMATION_INTERVAL: Duration = Duration::from_millis(80);
const SUBMISSION_JOIN_DEADLINE: Duration = Duration::from_secs(2);
const DISPLAY_CLIPPED: &str = "\n[display clipped; durable journal intact]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineTone {
    /// Default agent / system line.
    Agent,
    /// A tool invocation header (action + dimmed target).
    Tool,
    /// A tool result line (success / failure).
    ToolResult { success: bool },
    /// A parent/child/sibling team message, carrying its kind label.
    Message,
    /// A provider or tool fault.
    Fault,
    /// The user's own input echo (speaker ❯ + text), rendered in the rare accent.
    User,
    /// The start banner line (`pentesting — ready · /help`), with the
    /// program name in the rare accent and the rest muted.
    Banner,
}

#[derive(Debug, Clone)]
struct TranscriptLine {
    speaker: String,
    text: String,
    tone: LineTone,
    /// Dimmed detail rendered after the speaker (tool target, message kind).
    subtitle: Option<String>,
}

impl TranscriptLine {
    fn new(speaker: String, text: String, tone: LineTone, subtitle: Option<String>) -> Self {
        Self {
            speaker,
            text,
            tone,
            subtitle,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Submission {
    User(String),
    Compact,
    Goal(Option<String>),
    Bash(String),
}

impl Submission {
    fn original_input(&self) -> String {
        match self {
            Self::User(text) => text.clone(),
            Self::Compact => "/compact".to_owned(),
            Self::Goal(Some(goal)) => format!("/goal {goal}"),
            Self::Goal(None) => "/goal".to_owned(),
            Self::Bash(command) => format!("!{command}"),
        }
    }
}

enum SubmissionEffect {
    None,
    Line { speaker: &'static str, text: String },
    Goal(Option<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlobalKeyAction {
    Interrupt,
    ClearInput,
}

fn global_key_action(key: KeyEvent) -> Option<GlobalKeyAction> {
    match key.code {
        KeyCode::Esc => Some(GlobalKeyAction::Interrupt),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(GlobalKeyAction::ClearInput)
        }
        _ => None,
    }
}

/// A scrollable overlay (e.g. `/status` shows main's battlefield). Closed with
/// Esc / x; scrolled with the arrow / PageUp-PageDown keys.
#[derive(Debug, Clone)]
pub(crate) struct Modal {
    pub(crate) title: String,
    pub(crate) lines: Vec<String>,
    pub(crate) scroll: usize,
}

pub struct TuiState {
    goal: String,
    transcript: VecDeque<TranscriptLine>,
    transcript_bytes: usize,
    transcript_lines: usize,
    partial: HashMap<AgentId, String>,
    active_turns: HashMap<AgentId, Instant>,
    /// Live team roster (main + non-terminal workers) for the roster bar.
    team: Vec<AgentSnapshot>,
    input: String,
    input_cursor: usize,
    status: String,
    auto: bool,
    pending_submissions: usize,
    model_setup: Option<ModelSetup>,
    /// Lines from the top of the wrapped transcript shown at the viewport top.
    /// Top-anchored so streaming/growth at the bottom never moves a scrolled-up
    /// view. `scroll_follow` pins the view to the newest content instead.
    scroll_offset: usize,
    scroll_follow: bool,
    scroll_max: usize,
    spinner_frame: usize,
    /// Short label of what main is currently doing (thinking / running <tool> /
    /// responding), shown in the status line instead of a bare "working".
    activity: String,
    /// Active overlay, if any (e.g. the `/status` battlefield view).
    modal: Option<Modal>,
    /// Highlighted row in the `/` command menu.
    command_selection: usize,
    /// Cumulative provider token usage this session, summed across every agent's
    /// requests, shown live in the status line.
    input_tokens: u64,
    output_tokens: u64,
    /// Tool calls main has made in the current turn — shown live so a long turn
    /// visibly advances step by step instead of looking stuck.
    turn_steps: usize,
}

impl TuiState {
    pub fn new(goal: impl Into<String>) -> Self {
        let mut state = Self {
            goal: goal.into(),
            transcript: VecDeque::new(),
            transcript_bytes: 0,
            transcript_lines: 0,
            partial: HashMap::new(),
            active_turns: HashMap::new(),
            team: Vec::new(),
            input: String::new(),
            input_cursor: 0,
            status: "ready · /help".to_owned(),
            auto: false,
            pending_submissions: 0,
            model_setup: None,
            scroll_offset: 0,
            scroll_follow: true,
            scroll_max: 0,
            spinner_frame: 0,
            activity: String::new(),
            modal: None,
            command_selection: 0,
            input_tokens: 0,
            output_tokens: 0,
            turn_steps: 0,
        };
        // Start banner: the program name reads in the rare accent, the rest
        // stays muted. It is the first transcript entry.
        state.push_entry(
            String::new(),
            "pentesting — ready · /help".to_owned(),
            LineTone::Banner,
            None,
        );
        state
    }

    fn open_modal(&mut self, title: impl Into<String>, content: &str) {
        self.modal = Some(Modal {
            title: title.into(),
            lines: content.lines().map(str::to_owned).collect(),
            scroll: 0,
        });
    }

    fn modal_open(&self) -> bool {
        self.modal.is_some()
    }

    fn close_modal(&mut self) {
        self.modal = None;
    }

    fn modal_scroll(&mut self, delta: isize) {
        if let Some(modal) = &mut self.modal {
            let max = modal.lines.len().saturating_sub(1);
            modal.scroll = modal.scroll.saturating_add_signed(delta).min(max);
        }
    }

    pub fn set_team(&mut self, team: Vec<AgentSnapshot>) {
        // Surface worker lifecycle (spawn / recall) in the transcript, since a
        // worker's own turns are otherwise hidden.
        let created = team
            .iter()
            .filter(|agent| {
                !agent.id.is_main() && !self.team.iter().any(|prev| prev.id == agent.id)
            })
            .map(|agent| (agent.id.clone(), agent.role.clone()))
            .collect::<Vec<_>>();
        let ended = self
            .team
            .iter()
            .filter(|prev| !prev.id.is_main() && !team.iter().any(|agent| agent.id == prev.id))
            .map(|prev| prev.id.clone())
            .collect::<Vec<_>>();
        self.partial
            .retain(|agent_id, _| team.iter().any(|agent| &agent.id == agent_id));
        self.team = team;
        for (id, role) in created {
            self.push_entry(
                "team".to_owned(),
                format!("spawned {id}  {role}"),
                LineTone::Message,
                None,
            );
        }
        for id in ended {
            self.push_entry(
                "team".to_owned(),
                format!("recalled {id}"),
                LineTone::Message,
                None,
            );
        }
    }

    pub fn set_input(&mut self, input: impl Into<String>) -> bool {
        let input = input.into();
        if input.len() > MAX_INPUT_BYTES {
            self.status = format!("input limit is {MAX_INPUT_BYTES} bytes; input was not replaced");
            return false;
        }
        self.input = input;
        self.input_cursor = self.input.len();
        true
    }

    fn try_push_input_char(&mut self, character: char) -> bool {
        if self.input.len().saturating_add(character.len_utf8()) > MAX_INPUT_BYTES {
            self.status = format!("input limit is {MAX_INPUT_BYTES} bytes");
            return false;
        }
        self.input.insert(self.input_cursor, character);
        self.input_cursor = self.input_cursor.saturating_add(character.len_utf8());
        true
    }

    fn try_append_paste(&mut self, pasted: &str) -> bool {
        let pasted = single_line_text(pasted);
        if self.input.len().saturating_add(pasted.len()) > MAX_INPUT_BYTES {
            self.status = format!("input limit is {MAX_INPUT_BYTES} bytes; paste was not inserted");
            return false;
        }
        self.input.insert_str(self.input_cursor, &pasted);
        self.input_cursor = self.input_cursor.saturating_add(pasted.len());
        true
    }

    fn move_input_left(&mut self) {
        self.input_cursor = self.input[..self.input_cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index);
    }

    fn move_input_right(&mut self) {
        if let Some(character) = self.input[self.input_cursor..].chars().next() {
            self.input_cursor = self.input_cursor.saturating_add(character.len_utf8());
        }
    }

    fn move_input_home(&mut self) {
        self.input_cursor = 0;
    }

    fn move_input_end(&mut self) {
        self.input_cursor = self.input.len();
    }

    fn backspace_input(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        let previous = self.input[..self.input_cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index);
        self.input.drain(previous..self.input_cursor);
        self.input_cursor = previous;
    }

    fn delete_input(&mut self) {
        let Some(character) = self.input[self.input_cursor..].chars().next() else {
            return;
        };
        let end = self.input_cursor.saturating_add(character.len_utf8());
        self.input.drain(self.input_cursor..end);
    }

    fn clear_input(&mut self) {
        self.input.clear();
        self.input_cursor = 0;
    }

    fn take_input(&mut self) -> String {
        self.input_cursor = 0;
        std::mem::take(&mut self.input)
    }

    pub fn apply_event(&mut self, event: RuntimeEvent) {
        match event {
            RuntimeEvent::TurnStarted { agent_id } => {
                self.partial.remove(&agent_id);
                if agent_id.is_main() {
                    // Honest state: before any token streams we are WAITING for the
                    // model, not "thinking" (UX, INTENT-0002 §3.18). It becomes
                    // "thinking" on the first reasoning token, "responding" on text.
                    self.activity = "waiting for model".to_owned();
                    self.turn_steps = 0;
                }
                self.active_turns.insert(agent_id, Instant::now());
            }
            RuntimeEvent::TurnFinished { agent_id, success } => {
                self.flush_live_output(&agent_id);
                self.active_turns.remove(&agent_id);
                if agent_id.is_main() {
                    self.activity.clear();
                }
                self.status = format!(
                    "{agent_id} turn {}",
                    if success { "complete" } else { "stopped" }
                );
            }
            // Only main's own step-by-step work (streaming, tool calls, response)
            // is shown; workers surface only through team messages and lifecycle,
            // to keep the transcript readable during a fan-out.
            RuntimeEvent::Delta {
                agent_id,
                delta: ModelDelta::Text(text),
            } => {
                if agent_id.is_main() {
                    self.activity = "responding".to_owned();
                    append_display(
                        self.partial.entry(agent_id).or_default(),
                        &text,
                        MAX_PARTIAL_BYTES,
                    );
                }
            }
            // Stream main's reasoning live so a slow model's "thinking" is visible
            // instead of a blank spinner. Display-only: reasoning never re-enters the
            // model context (the session records only the final response text).
            RuntimeEvent::Delta {
                agent_id,
                delta: ModelDelta::Reasoning(text),
            } => {
                if agent_id.is_main() {
                    self.activity = "thinking".to_owned();
                    append_display(
                        self.partial.entry(agent_id).or_default(),
                        &text,
                        MAX_PARTIAL_BYTES,
                    );
                }
            }
            RuntimeEvent::Delta {
                delta: ModelDelta::Usage(usage),
                ..
            } => {
                // Sum token usage across every agent's requests for a live session total.
                self.input_tokens = self.input_tokens.saturating_add(usage.input_tokens);
                self.output_tokens = self.output_tokens.saturating_add(usage.output_tokens);
            }
            RuntimeEvent::Delta { .. } => {}
            RuntimeEvent::ToolStarted {
                agent_id,
                name,
                summary,
            } => {
                if agent_id.is_main() {
                    // Commit any streamed response first so the tool call lands
                    // after it in chronological order, not pinned below it.
                    self.flush_live_output(&agent_id);
                    // Live feedback: show which tool is running and advance the step
                    // counter so a long turn visibly progresses (UX, INTENT-0002 §3.18).
                    self.activity = format!("running {name}");
                    self.turn_steps = self.turn_steps.saturating_add(1);
                    self.push_entry(name, String::new(), LineTone::Tool, summary);
                }
            }
            RuntimeEvent::ToolFinished {
                agent_id,
                name,
                success,
                output,
            } => {
                if agent_id.is_main() {
                    // After a tool returns we wait on the model for the next move.
                    self.activity = "waiting for model".to_owned();
                    let status = if success { "ok" } else { "failed" };
                    // Orchestration tools (team/journal) return bookkeeping JSON
                    // that the lifecycle and comms lines already convey; show only
                    // their status so the transcript stays about the actual work.
                    let bookkeeping = matches!(
                        name.as_str(),
                        crate::tools::names::TEAM | crate::tools::names::JOURNAL
                    );
                    let text = if output.is_empty() || bookkeeping {
                        status.to_owned()
                    } else {
                        format!("{status}\n{}", clip_tool_output(&output))
                    };
                    self.push_entry(String::new(), text, LineTone::ToolResult { success }, None);
                }
            }
            RuntimeEvent::Assistant { agent_id, text } => {
                if agent_id.is_main() {
                    let partial = self.partial.get(&agent_id).cloned().unwrap_or_default();
                    self.flush_live_output(&agent_id);
                    if !text.is_empty() && partial.is_empty() {
                        self.push_line("", text);
                    }
                }
            }
            RuntimeEvent::AgentMessage {
                sender,
                recipients,
                kind,
                body,
            } => {
                self.flush_live_output(&sender);
                let recipients = recipients
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                self.push_entry(
                    format!("{sender} → {recipients}"),
                    body,
                    LineTone::Message,
                    Some(message_kind_label(kind).to_owned()),
                );
            }
            RuntimeEvent::TeamChanged => self.status = "team changed".to_owned(),
            RuntimeEvent::Fault { agent_id, message } => {
                self.flush_live_output(&agent_id);
                self.push_entry(agent_id.to_string(), message, LineTone::Fault, None);
                self.status = "provider/tool fault".to_owned();
            }
        }
    }

    fn push_line(&mut self, speaker: impl Into<String>, text: impl Into<String>) {
        self.push_entry(speaker.into(), text.into(), LineTone::Agent, None);
    }

    fn push_entry(
        &mut self,
        speaker: String,
        text: String,
        tone: LineTone,
        subtitle: Option<String>,
    ) {
        let text = clip_display(text, MAX_DISPLAY_ENTRY_BYTES);
        let subtitle = subtitle.map(|subtitle| clip_display(subtitle, MAX_DISPLAY_ENTRY_BYTES));
        let subtitle_bytes = subtitle.as_ref().map_or(0, String::len);
        let line_count = text.lines().count().max(1);
        self.transcript_bytes = self.transcript_bytes.saturating_add(
            speaker
                .len()
                .saturating_add(text.len())
                .saturating_add(subtitle_bytes),
        );
        self.transcript_lines = self.transcript_lines.saturating_add(line_count);
        self.transcript
            .push_back(TranscriptLine::new(speaker, text, tone, subtitle));
        while self.transcript.len() > MAX_TRANSCRIPT_LINES
            || self.transcript_bytes > MAX_TRANSCRIPT_BYTES
            || self.transcript_lines > MAX_TRANSCRIPT_DISPLAY_LINES
        {
            if let Some(removed) = self.transcript.pop_front() {
                let removed_bytes = removed
                    .speaker
                    .len()
                    .saturating_add(removed.text.len())
                    .saturating_add(removed.subtitle.as_ref().map_or(0, String::len));
                self.transcript_bytes = self.transcript_bytes.saturating_sub(removed_bytes);
                self.transcript_lines = self
                    .transcript_lines
                    .saturating_sub(removed.text.lines().count().max(1));
            } else {
                self.transcript_bytes = 0;
                self.transcript_lines = 0;
                break;
            }
        }
    }

    fn flush_live_output(&mut self, agent_id: &AgentId) {
        if let Some(response) = self.partial.remove(agent_id)
            && !response.is_empty()
        {
            self.push_line("", response);
        }
    }

    fn set_auto(&mut self, enabled: bool) {
        self.auto = enabled;
        self.status = format!("auto {}", if enabled { "on" } else { "off" });
    }

    /// Positive `delta` scrolls up (toward older content); negative scrolls down.
    fn scroll_transcript(&mut self, delta: isize) {
        // Anchor from the current resolved offset so the first scroll away from
        // the bottom starts where the viewport actually is.
        let base = if self.scroll_follow {
            self.scroll_max
        } else {
            self.scroll_offset.min(self.scroll_max)
        };
        let offset = if delta >= 0 {
            base.saturating_sub(delta as usize)
        } else {
            base.saturating_add(delta.unsigned_abs())
        };
        if offset >= self.scroll_max {
            // Reached the bottom: resume following the newest content.
            self.scroll_follow = true;
            self.scroll_offset = self.scroll_max;
        } else {
            self.scroll_follow = false;
            self.scroll_offset = offset;
        }
    }

    /// Pin the view to the newest content (auto-follow).
    fn scroll_to_bottom(&mut self) {
        self.scroll_follow = true;
    }

    fn advance_spinner(&mut self) {
        self.spinner_frame = self.spinner_frame.wrapping_add(1);
    }
}

fn agent_state_label(state: AgentState) -> &'static str {
    match state {
        AgentState::Running => "running",
        AgentState::Waiting => "waiting",
        AgentState::Recalling => "recalling",
        AgentState::Finished => "finished",
        AgentState::Stopped => "stopped",
        AgentState::Faulted => "faulted",
    }
}

/// Extract main's `## Battlefield` section from its brief for the status modal,
/// falling back to the whole brief when that section is absent.
fn battlefield_section(brief: &str) -> String {
    let mut section = String::new();
    let mut in_section = false;
    for line in brief.lines() {
        let is_header = line.trim_start().starts_with("## ");
        if in_section {
            if is_header {
                break;
            }
            section.push_str(line);
            section.push('\n');
        } else if is_header && line.to_ascii_lowercase().contains("battlefield") {
            in_section = true;
            section.push_str(line);
            section.push('\n');
        }
    }
    if section.trim().is_empty() {
        brief.trim_end().to_owned()
    } else {
        section.trim_end().to_owned()
    }
}

/// Keep a tool result readable in the transcript: at most a handful of lines,
/// each bounded in width, with a short note when more was produced. The full
/// output is preserved durably in the journal regardless.
fn clip_tool_output(output: &str) -> String {
    const MAX_LINES: usize = 12;
    const MAX_LINE_CHARS: usize = 200;
    let all = output.trim_end_matches('\n').lines().collect::<Vec<_>>();
    let mut shown = all
        .iter()
        .take(MAX_LINES)
        .map(|line| {
            if line.chars().count() > MAX_LINE_CHARS {
                let head = line.chars().take(MAX_LINE_CHARS).collect::<String>();
                format!("{head} [...]")
            } else {
                (*line).to_owned()
            }
        })
        .collect::<Vec<_>>();
    if all.len() > MAX_LINES {
        shown.push(format!(
            "[... +{} more lines; full output in journal ...]",
            all.len() - MAX_LINES
        ));
    }
    shown.join("\n")
}

fn single_line_text(text: &str) -> String {
    let mut flattened = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                flattened.push(' ');
            }
            '\n' => flattened.push(' '),
            character => flattened.push(character),
        }
    }
    flattened
}

pub(crate) fn message_kind_label(kind: MessageKind) -> &'static str {
    match kind {
        MessageKind::Progress => "Progress",
        MessageKind::Insight => "Insight",
        MessageKind::Request => "Request",
        MessageKind::Final => "Final",
    }
}

fn submission_channel() -> (mpsc::Sender<Submission>, mpsc::Receiver<Submission>) {
    mpsc::channel(MAX_PENDING_SUBMISSIONS)
}

fn try_enqueue_submission(
    sender: &mpsc::Sender<Submission>,
    submission: Submission,
) -> Result<(), Submission> {
    sender
        .try_send(submission)
        .map_err(|error| error.into_inner())
}

fn append_display(buffer: &mut String, text: &str, max_bytes: usize) {
    if text.is_empty() || buffer.ends_with(DISPLAY_CLIPPED) {
        return;
    }
    if buffer.len().saturating_add(text.len()) <= max_bytes {
        buffer.push_str(text);
        return;
    }
    let content_limit = max_bytes.saturating_sub(DISPLAY_CLIPPED.len());
    truncate_utf8(buffer, content_limit);
    if buffer.len() < content_limit {
        let remaining = content_limit - buffer.len();
        buffer.push_str(utf8_prefix(text, remaining));
    }
    buffer.push_str(DISPLAY_CLIPPED);
}

fn clip_display(mut text: String, max_bytes: usize) -> String {
    if text.len() > max_bytes {
        truncate_utf8(&mut text, max_bytes.saturating_sub(DISPLAY_CLIPPED.len()));
        text.push_str(DISPLAY_CLIPPED);
    }
    if text.lines().count() <= MAX_DISPLAY_ENTRY_LINES {
        return text;
    }
    let mut clipped = text
        .lines()
        .take(MAX_DISPLAY_ENTRY_LINES.saturating_sub(1))
        .collect::<Vec<_>>()
        .join("\n");
    clipped.push_str(DISPLAY_CLIPPED);
    clipped
}

fn truncate_utf8(text: &mut String, max_bytes: usize) {
    text.truncate(utf8_boundary(text, max_bytes));
}

fn utf8_prefix(text: &str, max_bytes: usize) -> &str {
    &text[..utf8_boundary(text, max_bytes)]
}

fn utf8_boundary(text: &str, max_bytes: usize) -> usize {
    let mut boundary = max_bytes.min(text.len());
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

pub async fn run_tui(
    runtime: TeamRuntime,
    goal: impl Into<String>,
    provider: Arc<ProviderSlot>,
    settings: ProviderSettingsStore,
) -> anyhow::Result<()> {
    let mut terminal = TerminalSession::open()?;
    let mut state = TuiState::new(goal);
    state.auto = runtime.auto_enabled();
    state.set_team(runtime.coordinator().live_team()?);
    let mut events = runtime.subscribe();
    let mut input_events = EventStream::new();
    let mut animation = tokio::time::interval(ANIMATION_INTERVAL);
    animation.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let (submission_tx, mut submission_rx) = submission_channel();
    let (result_tx, mut result_rx) = mpsc::channel(MAX_PENDING_SUBMISSIONS + 1);
    let submission_runtime = runtime.clone();
    let mut submission_driver = tokio::spawn(async move {
        while let Some(submission) = submission_rx.recv().await {
            let result: Result<SubmissionEffect, RuntimeError> = match submission {
                Submission::User(input) => submission_runtime
                    .submit_user(input)
                    .await
                    .map(|_| SubmissionEffect::None),
                Submission::Compact => submission_runtime.compact_main().await.map(|compacted| {
                    SubmissionEffect::Line {
                        speaker: "compact",
                        text: if compacted {
                            "main context compacted and coverage verified".to_owned()
                        } else {
                            "main context is below the semantic compaction threshold".to_owned()
                        },
                    }
                }),
                Submission::Goal(objective) => {
                    let effect = objective.clone();
                    submission_runtime
                        .set_goal(objective)
                        .await
                        .map(|_| SubmissionEffect::Goal(effect))
                }
                Submission::Bash(command) => {
                    submission_runtime
                        .run_bash(command)
                        .await
                        .map(|text| SubmissionEffect::Line {
                            speaker: "shell",
                            text,
                        })
                }
            };
            if result_tx.send(result).await.is_err() {
                break;
            }
        }
    });

    loop {
        terminal.draw(&mut state)?;
        tokio::select! {
            input = input_events.next() => {
                match input {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        if state.modal_open() {
                            match key.code {
                                KeyCode::Esc
                                | KeyCode::Char('x')
                                | KeyCode::Char('q') => state.close_modal(),
                                KeyCode::Up => state.modal_scroll(-1),
                                KeyCode::Down => state.modal_scroll(1),
                                KeyCode::PageUp => state.modal_scroll(-10),
                                KeyCode::PageDown => state.modal_scroll(10),
                                _ => {}
                            }
                            continue;
                        }
                        if let Some(action) = global_key_action(key) {
                            if cancel_model_setup(&mut state) {
                                continue;
                            }
                            match action {
                                GlobalKeyAction::Interrupt => {
                                    let interrupted_main = runtime.interrupt_main();
                                    runtime.set_auto(false);
                                    state.auto = false;
                                    state.status = if interrupted_main {
                                        "interrupted; auto off".to_owned()
                                    } else {
                                        "auto off".to_owned()
                                    };
                                }
                                GlobalKeyAction::ClearInput => {
                                    state.clear_input();
                                    state.status = "input cleared".to_owned();
                                }
                            }
                            continue;
                        }
                        // The `/` command menu takes the arrow/Tab/Enter keys while open.
                        let menu = if state.model_setup.is_none() {
                            command::command_menu_matches(&state.input)
                        } else {
                            Vec::new()
                        };
                        if !menu.is_empty() {
                            let selected = state.command_selection.min(menu.len() - 1);
                            match key.code {
                                KeyCode::Up => {
                                    state.command_selection = selected.saturating_sub(1);
                                    continue;
                                }
                                KeyCode::Down => {
                                    state.command_selection = (selected + 1).min(menu.len() - 1);
                                    continue;
                                }
                                KeyCode::Tab => {
                                    state.set_input(format!("{} ", menu[selected].0));
                                    state.command_selection = 0;
                                    continue;
                                }
                                KeyCode::Enter => {
                                    state.set_input(menu[selected].0);
                                    state.command_selection = 0;
                                }
                                _ => {}
                            }
                        }
                        match key.code {
                            KeyCode::Up => { state.scroll_transcript(1); }
                            KeyCode::Down => { state.scroll_transcript(-1); }
                            KeyCode::PageUp => { state.scroll_transcript(10); }
                            KeyCode::PageDown => { state.scroll_transcript(-10); }
                            KeyCode::Home if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                state.scroll_transcript(isize::MAX);
                            }
                            KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                state.scroll_to_bottom();
                            }
                            KeyCode::Left => state.move_input_left(),
                            KeyCode::Right => state.move_input_right(),
                            KeyCode::Home => state.move_input_home(),
                            KeyCode::End => state.move_input_end(),
                            KeyCode::Delete => state.delete_input(),
                            KeyCode::Char(character) => {
                                state.try_push_input_char(character);
                                state.command_selection = 0;
                            }
                            KeyCode::Backspace => {
                                state.backspace_input();
                                state.command_selection = 0;
                            }
                            KeyCode::Enter => {
                                let input = state.take_input();
                                if input.trim().is_empty() && state.model_setup.is_none() {
                                    continue;
                                }
                                if state.model_setup.is_some() {
                                    advance_model_setup(
                                        &mut state,
                                        input,
                                        &provider,
                                        &settings,
                                    ).await;
                                    continue;
                                }
                                match parse_command(&input) {
                                    Ok(Some(command)) => {
                                        if handle_command(
                                            &runtime,
                                            &mut state,
                                            &submission_tx,
                                            command,
                                        ).await? {
                                            break;
                                        }
                                    }
                                    Ok(None) => {
                                        let displayed = input.clone();
                                        // While main is working, fold the input into the
                                        // running turn instead of queuing a fresh one, so
                                        // the agent keeps its progress and re-prioritizes.
                                        if state.active_turns.contains_key(&AgentId::main())
                                            && matches!(runtime.steer_main(input.clone()), Ok(true))
                                        {
                                            state.push_entry(
                                                "❯".to_owned(),
                                                displayed,
                                                LineTone::User,
                                                Some("steering".to_owned()),
                                            );
                                            // Jump back to the newest content so the
                                            // user sees their own input and the reply.
                                            state.scroll_to_bottom();
                                            continue;
                                        }
                                        match try_enqueue_submission(
                                            &submission_tx,
                                            Submission::User(input),
                                        ) {
                                            Ok(()) => {
                                                state.pending_submissions = state
                                                    .pending_submissions
                                                    .saturating_add(1);
                                                state.push_entry(
                                                    "❯".to_owned(),
                                                    displayed,
                                                    LineTone::User,
                                                    None,
                                                );
                                                // Follow the newest content so the user
                                                // sees their input and the response.
                                                state.scroll_to_bottom();
                                            }
                                            Err(submission) => {
                                                state.set_input(submission.original_input());
                                                state.status = format!(
                                                    "submission queue is full ({MAX_PENDING_SUBMISSIONS}); input preserved"
                                                );
                                            }
                                        }
                                    }
                                    Err(error) => state.push_line("command", error),
                                }
                            }
                            _ => {}
                        }
                    }
                    Some(Ok(Event::Paste(pasted))) => {
                        state.try_append_paste(&pasted);
                    }
                    Some(Ok(Event::Mouse(mouse))) => match mouse.kind {
                        MouseEventKind::ScrollUp => state.scroll_transcript(3),
                        MouseEventKind::ScrollDown => state.scroll_transcript(-3),
                        _ => {}
                    },
                    Some(Err(error)) => return Err(error).context("terminal input failed"),
                    None => break,
                    _ => {}
                }
            }
            event = events.recv() => match event {
                Ok(event) => {
                    let team_changed = matches!(event, RuntimeEvent::TeamChanged);
                    state.apply_event(event);
                    if team_changed {
                        state.set_team(runtime.coordinator().live_team()?);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    state.status = format!("display skipped {skipped} events; durable journal is intact");
                    state.set_team(runtime.coordinator().live_team()?);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
            Some(result) = result_rx.recv() => {
                state.pending_submissions = state.pending_submissions.saturating_sub(1);
                match result {
                    Ok(SubmissionEffect::None) => {}
                    Ok(SubmissionEffect::Line { speaker, text }) => {
                        state.push_line(speaker, text);
                    }
                    Ok(SubmissionEffect::Goal(objective)) => {
                        state.goal = objective.clone().unwrap_or_default();
                        // Setting a goal starts autonomous work; clearing it stops.
                        let enabled = objective.is_some();
                        runtime.set_auto(enabled);
                        state.auto = enabled;
                        state.push_line(
                            "goal",
                            objective.map_or_else(
                                || "goal cleared; autonomous loop stopped".to_owned(),
                                |goal| format!("goal set: {goal}; autonomous loop started"),
                            ),
                        );
                    }
                    Err(error) => {
                        // Provider and tool faults are already surfaced through the
                        // `RuntimeEvent::Fault` broadcast channel (rendered as an agent
                        // line), so re-printing the same message here would duplicate it.
                        // Keep the status line informative instead.
                        if !matches!(error, RuntimeError::Provider(_) | RuntimeError::Tool(_)) {
                            state.push_line("runtime", error.to_string());
                        }
                        state.status = "last action failed; see the transcript".to_owned();
                    }
                }
                state.set_team(runtime.coordinator().live_team()?);
            }
            // Cosmetic only: advance the spinner while an agent has a live turn.
            // Gated on `active_turns` so that when idle the branch is disabled and
            // the loop blocks on real events instead of waking every 80ms. The team
            // roster is refreshed from `TeamChanged` events (see `events.recv()`),
            // not polled here — polling it per frame was the redundant repaint.
            _ = animation.tick(), if !state.active_turns.is_empty() => {
                state.advance_spinner();
            }
        }
    }
    drop(submission_tx);
    runtime.shutdown().await;
    if tokio::time::timeout(SUBMISSION_JOIN_DEADLINE, &mut submission_driver)
        .await
        .is_err()
    {
        submission_driver.abort();
        let _ = submission_driver.await;
    }
    Ok(())
}

async fn handle_command(
    runtime: &TeamRuntime,
    state: &mut TuiState,
    submissions: &mpsc::Sender<Submission>,
    command: UiCommand,
) -> anyhow::Result<bool> {
    match command {
        UiCommand::Exit => return Ok(true),
        UiCommand::Auto => {
            let enabled = !state.auto;
            runtime.set_auto(enabled);
            state.set_auto(enabled);
        }
        UiCommand::Status => {
            let team = runtime.coordinator().live_team()?;
            let active = team
                .iter()
                .filter(|agent| !agent.state.is_terminal())
                .count();
            let unread = team.iter().map(|agent| agent.unread).sum::<usize>();
            let brief = runtime.brief(&AgentId::main()).unwrap_or_default();
            let mut body = String::new();
            if !state.goal.trim().is_empty() {
                body.push_str(&format!("goal: {}\n\n", state.goal.trim()));
            }
            body.push_str(&format!(
                "agents {} · active {active} · unread {unread}\n\n",
                team.len()
            ));
            // Live team — the real progress (roles, states, latest insights).
            for agent in &team {
                body.push_str(&format!(
                    "{} — {}\n",
                    agent.id,
                    agent_state_label(agent.state)
                ));
                if !agent.id.is_main() && !agent.task.trim().is_empty() {
                    body.push_str(&format!("  task: {}\n", agent.task.trim()));
                }
                if let Some(insight) = &agent.latest_insight {
                    body.push_str(&format!("  insight: {}\n", insight.trim()));
                }
                if let Some(waiting) = &agent.waiting_on {
                    body.push_str(&format!("  waiting: {}\n", waiting.trim()));
                }
                body.push('\n');
            }
            // Main's battlefield note, maintained live via the `brief` tool
            // (falls back to the compaction-curated brief until the first note).
            body.push_str("── battlefield ──\n");
            body.push_str(&battlefield_section(&brief));
            state.open_modal("Status", &body);
            state.set_team(team);
        }
        UiCommand::Agent => {
            let team = runtime.coordinator().live_team()?;
            let agents = team
                .iter()
                .map(|agent| format!("{} · {:?} · {}", agent.id, agent.state, agent.task))
                .collect::<Vec<_>>()
                .join("\n");
            state.push_line("agents", agents);
            state.set_team(team);
        }
        UiCommand::AgentSwitch(agent_id) => {
            let agent = runtime.coordinator().inspect(&agent_id)?;
            let brief = runtime.brief(&agent_id)?;
            state.push_line(
                "agent",
                format!(
                    "{} · {:?} · {}\n{}",
                    agent.id, agent.state, agent.task, brief
                ),
            );
        }
        UiCommand::Help => state.push_line("help", command_help()),
        UiCommand::Compact => enqueue_background(state, submissions, Submission::Compact),
        UiCommand::New => state.push_line(
            "new",
            "start a new durable run by exiting and launching `pentesting run --goal ...`",
        ),
        UiCommand::Goal(goal) => enqueue_background(state, submissions, Submission::Goal(goal)),
        UiCommand::Resume => {
            let current_root = runtime.coordinator().journal().root().to_path_buf();
            let body = build_resume_modal(&current_root);
            state.open_modal("Resume Saved Sessions", &body);
        }
        UiCommand::Model(query) => begin_model_setup(state, query),
        UiCommand::Update => state.push_line("update", "npm install -g pentesting@latest"),
        UiCommand::Bash(command) => {
            enqueue_background(state, submissions, Submission::Bash(command))
        }
        UiCommand::Target(scope) => match runtime.set_engagement_scope(scope.clone()) {
            Ok(()) => state.push_line(
                "target",
                match scope {
                    Some(scope) => format!("authorized target set: {scope}"),
                    None => "authorized target cleared".to_owned(),
                },
            ),
            Err(error) => state.push_line("target", format!("target error: {error}")),
        },
    }
    Ok(false)
}

/// Build the `/resume` modal body: the saved sessions under this workspace
/// (newest first, each with its goal) and the resume command reminder.
fn build_resume_modal(current_root: &std::path::Path) -> String {
    let mut entries: Vec<std::path::PathBuf> = Vec::new();
    if let Some(parent) = current_root.parent()
        && let Ok(read_dir) = std::fs::read_dir(parent)
    {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir()
                && (path.join("journal").exists() || path.join("journal.jsonl").exists())
            {
                entries.push(path);
            }
        }
    }

    entries.sort_by(|a, b| {
        let time_a = std::fs::metadata(a).and_then(|m| m.modified()).ok();
        let time_b = std::fs::metadata(b).and_then(|m| m.modified()).ok();
        time_b.cmp(&time_a)
    });

    let mut body = String::new();
    body.push_str("Saved Sessions in Workspace:\n\n");

    if entries.is_empty() {
        body.push_str("  No prior sessions found in this workspace.\n\n");
    } else {
        for path in entries.iter().take(MAX_RESUME_SESSIONS) {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let is_current = path == current_root;
            let marker = if is_current { "  ◀ CURRENT" } else { "" };
            body.push_str(&format!("• {name}{marker}\n"));
            body.push_str(&format!("  Path: {}\n", path.display()));

            if let Ok(journal) =
                crate::journal::RunJournal::open(path, crate::journal::JournalConfig::default())
            {
                let journal_ref = std::sync::Arc::new(journal);
                if let Ok(coordinator) = crate::coordinator::AgentCoordinator::recover(journal_ref)
                    && let Ok(main_snapshot) = coordinator.inspect(&crate::domain::AgentId::main())
                {
                    let task = main_snapshot.task.trim();
                    if !task.is_empty() {
                        body.push_str(&format!("  Goal: {task}\n"));
                    }
                }
            }
            body.push('\n');
        }
    }

    body.push_str("──────────────────────────────────────────────────\n");
    body.push_str("To resume a session, exit (/exit) and launch with:\n");
    body.push_str("  pentesting run --resume <path>\n\n");
    body.push_str("Current session resume command:\n");
    body.push_str(&format!(
        "  pentesting run --resume {}\n\n",
        current_root.display()
    ));
    body.push_str("Press Esc or x to close this modal.");
    body
}

fn enqueue_background(
    state: &mut TuiState,
    submissions: &mpsc::Sender<Submission>,
    submission: Submission,
) {
    match try_enqueue_submission(submissions, submission) {
        Ok(()) => {
            state.pending_submissions = state.pending_submissions.saturating_add(1);
            state.status = "command queued".to_owned();
        }
        Err(submission) => {
            state.set_input(submission.original_input());
            state.status =
                format!("submission queue is full ({MAX_PENDING_SUBMISSIONS}); command restored");
        }
    }
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn open() -> anyhow::Result<Self> {
        enable_raw_mode().context("enable terminal raw mode")?;
        let mut stdout = io::stdout();
        if let Err(error) = write_terminal_setup(&mut stdout) {
            let _ = disable_raw_mode();
            let _ = write_terminal_teardown(&mut io::stdout());
            return Err(error).context("configure terminal input");
        }
        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let _ = disable_raw_mode();
                let _ = write_terminal_teardown(&mut io::stdout());
                Err(error).context("initialize terminal")
            }
        }
    }

    fn draw(&mut self, state: &mut TuiState) -> io::Result<()> {
        self.terminal.draw(|frame| render(frame, state)).map(|_| ())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = write_terminal_teardown(self.terminal.backend_mut());
        let _ = self.terminal.show_cursor();
    }
}

fn write_terminal_setup(output: &mut impl Write) -> io::Result<()> {
    // No mouse capture: the terminal keeps native click-drag selection and copy.
    // Scroll the transcript with the arrow keys / PageUp-PageDown instead (many
    // terminals also translate the wheel to those in the alternate screen).
    execute!(output, EnterAlternateScreen, EnableBracketedPaste)
}

fn write_terminal_teardown(output: &mut impl Write) -> io::Result<()> {
    execute!(output, DisableBracketedPaste, LeaveAlternateScreen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_buffer(state: &mut TuiState, width: u16, height: u16) -> String {
        let backend = ratatui::backend::TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, state)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn scroll_to_top_reaches_oldest_entry_past_a_huge_entry() {
        let mut state = TuiState::new("goal");
        state.push_line("old-marker", "topmost line");
        for index in 0..20 {
            state.push_line("main", format!("filler-{index}"));
        }
        // A large multi-line entry like a big tool output.
        state.push_line(
            "tool",
            (0..400).map(|i| format!("line-{i}\n")).collect::<String>(),
        );

        let _ = render_buffer(&mut state, 40, 8);
        state.scroll_transcript(isize::MAX);
        let top = render_buffer(&mut state, 40, 8);
        assert!(top.contains("old-marker"), "top after scroll: {top:?}");
        assert!(top.contains("topmost line"));
    }

    #[test]
    fn model_setup_masks_api_keys_and_never_adds_them_to_transcript() {
        let mut state = TuiState::new("goal");
        state.model_setup = Some(ModelSetup::ApiKey { query: None });
        state.input = "top-secret".to_owned();

        assert_eq!(input_projection(&state), "**********");
        assert!(
            state
                .transcript
                .iter()
                .all(|line| !line.text.contains("top-secret"))
        );
    }

    #[test]
    fn model_setup_starts_with_api_key_without_a_provider_menu() {
        let mut state = TuiState::new("goal");
        begin_model_setup(&mut state, None);

        assert!(matches!(state.model_setup, Some(ModelSetup::ApiKey { .. })));

        let backend = ratatui::backend::TestBackend::new(100, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("API key - hidden; type, then Enter"));
        assert!(!rendered.contains("openrouter"));
    }

    #[test]
    fn model_setup_can_be_cancelled_without_exiting_the_tui() {
        let mut state = TuiState::new("goal");
        begin_model_setup(&mut state, None);
        state.input = "partially-entered-secret".to_owned();

        assert!(cancel_model_setup(&mut state));
        assert!(state.model_setup.is_none());
        assert!(state.input.is_empty());
        assert_eq!(state.status, "model setup cancelled");
        assert!(!cancel_model_setup(&mut state));
    }

    #[test]
    fn transcript_scroll_reaches_older_messages() {
        let mut state = TuiState::new("goal");
        for index in 0..10 {
            state.push_line("main", format!("message-{index}"));
        }

        let latest = render_buffer(&mut state, 80, 11);
        assert!(latest.contains("message-9"));
        assert!(!latest.contains("message-0"));

        state.scroll_transcript(isize::MAX);
        let older = render_buffer(&mut state, 80, 11);
        assert!(older.contains("message-0"));
        assert!(!older.contains("message-9"));
    }

    #[test]
    fn transcript_scroll_reaches_the_top_of_wrapped_messages() {
        let mut state = TuiState::new("goal");
        state.push_line("old", "old-word ".repeat(80));
        state.push_line("new", "latest-message");
        let _ = render_buffer(&mut state, 40, 11);

        state.scroll_transcript(isize::MAX);
        let oldest = render_buffer(&mut state, 40, 11);

        assert!(oldest.contains("old"));
        assert!(!oldest.contains("latest-message"));
    }

    #[test]
    fn scrolled_up_viewport_stays_put_when_content_grows_at_the_bottom() {
        let mut state = TuiState::new("goal");
        state.push_line("anchor", "keep-me-visible");
        for index in 0..30 {
            state.push_line("main", format!("message-{index}"));
        }
        // Scroll all the way up so the oldest line is at the viewport top.
        let _ = render_buffer(&mut state, 24, 11);
        state.scroll_transcript(isize::MAX);
        let before = render_buffer(&mut state, 24, 11);
        assert!(before.contains("keep-me-visible"));
        let offset_before = state.scroll_offset;

        // Streaming/growth at the bottom must NOT move a scrolled-up viewport.
        for index in 30..40 {
            state.push_line("main", format!("message-{index}"));
        }
        let after = render_buffer(&mut state, 24, 11);

        assert!(!state.scroll_follow);
        assert_eq!(state.scroll_offset, offset_before);
        assert!(after.contains("keep-me-visible"));
    }

    #[test]
    fn scrolling_back_to_the_bottom_resumes_following_new_content() {
        let mut state = TuiState::new("goal");
        for index in 0..30 {
            state.push_line("main", format!("message-{index}"));
        }
        let _ = render_buffer(&mut state, 24, 11);
        state.scroll_transcript(5);
        assert!(!state.scroll_follow);
        state.scroll_transcript(-isize::MAX);
        assert!(state.scroll_follow);

        state.push_line("main", "newest-line");
        let rendered = render_buffer(&mut state, 24, 11);
        assert!(rendered.contains("newest-line"));
    }

    #[test]
    fn main_reasoning_is_streamed_live_in_the_transcript() {
        let mut state = TuiState::new("goal");
        state.apply_event(RuntimeEvent::TurnStarted {
            agent_id: AgentId::main(),
        });
        state.apply_event(RuntimeEvent::Delta {
            agent_id: AgentId::main(),
            delta: ModelDelta::Reasoning("checking the request".to_owned()),
        });
        state.apply_event(RuntimeEvent::Delta {
            agent_id: AgentId::main(),
            delta: ModelDelta::Text("the answer".to_owned()),
        });

        let rendered = display_lines(&state)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<String>();
        // Main's reasoning ("thinking") is streamed live so a slow model's progress
        // is visible instead of a blank spinner, and the final response follows it.
        assert!(rendered.contains("checking the request"));
        assert!(rendered.contains("the answer"));
    }

    #[test]
    fn wrapped_transcript_keeps_the_newest_multiline_entry_visible() {
        let mut state = TuiState::new("goal");
        state.push_line("old", "word ".repeat(40));
        state.push_line(
            "config",
            "provider=openai-compatible\nbase_url=https://example.test/v1\nmodel=qa-model\napi_key=configured",
        );

        let rendered = render_buffer(&mut state, 80, 12);

        assert!(rendered.contains("api_key=configured"));
    }

    #[test]
    fn input_and_display_projections_are_bounded_without_touching_the_journal() {
        let mut state = TuiState::new("goal");
        for _ in 0..MAX_INPUT_BYTES {
            assert!(state.try_push_input_char('a'));
        }
        assert!(!state.try_push_input_char('b'));
        assert_eq!(state.input.len(), MAX_INPUT_BYTES);

        state.set_input("prefix");
        assert!(!state.try_append_paste(&"한".repeat(MAX_INPUT_BYTES)));
        assert_eq!(state.input, "prefix");
        assert!(state.try_append_paste("-pasted"));
        assert_eq!(state.input, "prefix-pasted");

        state.apply_event(RuntimeEvent::Delta {
            agent_id: AgentId::main(),
            delta: ModelDelta::Text("x".repeat(MAX_PARTIAL_BYTES + 1)),
        });
        assert!(state.partial[&AgentId::main()].len() <= MAX_PARTIAL_BYTES);
        state.set_team(Vec::new());
        assert!(state.partial.is_empty());

        for index in 0..(MAX_TRANSCRIPT_BYTES / MAX_DISPLAY_ENTRY_BYTES + 2) {
            state.push_line(
                format!("agent-{index}"),
                "z".repeat(MAX_DISPLAY_ENTRY_BYTES + 1),
            );
        }
        assert!(state.transcript_bytes <= MAX_TRANSCRIPT_BYTES);
        assert!(
            state
                .transcript
                .iter()
                .all(|line| line.text.len() <= MAX_DISPLAY_ENTRY_BYTES)
        );

        for index in 0..=MAX_TRANSCRIPT_LINES {
            state.push_line("small", index.to_string());
        }
        assert!(state.transcript.len() <= MAX_TRANSCRIPT_LINES);

        state.push_line("many-lines", "line\n".repeat(50_000));
        assert!(display_lines(&state).len() <= MAX_TRANSCRIPT_DISPLAY_LINES);
    }

    #[test]
    fn input_editor_moves_and_edits_on_utf8_boundaries() {
        let mut state = TuiState::new("goal");
        assert!(state.set_input("a한b"));

        state.move_input_left();
        state.backspace_input();
        assert_eq!(state.input, "ab");

        state.move_input_home();
        state.delete_input();
        assert_eq!(state.input, "b");

        state.move_input_end();
        assert!(state.try_push_input_char('!'));
        state.move_input_left();
        assert!(state.try_append_paste("한글"));
        assert_eq!(state.input, "b한글!");

        state.move_input_right();
        state.move_input_end();
        assert_eq!(state.input_cursor, state.input.len());
    }

    #[test]
    fn rendered_cursor_tracks_the_input_editor_position() {
        let mut state = TuiState::new("goal");
        assert!(state.set_input("ab"));
        state.move_input_left();
        let backend = ratatui::backend::TestBackend::new(80, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        terminal.draw(|frame| render(frame, &mut state)).unwrap();

        assert_eq!(
            terminal.get_cursor_position().unwrap(),
            ratatui::layout::Position::new(3, 11)
        );
    }

    #[test]
    fn multiline_paste_is_flattened_for_the_single_line_editor() {
        let mut state = TuiState::new("goal");

        assert!(state.try_append_paste("first\r\nsecond\nthird\rfourth"));

        assert_eq!(state.input, "first second third fourth");
        assert_eq!(state.input_cursor, state.input.len());
    }

    #[test]
    fn terminal_setup_enters_alternate_screen_without_capturing_the_mouse() {
        let mut output = Vec::new();

        write_terminal_setup(&mut output).unwrap();

        let output = String::from_utf8(output).unwrap();
        // Alternate screen gives a clean full-height canvas (no header corruption,
        // no clobbering of the user's scrollback) with bracketed paste. The mouse
        // is NOT captured, so native click-drag selection and copy keep working;
        // the transcript scrolls with the arrow / PageUp-PageDown keys.
        assert!(output.contains("\u{1b}[?1049h"));
        assert!(output.contains("\u{1b}[?2004h"));
        assert!(!output.contains("\u{1b}[?1000h"));
        assert!(!output.contains("\u{1b}[?1006h"));
    }

    #[test]
    fn terminal_teardown_restores_the_original_screen() {
        let mut output = Vec::new();

        write_terminal_teardown(&mut output).unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("\u{1b}[?1049l"));
        assert!(output.contains("\u{1b}[?2004l"));
    }

    #[test]
    fn active_submission_is_not_counted_as_waiting_in_the_queue() {
        let mut state = TuiState::new("goal");
        state.pending_submissions = 1;
        state.apply_event(RuntimeEvent::TurnStarted {
            agent_id: AgentId::main(),
        });

        let active_only = render_buffer(&mut state, 80, 12);
        assert!(!active_only.contains("queued"));

        state.pending_submissions = 3;
        let waiting = render_buffer(&mut state, 80, 12);
        assert!(waiting.contains("2 queued"));
    }

    #[test]
    fn working_status_reports_elapsed_time() {
        let mut state = TuiState::new("goal");
        state.apply_event(RuntimeEvent::TurnStarted {
            agent_id: AgentId::main(),
        });

        let rendered = render_buffer(&mut state, 80, 12);

        assert!(rendered.contains(" · 0s"));
    }

    #[test]
    fn escape_interrupts_and_ctrl_c_clears_instead_of_quitting() {
        assert_eq!(
            global_key_action(crossterm::event::KeyEvent::new(
                KeyCode::Esc,
                KeyModifiers::NONE,
            )),
            Some(GlobalKeyAction::Interrupt),
        );
        assert_eq!(
            global_key_action(crossterm::event::KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
            )),
            Some(GlobalKeyAction::ClearInput),
        );
    }

    #[test]
    fn turn_finished_flushes_response_and_stops_loading() {
        let mut state = TuiState::new("goal");
        let main = AgentId::main();
        state.apply_event(RuntimeEvent::TurnStarted {
            agent_id: main.clone(),
        });
        state.apply_event(RuntimeEvent::Delta {
            agent_id: main.clone(),
            delta: ModelDelta::Reasoning("reason".to_owned()),
        });
        state.apply_event(RuntimeEvent::Delta {
            agent_id: main.clone(),
            delta: ModelDelta::Text("answer".to_owned()),
        });
        state.apply_event(RuntimeEvent::TurnFinished {
            agent_id: main,
            success: true,
        });

        let rendered = render_buffer(&mut state, 80, 11);
        assert!(rendered.contains("answer"));
        assert!(rendered.contains("main turn complete"));
        assert!(!rendered.contains("main working"));
    }

    #[test]
    fn overlapping_turns_keep_loading_until_every_agent_finishes() {
        let mut state = TuiState::new("goal");
        let main = AgentId::main();
        let worker = AgentId::new("worker-01").unwrap();
        state.apply_event(RuntimeEvent::TurnStarted {
            agent_id: main.clone(),
        });
        state.apply_event(RuntimeEvent::TurnStarted {
            agent_id: worker.clone(),
        });
        state.apply_event(RuntimeEvent::TurnFinished {
            agent_id: main,
            success: true,
        });

        // Main has finished but the worker is still mid-turn, so the busy
        // indicator must keep spinning — any live agent turn keeps loading on.
        let rendered = render_buffer(&mut state, 80, 11);
        assert!(rendered.contains("working"));
        assert!(!rendered.contains("main turn"));
    }

    #[tokio::test]
    async fn submission_queue_is_bounded_fifo_and_returns_overflow_input() {
        let (sender, mut receiver) = submission_channel();
        for index in 0..MAX_PENDING_SUBMISSIONS {
            try_enqueue_submission(&sender, Submission::User(format!("message-{index}"))).unwrap();
        }
        let overflow = Submission::User("must-not-be-lost".to_owned());
        assert_eq!(
            try_enqueue_submission(&sender, overflow.clone()).unwrap_err(),
            overflow
        );
        for index in 0..MAX_PENDING_SUBMISSIONS {
            assert_eq!(
                receiver.recv().await.unwrap(),
                Submission::User(format!("message-{index}"))
            );
        }
    }

    #[test]
    fn accepted_background_submissions_are_counted_until_a_result_arrives() {
        let (sender, _receiver) = submission_channel();
        let mut state = TuiState::new("goal");
        enqueue_background(&mut state, &sender, Submission::Compact);
        assert_eq!(state.pending_submissions, 1);
    }

    #[test]
    fn accent_constant_is_opaque_lime() {
        use super::theme::palette;
        use ratatui::style::Color;
        assert_eq!(palette::ACCENT, Color::Rgb(0xC8, 0xFF, 0x00));
    }

    #[test]
    fn start_banner_shows_program_name_in_accent_and_rest_muted() {
        use super::theme::{palette, styles};
        let state = TuiState::new("goal");
        let first = state
            .transcript
            .front()
            .expect("banner is the first transcript entry");
        assert_eq!(first.tone, LineTone::Banner);
        let lines = display_lines(&state);
        let banner = lines
            .iter()
            .find(|line| line.to_string().contains("pentesting"))
            .expect("banner line is rendered");
        let spans: Vec<_> = banner.spans.iter().collect();
        assert!(
            spans
                .iter()
                .any(|span| span.content.contains("pentesting") && span.style == styles::accent()),
            "program name uses accent: {banner:?}"
        );
        assert!(
            spans.iter().any(|span| span.style == styles::muted()),
            "banner remainder stays muted: {banner:?}"
        );
        let (_, banner_color) = tone_style(LineTone::Banner);
        assert_eq!(banner_color, palette::ACCENT);
    }

    #[test]
    fn user_echo_uses_accent_for_speaker_only() {
        use super::theme::{palette, styles};
        let mut state = TuiState::new("goal");
        state.push_entry(
            "❯".to_owned(),
            "hello agent".to_owned(),
            LineTone::User,
            None,
        );
        let lines = display_lines(&state);
        let echo = lines
            .iter()
            .find(|line| line.to_string().contains("hello agent"))
            .expect("user echo is rendered");
        assert!(
            echo.spans
                .iter()
                .any(|span| span.content == "❯" && span.style == styles::accent()),
            "speaker uses accent: {echo:?}"
        );
        assert!(
            echo.spans
                .iter()
                .all(|span| !span.content.contains("hello agent")
                    || span.style.fg != Some(palette::ACCENT)),
            "echo text does not use accent: {echo:?}"
        );
        let (_, user_color) = tone_style(LineTone::User);
        assert_eq!(user_color, palette::ACCENT);
    }

    #[test]
    fn input_row_renders_prompt_in_accent_and_text_in_default() {
        use super::theme::styles;
        let mut state = TuiState::new("goal");
        assert!(state.set_input("hi"));
        let backend = ratatui::backend::TestBackend::new(80, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        // Input row is the last row (single-line input here).
        let cell = &terminal.backend().buffer()[(0, 11)];
        assert_eq!(cell.symbol(), "❯");
        assert_eq!(cell.fg, styles::accent().fg.unwrap());
        let text_cell = &terminal.backend().buffer()[(2, 11)];
        assert_eq!(text_cell.symbol(), "h");
        assert_ne!(text_cell.fg, styles::accent().fg.unwrap());
    }

    #[test]
    fn non_user_tones_never_use_the_accent() {
        use super::theme::palette;
        for tone in [
            LineTone::Agent,
            LineTone::Tool,
            LineTone::ToolResult { success: true },
            LineTone::ToolResult { success: false },
            LineTone::Message,
            LineTone::Fault,
        ] {
            let (_, color) = tone_style(tone);
            assert_ne!(color, palette::ACCENT, "tone {tone:?} must stay monochrome");
        }
        // Agent echo text itself stays at the default weight (no accent).
        let mut state = TuiState::new("goal");
        state.push_line("main", "agent reply");
        let lines = display_lines(&state);
        let reply = lines
            .iter()
            .find(|line| line.to_string().contains("agent reply"))
            .expect("agent line is rendered");
        assert!(
            reply
                .spans
                .iter()
                .all(|span| span.style.fg != Some(palette::ACCENT)),
            "agent reply must not use accent: {reply:?}"
        );
    }
}
