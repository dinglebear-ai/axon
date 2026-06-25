#[path = "../build.rs"]
#[allow(dead_code)]
mod build_script;

#[test]
fn build_script_allows_fallback_web_assets_from_config_toml() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("config.toml");
    std::fs::write(&config_path, "[build]\nallow-fallback-web-assets = true\n")
        .expect("write config");

    assert!(build_script::allow_fallback_assets_from(
        None,
        Some(config_path.as_path()),
        dir.path(),
    ));
}

#[test]
fn build_script_env_override_still_allows_fallback_web_assets() {
    let dir = tempfile::tempdir().expect("tempdir");

    assert!(build_script::allow_fallback_assets_from(
        Some("1"),
        None,
        dir.path(),
    ));
}
