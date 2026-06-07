use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{Mutex, OnceLock},
};

use super::*;

#[test]
fn merge_settings_uses_default_collection_when_persisted_collection_missing() {
    let defaults = default_settings(&[("AXON_COLLECTION".to_string(), "docs".to_string())]);

    let merged = merge_settings(PartialPaletteSettings::default(), defaults);

    assert_eq!(merged.collection, "docs");
    assert!(merged.hide_on_blur);
}

#[test]
fn merge_settings_keeps_persisted_collection_over_default() {
    let defaults = default_settings(&[("AXON_COLLECTION".to_string(), "docs".to_string())]);
    let persisted = PartialPaletteSettings {
        collection: Some("saved".to_string()),
        ..PartialPaletteSettings::default()
    };

    let merged = merge_settings(persisted, defaults);

    assert_eq!(merged.collection, "saved");
}

#[test]
fn parse_settings_json_reports_path_on_malformed_settings() {
    let path = Path::new("/tmp/axon-palette/settings.json");
    let err = parse_settings_json("{not json", path).expect_err("malformed settings fail");

    assert!(err.contains("/tmp/axon-palette/settings.json"));
    assert!(err.contains("failed to parse palette settings"));
}

#[test]
fn env_file_writer_preserves_comments_and_updates_values() {
    let _guard = env_lock().lock().expect("env lock");
    let dir = tempfile_dir("env-roundtrip");
    let path = dir.join(".env");
    fs::write(&path, "# keep me\nAXON_COLLECTION=old\nUNKNOWN=value\n").expect("seed env");
    unsafe {
        std::env::set_var("AXON_ENV_PATH", &path);
    }
    let mut values = HashMap::new();
    values.insert(
        "AXON_COLLECTION".to_string(),
        serde_json::Value::String("docs".to_string()),
    );
    values.insert(
        "TEI_URL".to_string(),
        serde_json::Value::String("http://127.0.0.1:52000".to_string()),
    );

    write_axon_env_values(&values).expect("write env values");

    let contents = fs::read_to_string(&path).expect("read env");
    assert!(contents.contains("# keep me"));
    assert!(contents.contains("UNKNOWN=value"));
    assert!(contents.contains("AXON_COLLECTION=docs"));
    assert!(contents.contains("TEI_URL=http://127.0.0.1:52000"));
    unsafe {
        std::env::remove_var("AXON_ENV_PATH");
    }
}

#[test]
fn config_file_writer_updates_nested_toml_sections() {
    let _guard = env_lock().lock().expect("env lock");
    let dir = tempfile_dir("config-roundtrip");
    let path = dir.join("config.toml");
    fs::write(&path, "[search]\ncollection = \"old\"\n").expect("seed config");
    unsafe {
        std::env::set_var("AXON_CONFIG_PATH", &path);
    }
    let mut values = HashMap::new();
    values.insert(
        "search.collection".to_string(),
        serde_json::Value::String("docs".to_string()),
    );
    values.insert(
        "ask.cache.enabled".to_string(),
        serde_json::Value::Bool(true),
    );
    values.insert(
        "ask.chunk-limit".to_string(),
        serde_json::Value::Number(20.into()),
    );

    write_axon_config_values(&values).expect("write config values");
    let loaded = read_default_config_values();

    assert_eq!(
        loaded
            .get("search.collection")
            .and_then(|value| value.as_str()),
        Some("docs")
    );
    assert_eq!(
        loaded
            .get("ask.cache.enabled")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        loaded
            .get("ask.chunk-limit")
            .and_then(|value| value.as_i64()),
        Some(20)
    );
    unsafe {
        std::env::remove_var("AXON_CONFIG_PATH");
    }
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn tempfile_dir(name: &str) -> std::path::PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("axon-palette-{name}-{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}
