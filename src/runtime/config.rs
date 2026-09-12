use std::time::Duration;

use crate::compaction::SemanticCompactionConfig;
use crate::domain::ContextBudget;
use crate::engagement::Engagement;
use crate::journal::JournalConfig;

use super::constants::{
    DEFAULT_COMPACTION_TIMEOUT_SECS, DEFAULT_CONFIGURED_CONTEXT_TOKENS,
    DEFAULT_MAX_PARALLEL_REQUESTS, DEFAULT_RESERVED_RESPONSE_TOKENS, DEFAULT_TOOL_TIMEOUT_SECS,
};
use super::error::RuntimeError;

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub configured_context_tokens: u64,
    pub reserved_response_tokens: u64,
    pub max_model_turns: usize,
    pub max_parallel_requests: usize,
    pub tool_timeout: Duration,
    pub compaction_timeout: Duration,
    pub auto: bool,
    pub journal: JournalConfig,
    pub compaction: SemanticCompactionConfig,
    /// Optional authorized-engagement context injected from outside (INTENT-0002).
    /// Rendered into every agent's system prompt; never persisted to the journal.
    pub engagement: Option<Engagement>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let max_model_turns = std::env::var("PENTESTING_MAX_MODEL_TURNS")
            .ok()
            .and_then(|val| {
                let trimmed = val.trim();
                if trimmed.eq_ignore_ascii_case("unlimited")
                    || trimmed.eq_ignore_ascii_case("infinite")
                    || trimmed == "0"
                {
                    Some(usize::MAX)
                } else {
                    trimmed.parse().ok()
                }
            })
            .unwrap_or(usize::MAX);
        // Tokens reserved for the model's response. This value is both carved out
        // of the context window (compaction headroom) and sent verbatim as the
        // provider `max_completion_tokens` cap. Reasoning backbones (e.g. DeepSeek)
        // spend several thousand tokens thinking before emitting the tool-call
        // JSON, so an 8k cap truncates the call mid-object (EOF parse fault). It is
        // overridable via OPENAI_MAX_TOKENS / OPENAI_MAX_OUTPUT_TOKENS with k/m suffix support.
        let reserved_response_tokens =
            crate::settings::env_max_output_tokens().unwrap_or(DEFAULT_RESERVED_RESPONSE_TOKENS);
        // Operator-imposed context ceiling, min()'d against the provider's real
        // window. Read from OPENAI_CONTEXT_TOKENS (or aliases) with k/m suffix support
        // so the ceiling tracks the model's actual context; it must stay strictly
        // greater than reserved_response_tokens.
        let configured_context_tokens =
            crate::settings::env_context_tokens().unwrap_or(DEFAULT_CONFIGURED_CONTEXT_TOKENS);
        Self {
            configured_context_tokens,
            reserved_response_tokens,
            max_model_turns,
            max_parallel_requests: DEFAULT_MAX_PARALLEL_REQUESTS,
            tool_timeout: Duration::from_secs(DEFAULT_TOOL_TIMEOUT_SECS),
            compaction_timeout: Duration::from_secs(DEFAULT_COMPACTION_TIMEOUT_SECS),
            auto: false,
            journal: JournalConfig::default(),
            compaction: SemanticCompactionConfig::default(),
            engagement: None,
        }
    }
}

impl RuntimeConfig {
    pub fn validate(&self, provider_context: u64) -> Result<ContextBudget, RuntimeError> {
        if self.max_parallel_requests == 0
            || self.tool_timeout.is_zero()
            || self.compaction_timeout.is_zero()
        {
            return Err(RuntimeError::InvalidConfig);
        }
        let max_context = self.configured_context_tokens.min(provider_context);
        let reserved = self.reserved_response_tokens.min(max_context / 2);
        Ok(ContextBudget::new(
            self.configured_context_tokens,
            provider_context,
            reserved,
        )?)
    }
}
