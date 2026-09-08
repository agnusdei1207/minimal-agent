use serde::{Deserialize, Serialize};

use crate::domain::{CompactionCoverage, InsightId, SequenceRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveReason {
    UnreadInbox,
    IncompleteTool,
    PartialOutput,
    RecentExchange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextEntry {
    pub range: SequenceRange,
    pub content: String,
    pub insight_ids: Vec<InsightId>,
    pub atomic_group: Option<String>,
    pub live_reason: Option<LiveReason>,
}

impl ContextEntry {
    pub fn completed(range: SequenceRange, content: impl Into<String>) -> Self {
        Self {
            range,
            content: content.into(),
            insight_ids: Vec::new(),
            atomic_group: None,
            live_reason: None,
        }
    }

    pub fn with_insight(mut self, insight_id: InsightId) -> Self {
        if !self.insight_ids.contains(&insight_id) {
            self.insight_ids.push(insight_id);
        }
        self
    }

    pub fn atomic(mut self, group: impl Into<String>) -> Self {
        self.atomic_group = Some(group.into());
        self
    }

    pub fn protect(mut self, reason: LiveReason) -> Self {
        self.live_reason = Some(reason);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionResult {
    pub markdown: String,
    pub coverage: CompactionCoverage,
    pub source_sha256: String,
    pub source_ranges: Vec<SequenceRange>,
    pub live_tail: Vec<ContextEntry>,
    pub projected_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionOutcome {
    NotNeeded,
    Compacted(CompactionResult),
    /// Last-resort fallback when semantic compaction cannot proceed: keep only
    /// these context ranges (the protected live tail — the same kept-set the
    /// semantic path uses, so tool-call/result atomic groups are never split) and
    /// drop the rest from context. The brief is unchanged and the run journal still
    /// holds every dropped entry (INTENT-0001 §9 ledger). Prevents a fail-closed
    /// compaction from stalling the agent forever.
    MechanicallyTrimmed {
        kept_ranges: Vec<SequenceRange>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResponse {
    pub markdown: String,
    pub source_sha256: String,
    #[serde(flatten)]
    pub coverage: CompactionCoverage,
}

#[derive(Debug)]
pub struct Partition {
    pub source: String,
    pub ranges: Vec<SequenceRange>,
    pub insight_ids: Vec<InsightId>,
}

#[derive(Debug)]
pub struct SourceUnit {
    pub prefix: String,
    pub body: String,
    pub ranges: Vec<SequenceRange>,
    pub insight_ids: Vec<InsightId>,
    pub atomic: bool,
}
