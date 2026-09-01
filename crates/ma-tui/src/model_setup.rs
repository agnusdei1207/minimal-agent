use std::sync::Arc;

use ma_provider::provider::ProviderSlot;
use ma_provider::settings::{ProviderSettings, ProviderSettingsStore};

use super::TuiState;

#[derive(Debug, Clone)]
pub(super) enum ModelSetup {
    ApiKey {
        query: Option<String>,
    },
    CustomUrl {
        api_key: String,
        query: Option<String>,
    },
    Model {
        base_url: String,
        api_key: String,
    },
    ContextTokens {
        base_url: String,
        api_key: String,
        model: String,
    },
}

pub(super) fn begin(state: &mut TuiState, query: Option<String>) {
    state.model_setup = Some(ModelSetup::ApiKey { query });
    state.status = "model setup: enter API key (input is hidden); Ctrl+C cancels".to_owned();
}

pub(super) fn cancel(state: &mut TuiState) -> bool {
    if state.model_setup.take().is_none() {
        return false;
    }
    state.clear_input();
    state.status = "model setup cancelled".to_owned();
    true
}

pub(super) async fn advance(
    state: &mut TuiState,
    input: String,
    slot: &Arc<ProviderSlot>,
    store: &ProviderSettingsStore,
) {
    let Some(setup) = state.model_setup.take() else {
        return;
    };
    let value = input.trim().to_owned();
    if value.eq_ignore_ascii_case("cancel") {
        state.status = "model setup cancelled".to_owned();
        return;
    }

    match setup {
        ModelSetup::ApiKey { query } => accept_api_key(state, value, query),
        ModelSetup::CustomUrl { api_key, query } => accept_base_url(state, value, api_key, query),
        ModelSetup::Model { base_url, api_key } => accept_model(state, value, base_url, api_key),
        ModelSetup::ContextTokens {
            base_url,
            api_key,
            model,
        } => save_configuration(state, value, base_url, api_key, model, slot, store).await,
    }
}

fn accept_api_key(state: &mut TuiState, value: String, query: Option<String>) {
    if value.is_empty() {
        state.model_setup = Some(ModelSetup::ApiKey { query });
        state.status = "API key cannot be empty; enter it or cancel".to_owned();
        return;
    }
    state.model_setup = Some(ModelSetup::CustomUrl {
        api_key: value,
        query,
    });
    state.status = "model setup: enter the OpenAI-compatible base URL".to_owned();
}

fn accept_base_url(state: &mut TuiState, value: String, api_key: String, query: Option<String>) {
    match url::Url::parse(&value) {
        Ok(url) if matches!(url.scheme(), "http" | "https") => {
            state.model_setup = Some(ModelSetup::Model {
                base_url: url.to_string().trim_end_matches('/').to_owned(),
                api_key,
            });
            state.status = query.map_or_else(
                || "model setup: enter model name".to_owned(),
                |query| format!("model setup: enter model name (suggested: {query})"),
            );
        }
        _ => {
            state.model_setup = Some(ModelSetup::CustomUrl { api_key, query });
            state.status = "enter a valid http(s) base URL, or cancel".to_owned();
        }
    }
}

fn accept_model(state: &mut TuiState, value: String, base_url: String, api_key: String) {
    if value.is_empty() {
        state.model_setup = Some(ModelSetup::Model { base_url, api_key });
        state.status = "model name cannot be empty; enter it or cancel".to_owned();
        return;
    }
    state.model_setup = Some(ModelSetup::ContextTokens {
        base_url,
        api_key,
        model: value,
    });
    state.status =
        "model setup: enter context token count (suffix k/m allowed, e.g. 128k, 1m)".to_owned();
}

async fn save_configuration(
    state: &mut TuiState,
    value: String,
    base_url: String,
    api_key: String,
    model: String,
    slot: &Arc<ProviderSlot>,
    store: &ProviderSettingsStore,
) {
    let context_tokens = match parse_token_input(&value) {
        Some(tokens) => tokens,
        None => {
            restore_context_prompt(state, base_url, api_key, model);
            state.status =
                "context tokens must be a positive number (suffix k/m allowed, e.g. 128k or 1m)"
                    .to_owned();
            return;
        }
    };
    if context_tokens == 0 {
        restore_context_prompt(state, base_url, api_key, model);
        state.status = "context tokens must be greater than zero".to_owned();
        return;
    }

    let settings = ProviderSettings {
        provider: "openai-compatible".to_owned(),
        base_url,
        model: model.clone(),
        api_key,
        context_tokens,
    };
    match settings.build_provider().and_then(|provider| {
        store.save(&settings)?;
        Ok(provider)
    }) {
        Ok(provider) => {
            slot.replace(Arc::new(provider), model.clone()).await;
            state.push_line("model", format!("active model: {model}"));
            state.status = "model configuration saved".to_owned();
        }
        Err(error) => {
            state.push_line("model", format!("configuration failed: {error}"));
            state.status = "model configuration not changed".to_owned();
        }
    }
}

/// Parse a context-token count that may carry a `k`/`m` suffix
/// (e.g. `128k`, `1m`). A bare integer is also accepted. Returns the value in
/// tokens; `None` on any malformed input.
fn parse_token_input(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let (digits, multiplier) = match value.chars().next_back() {
        Some('k' | 'K') => (&value[..value.len() - 1], 1024u64),
        Some('m' | 'M') => (&value[..value.len() - 1], 1024u64 * 1024),
        _ => (value, 1u64),
    };
    let number: u64 = digits.trim().parse().ok()?;
    number.checked_mul(multiplier)
}

fn restore_context_prompt(state: &mut TuiState, base_url: String, api_key: String, model: String) {
    state.model_setup = Some(ModelSetup::ContextTokens {
        base_url,
        api_key,
        model,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_suffixed_tokens() {
        assert_eq!(parse_token_input("128000"), Some(128_000));
        assert_eq!(parse_token_input("128k"), Some(128 * 1024));
        assert_eq!(parse_token_input("1m"), Some(1024 * 1024));
        assert_eq!(parse_token_input("  1M "), Some(1024 * 1024));
    }

    #[test]
    fn rejects_garbage_and_zero() {
        assert_eq!(parse_token_input("1x"), None);
        assert_eq!(parse_token_input("k"), None);
        assert_eq!(parse_token_input(""), None);
        assert_eq!(parse_token_input("0"), Some(0));
        assert_eq!(parse_token_input("k1"), None);
    }

    #[tokio::test]
    async fn model_setup_accepts_suffixed_tokens() {
        let mut state = TuiState::new("goal");
        let slot = Arc::new(ProviderSlot::unconfigured(128_000));
        let directory = tempfile::tempdir().unwrap();
        let store = ProviderSettingsStore::new(directory.path());
        // A 1m context budget is now accepted (previously rejected by a bare
        // integer parse or the >= runtime_context_limit guard).
        save_configuration(
            &mut state,
            "1m".to_owned(),
            "https://example.test/v1".to_owned(),
            "secret".to_owned(),
            "small-model".to_owned(),
            &slot,
            &store,
        )
        .await;
        assert!(slot.is_configured(), "model should be configured after 1m");
    }
}
