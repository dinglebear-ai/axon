use super::*;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn invalid_toml_is_rejected_before_write() {
    let result = toml::from_str::<toml::Value>("[broken");
    assert!(result.is_err());
}

#[test]
fn remote_dir_rejects_parent_components() {
    assert!(validate_remote_dir("../axon").is_err());
    assert!(validate_remote_dir("/tmp/axon").is_err());
    assert_eq!(validate_remote_dir("axon-deploy").unwrap(), "axon-deploy");
}

#[test]
fn remote_dir_rejects_shell_metacharacters() {
    for value in [
        "axon $(touch /tmp/pwn)",
        "axon/$(touch_pwn)",
        "axon`touch_pwn`",
        "axon\";touch pwn;#",
        "axon;touch-pwn",
        "axon deploy",
        "axon\npwn",
    ] {
        assert!(validate_remote_dir(value).is_err(), "{value:?} should fail");
    }
    assert_eq!(
        validate_remote_dir("axon-deploy/nested_1.2").unwrap(),
        "axon-deploy/nested_1.2"
    );
}

#[test]
fn render_env_value_round_trips_through_dotenvy() {
    for value in [
        "two words",
        "Let's go",
        r"path\\with\\slashes",
        "hash#value",
    ] {
        let raw = format!("TEST={}\n", render_env_value(value));
        let parsed = parse_env_pairs_from_str(&raw).unwrap();
        assert_eq!(parsed.get("TEST").map(String::as_str), Some(value));
    }
}

#[allow(unsafe_code)]
#[test]
fn write_remote_runtime_env_does_not_write_service_urls_to_toml() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("custom.toml");
    let env_path = dir.path().join(".env");
    let previous = std::env::var_os("AXON_CONFIG_PATH");
    unsafe {
        std::env::set_var("AXON_CONFIG_PATH", &config_path);
    }
    std::fs::write(
        &env_path,
        "TAVILY_API_KEY='secret value'\nAXON_HTTP_TOKEN=token\nCUSTOM_VALUE=\"value with spaces\"\n",
    )
    .unwrap();

    let written = write_remote_runtime_env(
        &env_path,
        "http://127.0.0.1:53333",
        "http://127.0.0.1:52000",
        "http://127.0.0.1:6000",
    )
    .unwrap();

    assert_eq!(written, env_path);
    let env_raw = std::fs::read_to_string(&written).unwrap();
    assert!(env_raw.contains("QDRANT_URL=http://127.0.0.1:53333"));
    assert!(env_raw.contains("TEI_URL=http://127.0.0.1:52000"));
    assert!(env_raw.contains("AXON_CHROME_REMOTE_URL=http://127.0.0.1:6000"));
    assert!(env_raw.contains("TAVILY_API_KEY='secret value'"));
    assert!(env_raw.contains("AXON_HTTP_TOKEN=token"));
    assert!(env_raw.contains("CUSTOM_VALUE='value with spaces'"));

    let config_raw = std::fs::read_to_string(&config_path).unwrap_or_default();
    assert!(!config_raw.contains("[services]"));
    assert!(!config_raw.contains("qdrant-url"));
    assert!(!config_raw.contains("tei-url"));

    unsafe {
        if let Some(previous) = previous {
            std::env::set_var("AXON_CONFIG_PATH", previous);
        } else {
            std::env::remove_var("AXON_CONFIG_PATH");
        }
    }
}
