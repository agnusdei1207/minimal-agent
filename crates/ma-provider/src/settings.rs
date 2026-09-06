use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use futures::StreamExt;
use serde::{Deserialize, Serialize};

use crate::provider::{OpenAiChatProvider, OpenAiConfig};

const MAX_MODELS_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

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

/// Parse a token count that may carry a `k`/`m`/`g` suffix
/// (e.g. `128k`, `1m`, `32k`). A bare integer is also accepted. Returns the value in
/// tokens; `None` on any malformed input.
pub fn parse_token_input(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let (digits, multiplier) = match value.chars().next_back() {
        Some('k' | 'K') => (&value[..value.len() - 1], 1024u64),
        Some('m' | 'M') => (&value[..value.len() - 1], 1024u64 * 1024),
        Some('g' | 'G') => (&value[..value.len() - 1], 1024u64 * 1024 * 1024),
        _ => (value, 1u64),
    };
    let number: u64 = digits.trim().parse().ok()?;
    number.checked_mul(multiplier)
}

/// Resolve the configured context token ceiling from standard environment variables.
/// Accepts k/m suffixes (e.g. `128k`, `1m`).
pub fn env_context_tokens() -> Option<u64> {
    const KEYS: &[&str] = &[
        "OPENAI_CONTEXT_TOKENS",
        "MINIMAL_AGENT_CONTEXT_TOKENS",
        "OPENAI_MAX_CONTEXT_TOKENS",
        "MINIMAL_AGENT_MAX_CONTEXT_TOKENS",
        "CONTEXT_TOKENS",
        "MAX_CONTEXT_TOKENS",
    ];
    KEYS.iter().find_map(|&key| {
        std::env::var(key)
            .ok()
            .and_then(|val| parse_token_input(&val))
            .filter(|&tokens| tokens > 0)
    })
}

/// Resolve the maximum output/completion tokens from standard environment variables.
/// Accepts k/m suffixes (e.g. `16k`, `32k`).
pub fn env_max_output_tokens() -> Option<u64> {
    const KEYS: &[&str] = &[
        "OPENAI_MAX_TOKENS",
        "OPENAI_MAX_OUTPUT_TOKENS",
        "OPENAI_OUTPUT_MAX_TOKENS",
        "OPENAI_MAX_COMPLETION_TOKENS",
        "MINIMAL_AGENT_MAX_TOKENS",
        "MINIMAL_AGENT_MAX_OUTPUT_TOKENS",
        "MINIMAL_AGENT_OUTPUT_MAX_TOKENS",
        "MAX_OUTPUT_TOKENS",
        "OUTPUT_MAX_TOKENS",
    ];
    KEYS.iter().find_map(|&key| {
        std::env::var(key)
            .ok()
            .and_then(|val| parse_token_input(&val))
            .filter(|&tokens| tokens > 0)
    })
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
        let base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_owned());
        let context_tokens = env_context_tokens().unwrap_or(128_000);
        let max_output_tokens = env_max_output_tokens();
        Ok(Some(Self {
            provider: "openai-compatible".to_owned(),
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
            timeout: std::env::var("MINIMAL_AGENT_PROVIDER_TIMEOUT")
                .or_else(|_| std::env::var("OPENAI_TIMEOUT"))
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|secs| *secs > 0)
                .map(std::time::Duration::from_secs)
                .unwrap_or(std::time::Duration::from_secs(300)),
            headers: Default::default(),
        })?)
    }
}

pub async fn fetch_model_ids(base_url: &str, api_key: &str) -> anyhow::Result<Vec<String>> {
    let endpoint = format!("{}/models", base_url.trim_end_matches('/'));
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?
        .get(endpoint)
        .bearer_auth(api_key)
        .send()
        .await?
        .error_for_status()?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MODELS_RESPONSE_BYTES as u64)
    {
        anyhow::bail!("model catalog response is too large");
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await.transpose()? {
        if bytes.len().saturating_add(chunk.len()) > MAX_MODELS_RESPONSE_BYTES {
            anyhow::bail!("model catalog response is too large");
        }
        bytes.extend_from_slice(&chunk);
    }
    #[derive(Deserialize)]
    struct Catalog {
        data: Vec<CatalogModel>,
    }
    #[derive(Deserialize)]
    struct CatalogModel {
        id: String,
    }
    let mut models = serde_json::from_slice::<Catalog>(&bytes)?
        .data
        .into_iter()
        .map(|model| model.id)
        .filter(|model| !model.trim().is_empty())
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    Ok(models)
}

#[derive(Debug, Clone)]
pub struct ProviderSettingsStore {
    path: PathBuf,
}

impl ProviderSettingsStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            path: root.as_ref().join("provider.json"),
        }
    }

    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub fn load(&self) -> anyhow::Result<Option<ProviderSettings>> {
        match fs::read(&self.path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save(&self, settings: &ProviderSettings) -> anyhow::Result<()> {
        let parent = self.path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "provider settings need a parent",
            )
        })?;
        fs::create_dir_all(parent)?;
        let temporary = self
            .path
            .with_extension(format!("json.{}.tmp", std::process::id()));
        let result = (|| -> anyhow::Result<()> {
            let mut options = OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            serde_json::to_writer(&mut file, settings)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_suffixed_token_inputs() {
        assert_eq!(parse_token_input("128000"), Some(128_000));
        assert_eq!(parse_token_input("16k"), Some(16 * 1024));
        assert_eq!(parse_token_input("32K"), Some(32 * 1024));
        assert_eq!(parse_token_input("1m"), Some(1024 * 1024));
        assert_eq!(parse_token_input("  2M  "), Some(2 * 1024 * 1024));
        assert_eq!(parse_token_input("1g"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_token_input(""), None);
        assert_eq!(parse_token_input("abc"), None);
        assert_eq!(parse_token_input("-10"), None);
    }

    #[test]
    fn provider_settings_deserializes_with_optional_max_output_tokens() {
        let json = r#"{
            "provider": "openai-compatible",
            "base_url": "https://api.openai.com/v1",
            "model": "gpt-4o",
            "api_key": "test-key",
            "context_tokens": 128000
        }"#;
        let settings: ProviderSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.max_output_tokens, None);

        let json_with_max_tokens = r#"{
            "provider": "openai-compatible",
            "base_url": "https://api.openai.com/v1",
            "model": "gpt-4o",
            "api_key": "test-key",
            "context_tokens": 128000,
            "max_tokens": 32768
        }"#;
        let settings: ProviderSettings = serde_json::from_str(json_with_max_tokens).unwrap();
        assert_eq!(settings.max_output_tokens, Some(32_768));

        let json_with_max_output_tokens = r#"{
            "provider": "openai-compatible",
            "base_url": "https://api.openai.com/v1",
            "model": "gpt-4o",
            "api_key": "test-key",
            "context_tokens": 128000,
            "max_output_tokens": 16384
        }"#;
        let settings: ProviderSettings = serde_json::from_str(json_with_max_output_tokens).unwrap();
        assert_eq!(settings.max_output_tokens, Some(16_384));
    }
}
