use super::*;

/// Reuse the crate-wide env-mutation lock instead of a local/private one.
/// `std::env` is process-global — a separate same-named `Mutex` here would
/// provide zero mutual exclusion against `config::parse::build_config::tests`
/// (which also mutates `AXON_CONFIG_PATH`), letting the two modules race each
/// other under the default multi-threaded test runner. See that module's
/// `ENV_LOCK` doc comment for the full explanation.
use crate::config::parse::build_config::tests::env_guard;

struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        // SAFETY: serialized by ENV_LOCK for the duration of each test.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

#[test]
fn tuning_env_var_present_is_flagged() {
    let _lock = env_guard();
    let _guard = EnvGuard::set("AXON_HYBRID_CANDIDATES", "250");
    let findings = tuning_env_vars_present();
    let hit = findings
        .iter()
        .find(|f| f.key == "AXON_HYBRID_CANDIDATES")
        .expect("AXON_HYBRID_CANDIDATES should be flagged as move-toml");
    assert_eq!(hit.check, "tuning_env_should_move_to_toml");
    assert!(hit.remediation.contains("search.hybrid-candidates"));
}

#[test]
fn tuning_env_var_absent_is_silent() {
    let _lock = env_guard();
    unsafe { std::env::remove_var("AXON_HYBRID_CANDIDATES") };
    let findings = tuning_env_vars_present();
    assert!(!findings.iter().any(|f| f.key == "AXON_HYBRID_CANDIDATES"));
}

#[test]
fn compose_only_key_flagged_outside_container() {
    let _lock = env_guard();
    unsafe { std::env::remove_var("AXON_IN_CONTAINER") };
    let _guard = EnvGuard::set("TEI_HTTP_PORT", "52000");
    let findings = compose_only_env_vars_outside_compose();
    assert!(findings.iter().any(|f| f.key == "TEI_HTTP_PORT"));
}

#[test]
fn compose_only_key_silent_inside_container() {
    let _lock = env_guard();
    let _in_container = EnvGuard::set("AXON_IN_CONTAINER", "1");
    let _guard = EnvGuard::set("TEI_HTTP_PORT", "52000");
    let findings = compose_only_env_vars_outside_compose();
    assert!(findings.is_empty());
    unsafe { std::env::remove_var("AXON_IN_CONTAINER") };
}

#[test]
fn deprecated_key_with_replacement_is_flagged() {
    let _lock = env_guard();
    let _guard = EnvGuard::set("CHROME_URL", "http://127.0.0.1:6000");
    let findings = deprecated_env_vars_present();
    let hit = findings
        .iter()
        .find(|f| f.key == "CHROME_URL")
        .expect("CHROME_URL should be flagged as deprecated-with-replacement");
    assert_eq!(hit.check, "deprecated_env_key_with_replacement");
    assert!(hit.message.contains("AXON_CHROME_REMOTE_URL"));
}

#[test]
fn deprecated_stale_key_with_no_replacement_is_flagged() {
    let _lock = env_guard();
    let _guard = EnvGuard::set("AXON_TEST_QDRANT_URL", "http://127.0.0.1:1");
    let findings = deprecated_env_vars_present();
    let hit = findings
        .iter()
        .find(|f| f.key == "AXON_TEST_QDRANT_URL")
        .expect("AXON_TEST_QDRANT_URL should be flagged as deprecated");
    assert_eq!(hit.check, "deprecated_env_key");
}

#[test]
fn deprecated_checks_silent_when_absent() {
    let _lock = env_guard();
    unsafe {
        std::env::remove_var("CHROME_URL");
        std::env::remove_var("AXON_TEST_QDRANT_URL");
    }
    let findings = deprecated_env_vars_present();
    assert!(!findings.iter().any(|f| f.key == "CHROME_URL"));
    assert!(!findings.iter().any(|f| f.key == "AXON_TEST_QDRANT_URL"));
}

#[test]
fn secrets_scan_absent_config_toml_is_silent() {
    let _lock = env_guard();
    let _guard = EnvGuard::set("AXON_CONFIG_PATH", "/nonexistent/path/does-not-exist.toml");
    let findings = secrets_in_config_toml();
    assert!(findings.is_empty());
}

#[test]
fn secrets_scan_flags_secret_shaped_field() {
    let _lock = env_guard();
    let dir = std::env::temp_dir().join(format!(
        "axon-doctor-config-checks-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        "[llm]\napi_key = \"sk-1234567890abcdefghijklmnopqrstuvwxyz\"\n",
    )
    .expect("write temp config.toml");

    let _guard = EnvGuard::set("AXON_CONFIG_PATH", path.to_str().unwrap());
    let findings = secrets_in_config_toml();
    assert!(
        findings.iter().any(|f| f.key.contains("api_key")),
        "expected a secret-shaped finding, got {findings:?}"
    );
    // Never echo the raw secret value anywhere in the diagnostic.
    for f in &findings {
        assert!(!f.message.contains("sk-1234567890"));
        assert!(!f.remediation.contains("sk-1234567890"));
    }

    let _ = std::fs::remove_dir_all(&dir);
}
