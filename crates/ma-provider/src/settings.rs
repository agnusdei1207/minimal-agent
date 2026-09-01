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
        let context_tokens = std::env::var("OPENAI_CONTEXT_TOKENS")
            .ok()
            .map(|value| value.parse::<u64>())
            .transpose()?
            .unwrap_or(128_000);
        anyhow::ensure!(context_tokens > 0, "OPENAI_CONTEXT_TOKENS must be positive");
        Ok(Some(Self {
            provider: "openai-compatible".to_owned(),
            base_url,
            model,
            api_key,
            context_tokens,
        }))
    }

    pub fn build_provider(&self) -> anyhow::Result<OpenAiChatProvider> {
        Ok(OpenAiChatProvider::new(OpenAiConfig {
            base_url: self.base_url.parse()?,
            api_key: self.api_key.clone(),
            model: self.model.clone(),
            context_tokens: self.context_tokens,
            timeout: std::time::Duration::from_secs(120),
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
