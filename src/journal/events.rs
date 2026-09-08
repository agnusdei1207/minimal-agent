use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::domain::{AgentId, AgentMessage, AgentState, CompactionCoverage, SequenceRange};
use crate::engagement::Engagement;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JournalEvent {
    AgentCreated {
        agent_id: AgentId,
        role: String,
        task: String,
    },
    AgentAssigned {
        agent_id: AgentId,
        role: String,
        task: String,
    },
    AgentStateChanged {
        agent_id: AgentId,
        from: AgentState,
        to: AgentState,
        reason: String,
    },
    AgentMessage {
        message: AgentMessage,
    },
    MessageConsumed {
        recipient: AgentId,
        message_ids: Vec<Uuid>,
    },
    Transcript {
        agent_id: AgentId,
        role: TranscriptRole,
        content: String,
        complete: bool,
        atomic_group: Option<String>,
    },
    ToolCall {
        agent_id: AgentId,
        call_id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        agent_id: AgentId,
        call_id: String,
        content: String,
        success: bool,
    },
    BriefCheckpoint {
        agent_id: AgentId,
        markdown: String,
        markdown_sha256: String,
        source_sha256: String,
        source_ranges: Vec<SequenceRange>,
        coverage: CompactionCoverage,
    },
    /// A battlefield note the agent authored directly via the `brief` tool,
    /// decoupled from coverage-proven compaction so a weak model can always keep
    /// its strategy current (INTENT-0001 §9.1). Last write wins.
    BriefNote {
        agent_id: AgentId,
        markdown: String,
    },
    Finding {
        agent_id: AgentId,
        title: String,
        body: String,
    },
    Final {
        agent_id: AgentId,
        body: String,
    },
    Fault {
        agent_id: Option<AgentId>,
        code: String,
        message: String,
    },
    /// Durable authorized-engagement context so a resumed run recovers the same
    /// target/scope/flag doctrine it was created with (INTENT-0002 §4).
    EngagementSet {
        engagement: Engagement,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalEventKind {
    AgentCreated,
    AgentAssigned,
    AgentStateChanged,
    AgentMessage,
    MessageConsumed,
    Transcript,
    ToolCall,
    ToolResult,
    BriefCheckpoint,
    BriefNote,
    Finding,
    Final,
    Fault,
    EngagementSet,
}

impl JournalEvent {
    pub fn kind(&self) -> JournalEventKind {
        match self {
            Self::AgentCreated { .. } => JournalEventKind::AgentCreated,
            Self::AgentAssigned { .. } => JournalEventKind::AgentAssigned,
            Self::AgentStateChanged { .. } => JournalEventKind::AgentStateChanged,
            Self::AgentMessage { .. } => JournalEventKind::AgentMessage,
            Self::MessageConsumed { .. } => JournalEventKind::MessageConsumed,
            Self::Transcript { .. } => JournalEventKind::Transcript,
            Self::ToolCall { .. } => JournalEventKind::ToolCall,
            Self::ToolResult { .. } => JournalEventKind::ToolResult,
            Self::BriefCheckpoint { .. } => JournalEventKind::BriefCheckpoint,
            Self::BriefNote { .. } => JournalEventKind::BriefNote,
            Self::Finding { .. } => JournalEventKind::Finding,
            Self::Final { .. } => JournalEventKind::Final,
            Self::Fault { .. } => JournalEventKind::Fault,
            Self::EngagementSet { .. } => JournalEventKind::EngagementSet,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventStorage {
    Inline,
    Blob { sha256: String, bytes: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalAck {
    pub sequence: u64,
    pub storage: EventStorage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplayedEvent {
    pub sequence: u64,
    pub recorded_at: DateTime<Utc>,
    pub event: JournalEvent,
    pub storage: EventStorage,
}
