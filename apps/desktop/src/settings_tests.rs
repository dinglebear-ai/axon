use super::AxonSettings;

#[test]
fn loads_known_env_and_toml_values() {
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    let toml_path = dir.path().join("config.toml");
    std::fs::write(
        &env_path,
        "AXON_SERVER_URL=http://127.0.0.1:9000\nAXON_MCP_HTTP_TOKEN=secret\n",
    )
    .unwrap();
    std::fs::write(
        &toml_path,
        "[ask]\nchunk-limit = 12\n[workers]\ningest-lanes = 4\n",
    )
    .unwrap();

    let settings = AxonSettings::load(env_path, toml_path).unwrap();

    assert_eq!(
        value_for(&settings, "AXON_SERVER_URL"),
        "http://127.0.0.1:9000"
    );
    assert_eq!(value_for(&settings, "AXON_MCP_HTTP_TOKEN"), "secret");
    assert_eq!(value_for(&settings, "ask.chunk-limit"), "12");
    assert_eq!(value_for(&settings, "workers.ingest-lanes"), "4");
}

#[test]
fn saves_only_changed_values() {
    let dir = tempfile::tempdir().unwrap();
    let env_path = dir.path().join(".env");
    let toml_path = dir.path().join("config.toml");
    std::fs::write(&env_path, "# keep\nAXON_SERVER_URL=http://old\n").unwrap();
    std::fs::write(&toml_path, "[ask]\nchunk-limit = 12\n").unwrap();

    let mut settings = AxonSettings::load(env_path.clone(), toml_path.clone()).unwrap();
    set_value(&mut settings, "AXON_SERVER_URL", "http://new");
    set_value(&mut settings, "ask.chunk-limit", "16");
    settings.save();

    let env = std::fs::read_to_string(env_path).unwrap();
    let toml = std::fs::read_to_string(toml_path).unwrap();
    assert!(env.contains("# keep"));
    assert!(env.contains("AXON_SERVER_URL=http://new"));
    assert!(toml.contains("chunk-limit = 16"));
    assert_eq!(settings.dirty_count(), 0);
}

fn value_for(settings: &AxonSettings, key: &str) -> String {
    settings
        .entries
        .iter()
        .find(|entry| entry.key == key)
        .unwrap()
        .value
        .clone()
}

fn set_value(settings: &mut AxonSettings, key: &str, value: &str) {
    settings
        .entries
        .iter_mut()
        .find(|entry| entry.key == key)
        .unwrap()
        .value = value.to_string();
}
