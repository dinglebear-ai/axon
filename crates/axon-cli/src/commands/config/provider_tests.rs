use super::*;

#[test]
fn provider_names_extracts_unique_sorted_names() {
    let flat = BTreeMap::from([
        (
            "providers.codex.backend".to_string(),
            "codex-app-server".to_string(),
        ),
        ("providers.codex.model".to_string(), "gpt-5.5".to_string()),
        (
            "providers.gem.backend".to_string(),
            "gemini-headless".to_string(),
        ),
        ("search.collection".to_string(), "axon".to_string()),
    ]);
    assert_eq!(
        provider_names(&flat),
        vec!["codex".to_string(), "gem".to_string()]
    );
}

#[test]
fn provider_names_empty_when_no_providers() {
    let flat = BTreeMap::from([("ask.full-docs".to_string(), "6".to_string())]);
    assert!(provider_names(&flat).is_empty());
}

#[test]
fn validate_field_accepts_known_fields() {
    for f in ["backend", "model", "base-url", "api-key", "cmd", "home"] {
        assert!(validate_field(f).is_ok(), "{f} should be valid");
    }
}

#[test]
fn validate_field_rejects_unknown() {
    let err = validate_field("modle").unwrap_err().to_string();
    assert!(err.contains("unknown provider field"));
    assert!(err.contains("model"));
}

#[test]
fn fields_const_is_accepted_by_toml_provider_schema() {
    // Every settable CLI field must be a real `TomlProvider` field. Otherwise a
    // `provider set`-written key would be rejected on the next config load by
    // `deny_unknown_fields` and brick every command. This guards FIELDS ⊆ schema.
    let body: String = FIELDS.iter().map(|f| format!("{f} = \"x\"\n")).collect();
    let toml = format!("[providers.sample]\n{body}");
    assert!(
        axon_core::config::parse::validate_toml_config_text(&toml).is_ok(),
        "FIELDS contains a key TomlProvider rejects:\n{toml}"
    );
}

/// Restores `AXON_CONFIG_PATH` on drop so a panicking test can't leak state.
struct ConfigPathGuard(Option<String>);

impl ConfigPathGuard {
    fn point_at(path: &std::path::Path) -> Self {
        let prev = std::env::var("AXON_CONFIG_PATH").ok();
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("AXON_CONFIG_PATH", path);
        }
        Self(prev)
    }
}

impl Drop for ConfigPathGuard {
    fn drop(&mut self) {
        #[allow(unsafe_code)]
        match self.0.take() {
            Some(v) => unsafe { std::env::set_var("AXON_CONFIG_PATH", v) },
            None => unsafe { std::env::remove_var("AXON_CONFIG_PATH") },
        }
    }
}

fn cfg_provider(args: &[&str]) -> Config {
    let mut cfg = Config::default_minimal();
    cfg.positional = std::iter::once("provider".to_string())
        .chain(args.iter().map(|s| (*s).to_string()))
        .collect();
    cfg
}

fn flat_at(path: &std::path::Path) -> BTreeMap<String, String> {
    svc::flatten_toml(&svc::read_toml_document(path).unwrap())
}

#[test]
#[serial_test::serial]
fn provider_crud_round_trip_and_remove_clears_active() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    let _guard = ConfigPathGuard::point_at(&path);

    run_provider(&cfg_provider(&[
        "add",
        "foo",
        "gemini-headless",
        "model=gemini-3.1-flash",
    ]))
    .unwrap();
    let flat = flat_at(&path);
    assert_eq!(
        flat.get("providers.foo.backend").map(String::as_str),
        Some("gemini-headless")
    );
    assert_eq!(
        flat.get("providers.foo.model").map(String::as_str),
        Some("gemini-3.1-flash")
    );

    run_provider(&cfg_provider(&["use", "foo"])).unwrap();
    assert_eq!(
        flat_at(&path)
            .get("llm.active-provider")
            .map(String::as_str),
        Some("foo")
    );

    run_provider(&cfg_provider(&["set", "foo", "model", "gemini-3.1-pro"])).unwrap();
    assert_eq!(
        flat_at(&path)
            .get("providers.foo.model")
            .map(String::as_str),
        Some("gemini-3.1-pro")
    );

    // Removing the active profile must also clear the dangling active pointer,
    // else every subsequent `ask` errors on a missing profile.
    run_provider(&cfg_provider(&["remove", "foo"])).unwrap();
    let flat = flat_at(&path);
    assert!(
        !flat.keys().any(|k| k.starts_with("providers.foo.")),
        "profile should be gone"
    );
    assert!(
        !flat.contains_key("llm.active-provider"),
        "active pointer should be cleared"
    );
}

#[test]
#[serial_test::serial]
fn provider_add_rejects_invalid_backend_override() {
    let tmp = tempfile::tempdir().unwrap();
    let _guard = ConfigPathGuard::point_at(&tmp.path().join("config.toml"));
    let err = run_provider(&cfg_provider(&[
        "add",
        "bad",
        "gemini-headless",
        "backend=garbage",
    ]))
    .unwrap_err()
    .to_string();
    assert!(err.contains("invalid backend"), "got: {err}");
}

#[test]
#[serial_test::serial]
fn provider_set_rejects_unknown_profile() {
    let tmp = tempfile::tempdir().unwrap();
    let _guard = ConfigPathGuard::point_at(&tmp.path().join("config.toml"));
    let err = run_provider(&cfg_provider(&["set", "ghost", "model", "x"]))
        .unwrap_err()
        .to_string();
    assert!(err.contains("not found"), "got: {err}");
}
