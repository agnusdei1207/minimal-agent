use serde_json::json;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::compaction::{ContextEntry, LiveReason};
use crate::coordinator::MessageDelivery;
use crate::domain::{AgentId, SequenceRange};
use crate::engagement::Engagement;
use crate::journal::{JournalEvent, RunJournal, TranscriptRole};
use crate::provider::{ModelMessage, ModelRole, ToolCall};

use super::error::RuntimeError;
use super::session::{
    AgentSession, SessionRecord, fallback_tool_group, model_role, sequence_is_covered,
};

pub fn inbox_record(delivery: &MessageDelivery) -> Result<SessionRecord, RuntimeError> {
    let content = format!(
        "TEAM INBOX MESSAGE (durable; retain until semantically compacted)\n{}",
        serde_json::to_string(&json!({
            "sequence":delivery.sequence,
            "message":delivery.message,
        }))?
    );
    let mut entry = ContextEntry::completed(
        SequenceRange::new(delivery.sequence, delivery.sequence)?,
        content.clone(),
    )
    .protect(LiveReason::UnreadInbox);
    if let Some(insight) = &delivery.message.insight {
        entry = entry.with_insight(insight.id.clone());
    }
    Ok(SessionRecord {
        message: ModelMessage::new(ModelRole::User, content),
        context: entry,
    })
}

/// Recover the latest engagement recorded for a run, if any (INTENT-0002 §4).
pub fn recover_engagement(journal: &RunJournal) -> Result<Option<Engagement>, RuntimeError> {
    let mut engagement = None;
    for entry in journal.replay()? {
        if let JournalEvent::EngagementSet {
            engagement: recorded,
        } = entry.event
        {
            engagement = Some(recorded);
        }
    }
    Ok(engagement)
}

pub fn recover_sessions(
    journal: &RunJournal,
) -> Result<HashMap<AgentId, AgentSession>, RuntimeError> {
    let replay = journal.replay()?;
    let mut covered_ranges = HashMap::<AgentId, Vec<SequenceRange>>::new();
    let mut delivered_messages = HashMap::<(AgentId, Uuid), MessageDelivery>::new();
    let mut consumed_messages = HashSet::<(AgentId, Uuid)>::new();
    for entry in &replay {
        match &entry.event {
            JournalEvent::BriefCheckpoint {
                agent_id,
                source_ranges,
                ..
            } => {
                covered_ranges
                    .entry(agent_id.clone())
                    .or_default()
                    .extend(source_ranges.iter().copied());
            }
            JournalEvent::AgentMessage { message } => {
                for recipient in &message.audience {
                    delivered_messages.insert(
                        (recipient.clone(), message.id),
                        MessageDelivery {
                            sequence: entry.sequence,
                            message: message.clone(),
                        },
                    );
                }
            }
            JournalEvent::MessageConsumed {
                recipient,
                message_ids,
            } => {
                consumed_messages.extend(
                    message_ids
                        .iter()
                        .map(|message_id| (recipient.clone(), *message_id)),
                );
            }
            _ => {}
        }
    }

    let mut sessions = HashMap::<AgentId, AgentSession>::new();
    let mut pending_tools = HashMap::<(AgentId, String), (u64, ToolCall, String)>::new();
    for entry in replay {
        match entry.event {
            JournalEvent::Transcript {
                agent_id,
                role,
                content,
                complete,
                atomic_group,
            } if !sequence_is_covered(
                entry.sequence,
                covered_ranges
                    .get(&agent_id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            ) =>
            {
                if !complete && crate::provider::has_control_token(&content) {
                    continue;
                }
                let mut message = ModelMessage::new(model_role(role), content.clone());
                if role == TranscriptRole::Assistant
                    && let Ok(calls) = serde_json::from_str::<Vec<ToolCall>>(&content)
                    && !calls.is_empty()
                {
                    message.content.clear();
                    message.tool_calls = calls;
                }
                let mut context = ContextEntry::completed(
                    SequenceRange::new(entry.sequence, entry.sequence)?,
                    content,
                );
                if let Some(group) = atomic_group {
                    context = context.atomic(group);
                }
                if !complete {
                    context = context.protect(LiveReason::PartialOutput);
                }
                sessions
                    .entry(agent_id)
                    .or_default()
                    .records
                    .push(SessionRecord { message, context });
            }
            JournalEvent::ToolCall {
                agent_id,
                call_id,
                name,
                arguments,
            } if !sequence_is_covered(
                entry.sequence,
                covered_ranges
                    .get(&agent_id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            ) =>
            {
                let call = ToolCall {
                    id: call_id.clone(),
                    name,
                    arguments,
                };
                let session = sessions.entry(agent_id.clone()).or_default();
                let group = if let Some(record) = session
                    .records
                    .iter_mut()
                    .rev()
                    .find(|record| record.message.role == ModelRole::Assistant)
                {
                    if !record
                        .message
                        .tool_calls
                        .iter()
                        .any(|existing| existing.id == call_id)
                    {
                        record.message.tool_calls.push(call.clone());
                    }
                    record
                        .context
                        .atomic_group
                        .get_or_insert_with(|| fallback_tool_group(record.context.range.start))
                        .clone()
                } else {
                    let group = fallback_tool_group(entry.sequence);
                    let mut message = ModelMessage::new(ModelRole::Assistant, "");
                    message.tool_calls.push(call.clone());
                    session.records.push(SessionRecord {
                        message,
                        context: ContextEntry::completed(
                            SequenceRange::new(entry.sequence, entry.sequence)?,
                            format!("tool call {call_id}"),
                        )
                        .atomic(group.clone()),
                    });
                    group
                };
                pending_tools.insert((agent_id, call_id), (entry.sequence, call, group));
            }
            JournalEvent::ToolResult {
                agent_id,
                call_id,
                content,
                ..
            } if !sequence_is_covered(
                entry.sequence,
                covered_ranges
                    .get(&agent_id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            ) =>
            {
                let (start, group) = pending_tools
                    .remove(&(agent_id.clone(), call_id.clone()))
                    .map(|(sequence, _, group)| (sequence, group))
                    .unwrap_or_else(|| (entry.sequence, call_id.clone()));
                let mut message = ModelMessage::new(ModelRole::Tool, content.clone());
                message.tool_call_id = Some(call_id.clone());
                sessions
                    .entry(agent_id)
                    .or_default()
                    .records
                    .push(SessionRecord {
                        message,
                        context: ContextEntry::completed(
                            SequenceRange::new(start, entry.sequence)?,
                            content,
                        )
                        .atomic(group),
                    });
            }
            _ => {}
        }
    }

    for ((agent_id, call_id), (sequence, _, group)) in pending_tools {
        let mut message = ModelMessage::new(
            ModelRole::Tool,
            "Tool execution was interrupted by restart and was not replayed.",
        );
        message.tool_call_id = Some(call_id.clone());
        sessions
            .entry(agent_id)
            .or_default()
            .records
            .push(SessionRecord {
                message,
                context: ContextEntry::completed(
                    SequenceRange::new(sequence, sequence)?,
                    "incomplete tool execution; side effect not replayed",
                )
                .atomic(group)
                .protect(LiveReason::IncompleteTool),
            });
    }
    let mut consumed_by_agent = HashMap::<AgentId, Vec<MessageDelivery>>::new();
    for (agent_id, message_id) in consumed_messages {
        let Some(delivery) = delivered_messages.remove(&(agent_id.clone(), message_id)) else {
            continue;
        };
        if !sequence_is_covered(
            delivery.sequence,
            covered_ranges
                .get(&agent_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        ) {
            consumed_by_agent
                .entry(agent_id)
                .or_default()
                .push(delivery);
        }
    }
    for (agent_id, mut deliveries) in consumed_by_agent {
        deliveries.sort_by_key(|delivery| delivery.sequence);
        let session = sessions.entry(agent_id).or_default();
        for delivery in deliveries {
            let mut record = inbox_record(&delivery)?;
            record.context.live_reason = None;
            session.records.push(record);
        }
    }
    Ok(sessions)
}
