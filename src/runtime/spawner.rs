use std::sync::Weak;

use crate::domain::AgentId;
use crate::tools::{ToolError, WorkerSpawner};

use super::engine::{RuntimeInner, TeamRuntime};

pub struct RuntimeSpawner {
    pub inner: Weak<RuntimeInner>,
}

#[async_trait::async_trait]
impl WorkerSpawner for RuntimeSpawner {
    async fn spawn(
        &self,
        caller: &AgentId,
        role: String,
        task: String,
    ) -> Result<AgentId, ToolError> {
        let inner = self
            .inner
            .upgrade()
            .ok_or_else(|| ToolError::Spawner("runtime has stopped".to_owned()))?;
        TeamRuntime { inner }
            .spawn_worker(caller, role, task)
            .await
            .map_err(|error| ToolError::Spawner(error.to_string()))
    }
}
