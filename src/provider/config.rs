use std::collections::HashMap;
use std::time::Duration;
use url::Url;

use super::constants::CHAT_COMPLETIONS_ENDPOINT_PATH;
use super::fault::ProviderFault;

#[derive(Clone)]
pub struct OpenAiConfig {
    pub base_url: Url,
    pub api_key: String,
    pub model: String,
    pub context_tokens: u64,
    pub max_output_tokens: Option<u64>,
    pub timeout: Duration,
    pub headers: HashMap<String, String>,
}

impl OpenAiConfig {
    pub fn endpoint(&self) -> Result<Url, ProviderFault> {
        let base = self.base_url.as_str().trim_end_matches('/');
        let endpoint = if base.ends_with(CHAT_COMPLETIONS_ENDPOINT_PATH) {
            base.to_owned()
        } else {
            format!("{base}{CHAT_COMPLETIONS_ENDPOINT_PATH}")
        };
        endpoint
            .parse()
            .map_err(|error| ProviderFault::Configuration {
                message: format!("invalid chat endpoint: {error}"),
            })
    }
}
