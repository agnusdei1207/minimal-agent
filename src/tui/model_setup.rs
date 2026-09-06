use std::sync::Arc;

use crate::provider::ProviderSlot;
use crate::settings::{ProviderSettings, ProviderSettingsStore, parse_token_input};

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
    MaxOutputTokens {
        base_url: String,
        api_key: String,
        model: String,
        context_tokens: u64,
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
        } => accept_context_tokens(state, value, base_url, api_key, model),
        ModelSetup::MaxOutputTokens {
            base_url,
            api_key,
            model,
            context_tokens,
        } => {
            save_configuration(
                state,
                value,
                base_url,
                api_key,
                model,
                context_tokens,
                slot,
                store,
            )
            .await
        }
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
        "model setup: enter context token count (suffix k/m allowed, e.g. 128k, 1m, default 128k)"
            .to_owned();
}

fn accept_context_tokens(
    state: &mut TuiState,
    value: String,
    base_url: String,
    api_key: String,
    model: String,
) {
    let context_tokens = if value.is_empty() {
        128_000
    } else {
        match parse_token_input(&value) {
            Some(tokens) if tokens > 0 => tokens,
            _ => {
                restore_context_prompt(state, base_url, api_key, model);
                state.status =
                    "context tokens must be a positive number (suffix k/m allowed, e.g. 128k or 1m), or Enter for default (128k)"
                        .to_owned();
                return;
            }
        }
    };
    state.model_setup = Some(ModelSetup::MaxOutputTokens {
        base_url,
        api_key,
        model,
        context_tokens,
    });
    state.status =
        "model setup: enter max output tokens (suffix k/m allowed, e.g. 16k, 32k, default 32k, or Enter for default)"
            .to_owned();
}

#[allow(clippy::too_many_arguments)]
async fn save_configuration(
    state: &mut TuiState,
    value: String,
    base_url: String,
    api_key: String,
    model: String,
    context_tokens: u64,
    slot: &Arc<ProviderSlot>,
    store: &ProviderSettingsStore,
) {
    let max_output_tokens = if value.is_empty() {
        32_768u64.min(context_tokens.saturating_sub(1).max(1))
    } else {
        match parse_token_input(&value) {
            Some(tokens) if tokens > 0 => tokens,
            _ => {
                restore_output_tokens_prompt(state, base_url, api_key, model, context_tokens);
                state.status =
                    "max output tokens must be a positive number (suffix k/m allowed, e.g. 16k or 32k), or Enter for default"
                        .to_owned();
                return;
            }
        }
    };

    if max_output_tokens >= context_tokens {
        restore_output_tokens_prompt(state, base_url, api_key, model, context_tokens);
        state.status = format!(
            "max output tokens ({max_output_tokens}) must be less than context tokens ({context_tokens})"
        );
        return;
    }

    let settings = ProviderSettings {
        provider: "openai-compatible".to_owned(),
        base_url,
        model: model.clone(),
        api_key,
        context_tokens,
        max_output_tokens: Some(max_output_tokens),
    };
    match settings.build_provider().and_then(|provider| {
        store.save(&settings)?;
        Ok(provider)
    }) {
        Ok(provider) => {
            slot.replace(Arc::new(provider), model.clone()).await;
            state.push_line(
                "model",
                format!("active model: {model} (context: {context_tokens}, output max: {max_output_tokens})"),
            );
            state.status = "model configuration saved".to_owned();
        }
        Err(error) => {
            state.push_line("model", format!("configuration failed: {error}"));
            state.status = "model configuration not changed".to_owned();
        }
    }
}

fn restore_context_prompt(state: &mut TuiState, base_url: String, api_key: String, model: String) {
    state.model_setup = Some(ModelSetup::ContextTokens {
        base_url,
        api_key,
        model,
    });
}

fn restore_output_tokens_prompt(
    state: &mut TuiState,
    base_url: String,
    api_key: String,
    model: String,
    context_tokens: u64,
) {
    state.model_setup = Some(ModelSetup::MaxOutputTokens {
        base_url,
        api_key,
        model,
        context_tokens,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn model_setup_accepts_context_and_max_output_tokens() {
        let mut state = TuiState::new("goal");
        let slot = Arc::new(ProviderSlot::unconfigured(128_000));
        let directory = tempfile::tempdir().unwrap();
        let store = ProviderSettingsStore::new(directory.path());

        accept_context_tokens(
            &mut state,
            "1m".to_owned(),
            "https://example.test/v1".to_owned(),
            "secret".to_owned(),
            "small-model".to_owned(),
        );
        assert!(matches!(
            state.model_setup,
            Some(ModelSetup::MaxOutputTokens {
                context_tokens: 1_048_576,
                ..
            })
        ));

        save_configuration(
            &mut state,
            "32k".to_owned(),
            "https://example.test/v1".to_owned(),
            "secret".to_owned(),
            "small-model".to_owned(),
            1_048_576,
            &slot,
            &store,
        )
        .await;

        assert!(slot.is_configured(), "model should be configured");
        let saved = store.load().unwrap().unwrap();
        assert_eq!(saved.context_tokens, 1_048_576);
        assert_eq!(saved.max_output_tokens, Some(32_768));
    }

    #[tokio::test]
    async fn model_setup_defaults_tokens_on_empty_input() {
        let mut state = TuiState::new("goal");
        let slot = Arc::new(ProviderSlot::unconfigured(128_000));
        let directory = tempfile::tempdir().unwrap();
        let store = ProviderSettingsStore::new(directory.path());

        accept_context_tokens(
            &mut state,
            "".to_owned(),
            "https://example.test/v1".to_owned(),
            "secret".to_owned(),
            "small-model".to_owned(),
        );
        assert!(matches!(
            state.model_setup,
            Some(ModelSetup::MaxOutputTokens {
                context_tokens: 128_000,
                ..
            })
        ));

        save_configuration(
            &mut state,
            "".to_owned(),
            "https://example.test/v1".to_owned(),
            "secret".to_owned(),
            "small-model".to_owned(),
            128_000,
            &slot,
            &store,
        )
        .await;

        let saved = store.load().unwrap().unwrap();
        assert_eq!(saved.context_tokens, 128_000);
        assert_eq!(saved.max_output_tokens, Some(32_768));
    }

    #[tokio::test]
    async fn model_setup_rejects_output_tokens_exceeding_context() {
        let mut state = TuiState::new("goal");
        let slot = Arc::new(ProviderSlot::unconfigured(128_000));
        let directory = tempfile::tempdir().unwrap();
        let store = ProviderSettingsStore::new(directory.path());

        save_configuration(
            &mut state,
            "128k".to_owned(),
            "https://example.test/v1".to_owned(),
            "secret".to_owned(),
            "small-model".to_owned(),
            64_000,
            &slot,
            &store,
        )
        .await;

        assert!(!slot.is_configured());
        assert!(state.status.contains("must be less than context tokens"));
    }
}
