use super::*;

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
