use super::*;
use std::io::Write;
use std::sync::Mutex;
use tempfile::NamedTempFile;

// Serializes env-mutating tests to avoid data races on AXON_CONFIG_PATH/HOME.
// Uses the same pattern as helpers.rs and build_config.rs ENV_LOCK.
static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn missing_file_returns_default() {
    let path = Path::new("/nonexistent/path/that/should/not/exist/config.toml");
    let cfg = load_from_path(path, false).unwrap();
    assert!(cfg.search.hybrid_enabled.is_none());
    assert!(cfg.ask.chunk_limit.is_none());
}

#[cfg(unix)]
#[test]
fn load_from_path_rejects_symlinked_config() {
    // Plant a symlink at a config path pointing at a real TOML file.
    // load_from_path must refuse to follow the symlink even though
    // the target parses cleanly — a symlink under ~/.axon/ would let
    // a local attacker redirect [services] URLs / adapter cmds.
    let target = NamedTempFile::new().unwrap();
    writeln!(target.as_file(), "[ask]\nchunk-limit = 5").unwrap();
    let link = std::env::temp_dir().join(format!("axon-symlink-test-{}.toml", std::process::id()));
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(target.path(), &link).expect("create symlink");
    let result = load_from_path(&link, true);
    let _ = std::fs::remove_file(&link);
    let err = match result {
        Ok(_) => panic!("symlinked config must be rejected, got Ok"),
        Err(e) => e,
    };
    assert!(
        err.contains("symlinked config file") || err.contains("symlink attack"),
        "error should mention symlink rejection, got: {err}"
    );
}

#[test]
fn valid_toml_parses_search_section() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(
        f,
        "[search]\nhybrid-enabled = false\nhybrid-candidates = 200"
    )
    .unwrap();
    let cfg = load_from_path(f.path(), false).unwrap();
    assert_eq!(cfg.search.hybrid_enabled, Some(false));
    assert_eq!(cfg.search.hybrid_candidates, Some(200));
}

#[test]
fn valid_toml_parses_ask_section() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(
        f,
        "[ask]\nchunk-limit = 5\ncandidate-limit = 50\nmin-relevance-score = 0.6"
    )
    .unwrap();
    let cfg = load_from_path(f.path(), false).unwrap();
    assert_eq!(cfg.ask.chunk_limit, Some(5));
    assert_eq!(cfg.ask.candidate_limit, Some(50));
    assert!(cfg.ask.min_relevance_score.is_some());
}

#[test]
fn valid_toml_parses_tei_and_workers() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "[tei]\nmax-retries = 3\n[workers]\ningest-lanes = 4").unwrap();
    let cfg = load_from_path(f.path(), false).unwrap();
    assert_eq!(cfg.tei.max_retries, Some(3));
    assert_eq!(cfg.workers.ingest_lanes, Some(4));
}

#[test]
fn malformed_toml_returns_err() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "[search\nbadly_broken = !!!").unwrap();
    let result = load_from_path(f.path(), false);
    assert!(result.is_err(), "malformed TOML should return Err");
    assert!(
        result.err().unwrap().contains("parse error"),
        "error message should mention 'parse error'"
    );
}

#[test]
fn load_from_path_rejects_directory_config_path() {
    let dir = tempfile::tempdir().unwrap();
    let result = load_from_path(dir.path(), false);
    let err = match result {
        Ok(_) => panic!("directory config path should hard-fail"),
        Err(e) => e,
    };
    assert!(
        err.contains("cannot read config file"),
        "error should mention unreadable config, got: {err}"
    );
}

#[test]
fn load_from_path_rejects_not_a_directory_config_path() {
    let file = NamedTempFile::new().unwrap();
    let path = file.path().join("config.toml");
    let result = load_from_path(&path, false);
    let err = match result {
        Ok(_) => panic!("NotADirectory config path should hard-fail"),
        Err(e) => e,
    };
    assert!(
        err.contains("cannot read config file"),
        "error should mention unreadable config, got: {err}"
    );
}

#[test]
fn unknown_field_fails_parse() {
    let result = load_toml_config_from_str("[search]\nunknown-key = true");
    assert!(
        result.is_err(),
        "deny_unknown_fields should reject unknown keys"
    );
}

#[test]
fn empty_file_returns_default() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f).unwrap();
    let cfg = load_from_path(f.path(), false).unwrap();
    assert!(cfg.search.hybrid_enabled.is_none());
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_config_path_env_var_overrides_home() {
    let _guard = ENV_LOCK.lock().unwrap();
    let saved = std::env::var("AXON_CONFIG_PATH").ok();
    unsafe { std::env::set_var("AXON_CONFIG_PATH", "/tmp/custom_axon_config.toml") };
    let path = resolve_config_path();
    // Unconditionally restore so a panic can't contaminate other tests.
    match saved {
        Some(v) => unsafe { std::env::set_var("AXON_CONFIG_PATH", v) },
        None => unsafe { std::env::remove_var("AXON_CONFIG_PATH") },
    }
    assert_eq!(
        path.unwrap()
            .map(|resolved| (resolved.path, resolved.explicit)),
        Some((PathBuf::from("/tmp/custom_axon_config.toml"), true))
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_config_path_non_toml_extension_returns_err() {
    let _guard = ENV_LOCK.lock().unwrap();
    let saved = std::env::var("AXON_CONFIG_PATH").ok();
    unsafe { std::env::set_var("AXON_CONFIG_PATH", "/etc/passwd") };
    let result = resolve_config_path();
    match saved {
        Some(v) => unsafe { std::env::set_var("AXON_CONFIG_PATH", v) },
        None => unsafe { std::env::remove_var("AXON_CONFIG_PATH") },
    }
    assert!(
        result.is_err(),
        "non-.toml AXON_CONFIG_PATH should return Err"
    );
    assert!(
        result.err().unwrap().contains("AXON_CONFIG_PATH"),
        "error should mention AXON_CONFIG_PATH"
    );
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn explicit_missing_config_path_returns_err() {
    let _guard = ENV_LOCK.lock().unwrap();
    let saved = std::env::var("AXON_CONFIG_PATH").ok();
    unsafe { std::env::set_var("AXON_CONFIG_PATH", "/tmp/axon-missing-config.toml") };
    let result = load_toml_config();
    match saved {
        Some(v) => unsafe { std::env::set_var("AXON_CONFIG_PATH", v) },
        None => unsafe { std::env::remove_var("AXON_CONFIG_PATH") },
    }
    assert!(
        result.is_err(),
        "explicit missing AXON_CONFIG_PATH should hard-fail"
    );
    assert!(
        result.err().unwrap().contains("cannot read config file"),
        "error should explain the config path read failure"
    );
}

// ── New schema field tests ────────────────────────────────────────────────────

#[test]
fn services_url_fields_parse_without_error() {
    // [services] URL fields should be accepted by the parser (parse-warn-and-ignore
    // semantics) even though they do not affect service URLs at runtime.
    let result = load_toml_config_from_str(
        r#"
[services]
qdrant-url = "http://custom-qdrant:6333"
tei-url = "http://custom-tei:80"
chrome-remote-url = "http://custom-chrome:6000"
"#,
    );
    assert!(
        result.is_ok(),
        "services URL fields should parse without error: {:?}",
        result.err()
    );
    let cfg = result.unwrap();
    assert_eq!(
        cfg.services.qdrant_url.as_deref(),
        Some("http://custom-qdrant:6333")
    );
    assert_eq!(
        cfg.services.tei_url.as_deref(),
        Some("http://custom-tei:80")
    );
    assert_eq!(
        cfg.services.chrome_remote_url.as_deref(),
        Some("http://custom-chrome:6000")
    );
}

#[test]
fn workers_job_wait_timeout_secs_parses() {
    let result = load_toml_config_from_str("[workers]\njob-wait-timeout-secs = 600");
    assert!(
        result.is_ok(),
        "job-wait-timeout-secs should parse: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap().workers.job_wait_timeout_secs, Some(600));
}

#[test]
fn chrome_user_agent_parses() {
    let result = load_toml_config_from_str(
        r#"[chrome]
user-agent = "Mozilla/5.0 (Axon Test)""#,
    );
    assert!(
        result.is_ok(),
        "chrome user-agent should parse: {:?}",
        result.err()
    );
    assert_eq!(
        result.unwrap().chrome.user_agent.as_deref(),
        Some("Mozilla/5.0 (Axon Test)")
    );
}

#[test]
fn logging_max_bytes_parses() {
    let result = load_toml_config_from_str("[logging]\nmax-bytes = 5242880");
    assert!(
        result.is_ok(),
        "logging max-bytes should parse: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap().logging.max_bytes, Some(5_242_880));
}

#[test]
fn unknown_workers_field_fails_parse() {
    // deny_unknown_fields on TomlWorkersSection must reject typos
    let result = load_toml_config_from_str("[workers]\nmax-pending-embed-job = 10");
    assert!(
        result.is_err(),
        "unknown [workers] field should be rejected by deny_unknown_fields"
    );
}
