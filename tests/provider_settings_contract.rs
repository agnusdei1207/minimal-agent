use minimal_agent::settings::{ProviderSettings, ProviderSettingsStore};

#[test]
fn provider_settings_round_trip_outside_the_run_journal() {
    let root = tempfile::tempdir().unwrap();
    let store = ProviderSettingsStore::new(root.path());
    let expected = ProviderSettings {
        provider: "openrouter".to_owned(),
        base_url: "https://openrouter.ai/api/v1".to_owned(),
        model: "example/model".to_owned(),
        api_key: "secret".to_owned(),
        context_tokens: 128_000,
        max_output_tokens: Some(32_768),
    };

    store.save(&expected).unwrap();

    assert_eq!(store.load().unwrap(), Some(expected));
    assert_eq!(store.path(), root.path().join("provider.json"));
}

#[test]
fn provider_settings_loads_legacy_json_without_max_output_tokens() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("provider.json");
    std::fs::write(
        &path,
        r#"{
        "provider": "openrouter",
        "base_url": "https://openrouter.ai/api/v1",
        "model": "legacy-model",
        "api_key": "legacy-key",
        "context_tokens": 65536
    }"#,
    )
    .unwrap();

    let store = ProviderSettingsStore::new(root.path());
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.model, "legacy-model");
    assert_eq!(loaded.context_tokens, 65536);
    assert_eq!(loaded.max_output_tokens, None);
}
