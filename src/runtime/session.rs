use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::compaction::{ContextEntry, LiveReason};
use crate::coordinator::MessageDelivery;
use crate::domain::{InsightId, SequenceRange};
use crate::journal::TranscriptRole;
use crate::provider::{ModelMessage, ModelRole};

use super::engine::RuntimeInner;
use super::error::RuntimeError;
use super::recovery::inbox_record;

#[derive(Default)]
pub struct AgentSession {
    pub records: Vec<SessionRecord>,
    pub last_request_activity: u64,
}

impl AgentSession {
    pub fn integrate_inbox(&mut self, inbox: &[MessageDelivery]) -> Result<(), RuntimeError> {
        for delivery in inbox {
            let already_integrated = self.records.iter().any(|record| {
                record.context.live_reason == Some(LiveReason::UnreadInbox)
                    && record.context.range.start == delivery.sequence
                    && record.context.range.end == delivery.sequence
            });
            if !already_integrated {
                self.records.push(inbox_record(delivery)?);
            }
        }
        Ok(())
    }

    pub fn acknowledge_inbox(&mut self, inbox: &[MessageDelivery]) {
        let sequences = inbox
            .iter()
            .map(|delivery| delivery.sequence)
            .collect::<HashSet<_>>();
        for record in &mut self.records {
            if record.context.live_reason == Some(LiveReason::UnreadInbox)
                && record.context.range.start == record.context.range.end
                && sequences.contains(&record.context.range.start)
            {
                record.context.live_reason = None;
            }
        }
    }

    pub fn release_recovery_protection(&mut self) {
        for record in &mut self.records {
            if matches!(
                record.context.live_reason,
                Some(LiveReason::PartialOutput | LiveReason::IncompleteTool)
            ) {
                record.context.live_reason = None;
            }
        }
    }
}

pub struct SessionRecord {
    pub message: ModelMessage,
    pub context: ContextEntry,
}

pub struct ActiveTurn {
    inner: Arc<RuntimeInner>,
}

impl ActiveTurn {
    pub fn new(inner: Arc<RuntimeInner>) -> Self {
        inner.active_turns.fetch_add(1, Ordering::AcqRel);
        Self { inner }
    }
}

impl Drop for ActiveTurn {
    fn drop(&mut self) {
        self.inner.active_turns.fetch_sub(1, Ordering::AcqRel);
    }
}

pub fn fallback_tool_group(sequence: u64) -> String {
    format!("model-turn-{sequence}")
}

pub fn model_role(role: TranscriptRole) -> ModelRole {
    match role {
        TranscriptRole::System => ModelRole::System,
        TranscriptRole::User => ModelRole::User,
        TranscriptRole::Assistant => ModelRole::Assistant,
        TranscriptRole::Tool => ModelRole::Tool,
    }
}

pub fn unique_ids(ids: impl IntoIterator<Item = InsightId>) -> Vec<InsightId> {
    let mut seen = HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

pub fn sequence_is_covered(sequence: u64, ranges: &[SequenceRange]) -> bool {
    ranges
        .iter()
        .any(|range| sequence >= range.start && sequence <= range.end)
}
