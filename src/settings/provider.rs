use serde::{Deserialize, Serialize};

use crate::provider::{OpenAiChatProvider, OpenAiConfig};

use super::constants::{
    DEFAULT_BASE_URL, DEFAULT_CONTEXT_TOKENS, DEFAULT_PROVIDER_TIMEOUT_SECS, PROVIDER_KIND_OPENAI,
};
use super::env::{env_context_tokens, env_max_output_tokens};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSettings {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub context_tokens: u64,
    #[serde(default, alias = "max_tokens", alias = "max_completion_tokens")]
    pub max_output_tokens: Option<u64>,
}

impl ProviderSettings {
    pub fn from_standard_env() -> anyhow::Result<Option<Self>> {
        let Some(api_key) = std::env::var("OPENAI_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
        else {
            return Ok(None);
        };
        let Some(model) = std::env::var("OPENAI_MODEL")
            .ok()
            .filter(|value| !value.trim().is_empty())
        else {
            return Ok(None);
        };
        let base_url =
            std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        let context_tokens = env_context_tokens().unwrap_or(DEFAULT_CONTEXT_TOKENS);
        let max_output_tokens = env_max_output_tokens();
        Ok(Some(Self {
            provider: PROVIDER_KIND_OPENAI.to_owned(),
            base_url,
            model,
            api_key,
            context_tokens,
            max_output_tokens,
        }))
    }

    pub fn build_provider(&self) -> anyhow::Result<OpenAiChatProvider> {
        Ok(OpenAiChatProvider::new(OpenAiConfig {
            base_url: self.base_url.parse()?,
            api_key: self.api_key.clone(),
            model: self.model.clone(),
            context_tokens: self.context_tokens,
            max_output_tokens: self.max_output_tokens,
            timeout: std::env::var("PENTESTING_PROVIDER_TIMEOUT")
                .or_else(|_| std::env::var("MINIMAL_AGENT_PROVIDER_TIMEOUT"))
                .or_else(|_| std::env::var("OPENAI_TIMEOUT"))
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|secs| *secs > 0)
                .map(std::time::Duration::from_secs)
                .unwrap_or(std::time::Duration::from_secs(
                    DEFAULT_PROVIDER_TIMEOUT_SECS,
                )),
            headers: Default::default(),
        })?)
    }
}
