use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use async_trait::async_trait;
use tokio::sync::{RwLock, mpsc};

use super::fault::ProviderFault;
use super::traits::ModelProvider;
use super::types::{ModelDelta, ModelRequest, ModelTurn};

pub struct ProviderSlot {
    current: RwLock<Option<Arc<dyn ModelProvider>>>,
    active_model: RwLock<Option<String>>,
    configured: AtomicBool,
    context_limit: AtomicU64,
}

impl ProviderSlot {
    pub fn unconfigured(context_limit: u64) -> Self {
        Self {
            current: RwLock::new(None),
            active_model: RwLock::new(None),
            configured: AtomicBool::new(false),
            context_limit: AtomicU64::new(context_limit),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.configured.load(Ordering::Acquire)
    }

    pub fn runtime_context_limit(&self) -> u64 {
        self.context_limit.load(Ordering::Acquire)
    }

    pub async fn active_model(&self) -> Option<String> {
        self.active_model.read().await.clone()
    }

    pub async fn replace(&self, provider: Arc<dyn ModelProvider>, model: String) {
        let limit = provider.context_limit();
        if limit > 0 {
            self.context_limit.store(limit, Ordering::Release);
        }
        *self.current.write().await = Some(provider);
        *self.active_model.write().await = Some(model);
        self.configured.store(true, Ordering::Release);
    }
}

#[async_trait]
impl ModelProvider for ProviderSlot {
    fn context_limit(&self) -> u64 {
        self.context_limit.load(Ordering::Acquire)
    }

    async fn complete(
        &self,
        request: ModelRequest,
        deltas: Option<mpsc::UnboundedSender<ModelDelta>>,
    ) -> Result<ModelTurn, ProviderFault> {
        let provider =
            self.current
                .read()
                .await
                .clone()
                .ok_or_else(|| ProviderFault::Configuration {
                    message: "no model configured; run /model to choose one".to_owned(),
                })?;
        provider.complete(request, deltas).await
    }
}
