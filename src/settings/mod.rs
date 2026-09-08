pub mod constants;
pub mod env;
pub mod provider;
pub mod store;

pub use env::{debug_enabled, env_context_tokens, env_max_output_tokens, parse_token_input};
pub use provider::ProviderSettings;
pub use store::ProviderSettingsStore;

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
