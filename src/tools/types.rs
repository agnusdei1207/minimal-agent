use async_trait::async_trait;
use serde::Deserialize;

use super::error::ToolError;
use crate::domain::AgentId;

#[async_trait]
pub trait WorkerSpawner: Send + Sync {
    async fn spawn(
        &self,
        caller: &AgentId,
        role: String,
        task: String,
    ) -> Result<AgentId, ToolError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    pub content: String,
    pub success: bool,
}

#[derive(Deserialize)]
pub struct BashInput {
    pub command: String,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Deserialize)]
pub struct TmuxInput {
    pub args: String,
}

#[derive(Deserialize)]
pub struct WorkspaceInput {
    pub op: String,
    pub path: String,
    pub content: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateInput {
    pub role: String,
    pub task: String,
}

#[derive(Deserialize)]
pub struct AssignInput {
    pub agent_id: String,
    pub role: String,
    pub task: String,
}

#[derive(Deserialize)]
pub struct SendInput {
    pub to: Vec<String>,
    pub kind: String,
    pub body: String,
    pub insight: Option<InsightInput>,
}

#[derive(Deserialize)]
pub struct InsightInput {
    pub id: String,
    pub label: String,
    pub text: String,
}

#[derive(Deserialize)]
pub struct WaitInput {
    pub timeout_ms: Option<u64>,
}

#[derive(Deserialize)]
pub struct InspectInput {
    pub agent_id: Option<String>,
}

#[derive(Deserialize)]
pub struct RecallInput {
    pub agent_id: String,
    pub reason: String,
}

#[derive(Deserialize)]
pub struct FinishInput {
    pub body: String,
}

#[derive(Deserialize)]
pub struct JournalInput {
    pub start: u64,
    pub end: u64,
}

#[derive(Deserialize)]
pub struct ReportInput {
    pub op: String,
    pub title: Option<String>,
    pub body: String,
}

#[derive(Debug, Deserialize)]
pub struct BriefInput {
    pub body: String,
}
