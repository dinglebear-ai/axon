use super::*;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};

#[test]
fn isolated_config_disables_side_effects() {
    let dir = tempfile::tempdir().unwrap();
    write_isolated_config(dir.path(), Some("gpt-5.5")).unwrap();
    let toml = fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(toml.contains("model = \"gpt-5.5\""));
    assert!(toml.contains("approval_policy = \"never\""));
    assert!(toml.contains("sandbox_mode = \"read-only\""));
    assert!(toml.contains("apps = false"));
    assert!(toml.contains("hooks = false"));
    assert!(toml.contains("environment = \"off\""));
    // Must be valid TOML.
    toml::from_str::<toml::Value>(&toml).expect("valid TOML");
}

#[test]
fn isolated_config_omits_model_when_blank() {
    let dir = tempfile::tempdir().unwrap();
    write_isolated_config(dir.path(), Some("  ")).unwrap();
    let toml = fs::read_to_string(dir.path().join("config.toml")).unwrap();
    assert!(!toml.contains("model ="));
    toml::from_str::<toml::Value>(&toml).expect("valid TOML");
}

#[test]
fn isolated_config_escapes_model_quotes() {
    let dir = tempfile::tempdir().unwrap();
    write_isolated_config(dir.path(), Some(r#"weird"name"#)).unwrap();
    let toml = fs::read_to_string(dir.path().join("config.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&toml).expect("valid TOML despite quote in model");
    assert_eq!(parsed["model"].as_str().unwrap(), r#"weird"name"#);
}

#[test]
fn existing_dir_filters_missing_paths() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        existing_dir(dir.path().to_path_buf()),
        Some(dir.path().to_path_buf())
    );
    assert_eq!(existing_dir(dir.path().join("nope")), None);
}

#[test]
fn validate_source_home_rejects_non_dir() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let err = validate_source_home(file.path().to_path_buf()).unwrap_err();
    assert!(err.to_string().contains("must be a directory"));
}

#[cfg(unix)]
#[test]
fn validate_source_home_rejects_symlink() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real-home");
    fs::create_dir(&real).unwrap();
    let link = dir.path().join("home-link");
    symlink(&real, &link).unwrap();
    let err = validate_source_home(link).unwrap_err();
    assert!(
        err.to_string().contains("must not be a symlink"),
        "got: {err}"
    );
}

#[test]
fn copy_auth_copies_when_present() {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    fs::write(src.path().join("auth.json"), r#"{"token":"x"}"#).unwrap();
    copy_auth(src.path(), dst.path()).unwrap();
    assert_eq!(
        fs::read_to_string(dst.path().join("auth.json")).unwrap(),
        r#"{"token":"x"}"#
    );
}

#[test]
fn codex_child_env_keeps_openai_key_but_not_external_base_url() {
    let mut command = Command::new("env");
    apply_codex_env_allowlist_from(
        &mut command,
        [
            (
                OsString::from("OPENAI_API_KEY"),
                OsString::from("sk-test-key"),
            ),
            (
                OsString::from("OPENAI_BASE_URL"),
                OsString::from("http://localhost:8080/v1"),
            ),
        ],
    );
    let envs: BTreeMap<_, _> = command
        .as_std()
        .get_envs()
        .filter_map(|(key, value)| value.map(|v| (key.to_os_string(), v.to_os_string())))
        .collect();

    assert_eq!(
        envs.get(OsStr::new("OPENAI_API_KEY")).unwrap(),
        OsStr::new("sk-test-key")
    );
    assert!(!envs.contains_key(OsStr::new("OPENAI_BASE_URL")));
}

#[test]
fn codex_child_env_rehomes_home_and_xdg_dirs_to_isolated_home() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = Command::new("env");

    apply_codex_env_allowlist(&mut command);
    apply_codex_home_env(&mut command, dir.path());

    let envs: BTreeMap<_, _> = command
        .as_std()
        .get_envs()
        .filter_map(|(key, value)| value.map(|v| (key.to_os_string(), v.to_os_string())))
        .collect();

    assert_eq!(envs.get(OsStr::new("HOME")).unwrap(), dir.path());
    assert_eq!(envs.get(OsStr::new("CODEX_HOME")).unwrap(), dir.path());
    assert_eq!(
        envs.get(OsStr::new("XDG_CONFIG_HOME")).unwrap(),
        &dir.path().join(".config")
    );
    assert_eq!(
        envs.get(OsStr::new("XDG_CACHE_HOME")).unwrap(),
        &dir.path().join(".cache")
    );
    assert_eq!(
        envs.get(OsStr::new("XDG_DATA_HOME")).unwrap(),
        &dir.path().join(".local/share")
    );
}

#[cfg(unix)]
#[test]
fn copy_auth_rejects_symlinked_auth_json() {
    use std::os::unix::fs::symlink;

    let source = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    symlink(outside.path(), source.path().join("auth.json")).unwrap();

    let err = copy_auth(source.path(), dest.path()).unwrap_err();

    assert!(err.to_string().contains("auth.json must not be a symlink"));
}

#[cfg(unix)]
#[test]
fn copy_auth_writes_destination_auth_json_0600() {
    use std::os::unix::fs::PermissionsExt;

    let source = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    fs::write(source.path().join("auth.json"), "{}").unwrap();

    copy_auth(source.path(), dest.path()).unwrap();

    let mode = fs::metadata(dest.path().join("auth.json"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn copy_auth_is_noop_when_absent() {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    copy_auth(src.path(), dst.path()).unwrap();
    assert!(!dst.path().join("auth.json").exists());
}

#[test]
fn prepare_codex_home_copies_auth_and_writes_isolated_config() {
    let src = tempfile::tempdir().unwrap();
    fs::write(src.path().join("auth.json"), "{}").unwrap();
    let cfg = LlmBackendConfig {
        codex_home: Some(src.path().to_path_buf()),
        codex_model: Some("gpt-5.5".to_string()),
        ..LlmBackendConfig::default()
    };
    let home = prepare_codex_home(&cfg).unwrap();
    assert!(home.path().join("auth.json").exists(), "auth.json copied");
    let written = fs::read_to_string(home.path().join("config.toml")).unwrap();
    assert!(written.contains("model = \"gpt-5.5\""));
    assert!(written.contains("approval_policy = \"never\""));
}

#[test]
fn resolve_user_codex_home_honors_existing_override() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = LlmBackendConfig {
        codex_home: Some(dir.path().to_path_buf()),
        ..LlmBackendConfig::default()
    };
    assert_eq!(
        resolve_user_codex_home(&cfg).unwrap(),
        Some(dir.path().to_path_buf())
    );
}

#[test]
fn resolve_user_codex_home_errors_on_missing_override() {
    let cfg = LlmBackendConfig {
        codex_home: Some(std::path::PathBuf::from("/nonexistent/axon-codex-home-xyz")),
        ..LlmBackendConfig::default()
    };
    assert!(resolve_user_codex_home(&cfg).is_err());
}
