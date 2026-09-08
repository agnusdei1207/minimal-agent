use minimal_agent::engagement::Engagement;
use minimal_agent::journal::{JournalEvent, JournalEventKind, RunJournal};
use minimal_agent::runtime::RuntimeEvent;

use super::constants::{LOG_MESSAGE_SNIPPET_BYTES, LOG_SNIPPET_BYTES, MAX_TOOL_EVENT_BYTES};

#[derive(Default)]
pub struct HeadlessObservation {
    pub summary: String,
    pub flag: Option<String>,
}

impl HeadlessObservation {
    pub fn apply(&mut self, event: RuntimeEvent) {
        match event {
            RuntimeEvent::Assistant { text, .. } if !text.trim().is_empty() => {
                self.summary = text.trim().to_owned();
            }
            _ => {}
        }
    }
}

/// Trim and cap a value for a single debug log line, noting the omitted length.
pub fn truncate_for_log(text: &str, max: usize) -> String {
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

pub fn log_debug_event(event: &RuntimeEvent) {
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

pub fn headless_journal_evidence(
    journal: &RunJournal,
    engagement: &Engagement,
    after: u64,
) -> anyhow::Result<Option<String>> {
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
