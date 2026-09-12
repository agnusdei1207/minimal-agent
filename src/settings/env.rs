use super::constants::{ENV_CONTEXT_TOKEN_KEYS, ENV_MAX_OUTPUT_TOKEN_KEYS};

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
    ENV_CONTEXT_TOKEN_KEYS.iter().find_map(|&key| {
        std::env::var(key)
            .ok()
            .and_then(|val| parse_token_input(&val))
            .filter(|&tokens| tokens > 0)
    })
}

/// Whether verbose debug logging is enabled via `PENTESTING_DEBUG`. Single source
/// of truth for every debug gate.
pub fn debug_enabled() -> bool {
    std::env::var_os("PENTESTING_DEBUG").is_some()
}

/// Resolve the maximum output/completion tokens from standard environment variables.
/// Accepts k/m suffixes (e.g. `16k`, `32k`).
pub fn env_max_output_tokens() -> Option<u64> {
    ENV_MAX_OUTPUT_TOKEN_KEYS.iter().find_map(|&key| {
        std::env::var(key)
            .ok()
            .and_then(|val| parse_token_input(&val))
            .filter(|&tokens| tokens > 0)
    })
}
