use uuid::Uuid;

use crate::domain::{AgentDepth, AgentId, AgentMessage, AgentState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSnapshot {
    pub id: AgentId,
    pub depth: AgentDepth,
    /// The direct parent in the team tree; `None` only for main (INTENT-0004).
    pub parent: Option<AgentId>,
    pub role: String,
    pub task: String,
    pub state: AgentState,
    pub unread: usize,
    pub latest_insight: Option<String>,
    pub waiting_on: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageDelivery {
    pub sequence: u64,
    pub message: AgentMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendReceipt {
    pub sequence: u64,
    pub message_id: Uuid,
    pub audience: Vec<AgentId>,
}
