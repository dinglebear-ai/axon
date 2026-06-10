use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{Mutex, OnceLock},
};

use super::*;
use persistence::{write_axon_config_values, write_axon_env_values};

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

// ── Allowlist tests ────────────────────────────────────────────────────────

#[test]
fn env_writer_rejects_unknown_key() {
    let _guard = env_lock().lock().expect("env lock");
    let dir = tempfile_dir("env-allowlist-reject");
    let path = dir.join(".env");
    fs::write(&path, "").expect("seed env");
    unsafe {
        std::env::set_var("AXON_ENV_PATH", &path);
    }
    let mut values = HashMap::new();
    values.insert(
        "EVIL_UNKNOWN_KEY".to_string(),
        serde_json::Value::String("bad".to_string()),
    );

    let result = write_axon_env_values(&values);

    assert!(
        result.is_err(),
        "unknown env key must be rejected by the allowlist"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("allowlist") || msg.contains("EVIL_UNKNOWN_KEY"),
        "error should mention allowlist or the offending key: {msg}"
    );
    unsafe {
        std::env::remove_var("AXON_ENV_PATH");
    }
}

#[test]
fn config_writer_rejects_unknown_key() {
    let _guard = env_lock().lock().expect("env lock");
    let dir = tempfile_dir("config-allowlist-reject");
    let path = dir.join("config.toml");
    fs::write(&path, "").expect("seed config");
    unsafe {
        std::env::set_var("AXON_CONFIG_PATH", &path);
    }
    let mut values = HashMap::new();
    values.insert(
        "evil.unknown".to_string(),
        serde_json::Value::String("bad".to_string()),
    );

    let result = write_axon_config_values(&values);

    assert!(
        result.is_err(),
        "unknown config key must be rejected by the allowlist"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("allowlist") || msg.contains("evil.unknown"),
        "error should mention allowlist or the offending key: {msg}"
    );
    unsafe {
        std::env::remove_var("AXON_CONFIG_PATH");
    }
}

#[test]
fn env_writer_succeeds_with_allowed_key() {
    let _guard = env_lock().lock().expect("env lock");
    let dir = tempfile_dir("env-allowlist-ok");
    let path = dir.join(".env");
    fs::write(&path, "QDRANT_URL=http://old\n").expect("seed env");
    unsafe {
        std::env::set_var("AXON_ENV_PATH", &path);
    }
    let mut values = HashMap::new();
    values.insert(
        "QDRANT_URL".to_string(),
        serde_json::Value::String("http://new:6333".to_string()),
    );

    let result = write_axon_env_values(&values);

    assert!(result.is_ok(), "allowed key must succeed: {:?}", result);
    let contents = fs::read_to_string(&path).expect("read env");
    assert!(contents.contains("QDRANT_URL=http://new:6333"));
    unsafe {
        std::env::remove_var("AXON_ENV_PATH");
    }
}

// ── Atomic write / permissions tests ─────────────────────────────────────

#[test]
fn env_file_exists_after_write() {
    let _guard = env_lock().lock().expect("env lock");
    let dir = tempfile_dir("env-exists");
    let path = dir.join(".env");
    unsafe {
        std::env::set_var("AXON_ENV_PATH", &path);
    }
    let mut values = HashMap::new();
    values.insert(
        "TEI_URL".to_string(),
        serde_json::Value::String("http://127.0.0.1:52000".to_string()),
    );

    write_axon_env_values(&values).expect("write env");

    assert!(path.exists(), ".env must exist after write");
    unsafe {
        std::env::remove_var("AXON_ENV_PATH");
    }
}

#[cfg(unix)]
#[test]
fn env_file_has_private_permissions_after_write() {
    use std::os::unix::fs::PermissionsExt;

    let _guard = env_lock().lock().expect("env lock");
    let dir = tempfile_dir("env-perms");
    let path = dir.join(".env");
    unsafe {
        std::env::set_var("AXON_ENV_PATH", &path);
    }
    let mut values = HashMap::new();
    values.insert(
        "TEI_URL".to_string(),
        serde_json::Value::String("http://127.0.0.1:52000".to_string()),
    );

    write_axon_env_values(&values).expect("write env");

    let mode = fs::metadata(&path).expect("metadata").permissions().mode();
    // Mask to the low 9 permission bits: expect 0o600 (owner rw only).
    assert_eq!(
        mode & 0o777,
        0o600,
        ".env file mode should be 0o600, got {:#o}",
        mode & 0o777
    );
    unsafe {
        std::env::remove_var("AXON_ENV_PATH");
    }
}

// ── Atomic write correctness ──────────────────────────────────────────────

#[test]
fn no_tmp_file_after_successful_atomic_write() {
    let _guard = env_lock().lock().expect("env lock");
    let dir = tempfile_dir("atomic-no-tmp");
    let path = dir.join(".env");
    unsafe {
        std::env::set_var("AXON_ENV_PATH", &path);
    }
    let mut values = HashMap::new();
    values.insert(
        "TEI_URL".to_string(),
        serde_json::Value::String("http://127.0.0.1:52000".to_string()),
    );

    write_axon_env_values(&values).expect("write env");

    let tmp = path.with_extension("tmp");
    assert!(
        !tmp.exists(),
        ".env.tmp must not exist after successful write"
    );
    unsafe {
        std::env::remove_var("AXON_ENV_PATH");
    }
}

// ── normalize_shortcut_label ──────────────────────────────────────────────

#[test]
fn normalize_shortcut_label_canonicalizes_known_aliases() {
    assert_eq!(normalize_shortcut_label("option+space"), "Alt+Space");
    assert_eq!(normalize_shortcut_label("Alt+Space"), "Alt+Space");
    assert_eq!(normalize_shortcut_label("ctrl+space"), "Ctrl+Space");
    assert_eq!(normalize_shortcut_label("Ctrl+Space"), "Ctrl+Space");
    assert_eq!(normalize_shortcut_label("control+space"), "Ctrl+Space");
    assert_eq!(
        normalize_shortcut_label("cmd+shift+space"),
        "Cmd+Shift+Space"
    );
    assert_eq!(
        normalize_shortcut_label("super+shift+space"),
        "Cmd+Shift+Space"
    );
    assert_eq!(
        normalize_shortcut_label("command+shift+space"),
        "Cmd+Shift+Space"
    );
}

#[test]
fn normalize_shortcut_label_falls_back_to_default_for_unknown() {
    assert_eq!(normalize_shortcut_label("Win+Q"), DEFAULT_SHORTCUT);
    assert_eq!(normalize_shortcut_label(""), DEFAULT_SHORTCUT);
    assert_eq!(normalize_shortcut_label("not-a-shortcut"), DEFAULT_SHORTCUT);
}

// ── Helpers ───────────────────────────────────────────────────────────────

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
