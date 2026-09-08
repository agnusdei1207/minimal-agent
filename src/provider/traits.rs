use async_trait::async_trait;
use tokio::sync::mpsc;

use super::fault::ProviderFault;
use super::types::{ModelDelta, ModelRequest, ModelTurn};

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn context_limit(&self) -> u64;

    async fn complete(
        &self,
        request: ModelRequest,
        deltas: Option<mpsc::UnboundedSender<ModelDelta>>,
    ) -> Result<ModelTurn, ProviderFault>;
}
