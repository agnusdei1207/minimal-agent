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
    };

    store.save(&expected).unwrap();

    assert_eq!(store.load().unwrap(), Some(expected));
    assert_eq!(store.path(), root.path().join("provider.json"));
}
