use super::*;
use crate::config::parse::build_config::tests::{env_guard, with_env_saved};
use std::io::Write;
use tempfile::NamedTempFile;

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
    // a local attacker redirect adapter cmds via config.toml.
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
        "[providers.vector]\nhybrid-enabled = false\n[retrieval]\nhybrid-candidates = 200"
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
fn valid_toml_parses_canonical_llm_backend() {
    let cfg = load_toml_config_from_str("[providers.llm]\nbackend = \"codex-app-server\"")
        .expect("canonical backend should parse");
    assert_eq!(cfg.llm.backend.as_deref(), Some("codex-app-server"));
}

#[test]
fn valid_toml_parses_tei_and_workers() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(
        f,
        "[providers.embedding]\nmax-retries = 3\n[pipeline]\ningest-lanes = 4"
    )
    .unwrap();
    let cfg = load_from_path(f.path(), false).unwrap();
    assert_eq!(cfg.tei.max_retries, Some(3));
    assert_eq!(cfg.workers.ingest_lanes, Some(4));
}

#[test]
fn valid_toml_parses_embed_openai_compat_section() {
    let cfg = load_toml_config_from_str(
        r#"
[providers.embedding]
openai-model = "qwen3-openai"
openai-max-client-batch-size = 24
openai-max-concurrent = 12
openai-max-in-flight-inputs = 256
openai-pool-max-inputs = 768
"#,
    )
    .expect("openai-compatible embed fields should parse");

    assert_eq!(cfg.embed.openai_model.as_deref(), Some("qwen3-openai"));
    assert_eq!(cfg.embed.openai_max_client_batch_size, Some(24));
    assert_eq!(cfg.embed.openai_max_concurrent, Some(12));
    assert_eq!(cfg.embed.openai_max_in_flight_inputs, Some(256));
    assert_eq!(cfg.embed.openai_pool_max_inputs, Some(768));
}

#[test]
fn valid_toml_parses_chrome_bootstrap_section() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(
        f,
        "[providers.render]\nbootstrap-timeout-ms = 1500\nbootstrap-retries = 4"
    )
    .unwrap();
    let cfg = load_from_path(f.path(), false).unwrap();
    assert_eq!(cfg.chrome.bootstrap_timeout_ms, Some(1500));
    assert_eq!(cfg.chrome.bootstrap_retries, Some(4));
}

#[test]
fn extended_toml_tuning_sections_parse() {
    let raw = r#"
[providers.embedding]
max-concurrent-requests = 8
max-in-flight-inputs = 512
pool-max-inputs = 1024
prep-concurrency = 12
max-chunks-per-doc = 0
max-source-chunks-per-doc = 0
dedupe-exact-chunks = true
openai-max-client-batch-size = 32
openai-max-concurrent = 32
openai-max-in-flight-inputs = 512
openai-pool-max-inputs = 1024

[pipeline.chunking]
markdown-min-chars = 500
markdown-max-chars = 2000
overlap-chars = 200

[providers.vector]
upsert-batch-points = 1024
write-concurrency = 1
bulk-load = false
bulk-indexing-threshold-kb = 10485760
indexing-threshold-kb = 20000
hnsw-m = 32
hnsw-ef-construct = 256
payload-index-profile = "full"
payload-index-parallelism = 16
hnsw-on-disk = false
quantization-always-ram = true

[sources.code-search]
freshness-ttl-secs = 30
reindex-timeout-secs = 15
max-file-bytes = 10485760
changed-file-batch-size = 50

[watch]
tick-secs = 15
lease-secs = 300

[pipeline.endpoints]
bundle-concurrency = 8
chrome-concurrency = 2
verify-concurrency = 16
probe-concurrency = 16

[server.mcp]
task-result-wait-timeout-secs = 300

[server.mcp.embed]
max-local-bytes = 10485760
max-local-depth = 16
max-local-entries = 10000
"#;

    load_toml_config_from_str(raw).unwrap();
}

#[test]
fn root_config_example_parses() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root");
    let raw = std::fs::read_to_string(repo_root.join("config.example.toml"))
        .expect("read root config.example.toml");
    load_toml_config_from_str(&raw).unwrap();
}

/// Resolves a path under `crates/axon-core/tests/fixtures/`, the config
/// schema contract's fixture root (see
/// docs/pipeline-unification/schemas/config-schema.md's "Validation
/// Fixtures").
fn config_schema_fixture(relative: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative)
}

#[test]
fn config_fixture_minimal_valid_parses() {
    let raw = std::fs::read_to_string(config_schema_fixture("config/minimal.valid.toml"))
        .expect("read minimal.valid.toml fixture");
    load_toml_config_from_str(&raw).expect("minimal.valid.toml fixture should parse");
}

#[test]
fn config_fixture_full_valid_parses() {
    let raw = std::fs::read_to_string(config_schema_fixture("config/full.valid.toml"))
        .expect("read full.valid.toml fixture");
    load_toml_config_from_str(&raw).expect("full.valid.toml fixture should parse");
}

#[test]
fn config_fixture_unknown_key_invalid_fails() {
    let raw = std::fs::read_to_string(config_schema_fixture("config/unknown-key.invalid.toml"))
        .expect("read unknown-key.invalid.toml fixture");
    let result = load_toml_config_from_str(&raw);
    assert!(
        result.is_err(),
        "unknown-key.invalid.toml fixture should fail to parse"
    );
    let err = result.err().unwrap();
    assert!(
        err.contains("parse error"),
        "expected a parse error mentioning the unknown key, got: {err}"
    );
}

#[test]
fn valid_toml_parses_build_section() {
    let cfg = load_toml_config_from_str("[server]\nallow-fallback-web-assets = true\n")
        .expect("server allow-fallback-web-assets should parse");

    assert_eq!(cfg.build.allow_fallback_web_assets, Some(true));
}

#[test]
fn malformed_toml_returns_err() {
    let mut f = NamedTempFile::new().unwrap();
    writeln!(f, "[ask\nbadly_broken = !!!").unwrap();
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
    let result = load_toml_config_from_str("[providers.vector]\nunknown-key = true");
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
    let _guard = env_guard();
    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        std::env::set_var("AXON_CONFIG_PATH", "/tmp/custom_axon_config.toml");
        let path = resolve_config_path();

        assert_eq!(
            path.unwrap()
                .map(|resolved| (resolved.path, resolved.explicit)),
            Some((PathBuf::from("/tmp/custom_axon_config.toml"), true))
        );
    });
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn axon_config_path_non_toml_extension_returns_err() {
    let _guard = env_guard();
    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        std::env::set_var("AXON_CONFIG_PATH", "/etc/passwd");
        let result = resolve_config_path();

        assert!(
            result.is_err(),
            "non-.toml AXON_CONFIG_PATH should return Err"
        );
        assert!(
            result.err().unwrap().contains("AXON_CONFIG_PATH"),
            "error should mention AXON_CONFIG_PATH"
        );
    });
}

#[allow(unsafe_code)]
#[serial_test::serial]
#[test]
fn explicit_missing_config_path_returns_err() {
    let _guard = env_guard();
    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        std::env::set_var("AXON_CONFIG_PATH", "/tmp/axon-missing-config.toml");
        let result = load_toml_config();

        assert!(
            result.is_err(),
            "explicit missing AXON_CONFIG_PATH should hard-fail"
        );
        assert!(
            result.err().unwrap().contains("cannot read config file"),
            "error should explain the config path read failure"
        );
    });
}

// ── New schema field tests ────────────────────────────────────────────────────

#[test]
fn deprecated_services_section_is_rejected_with_new_home() {
    // [services] was the pre-contract compatibility shim for TOML-level
    // service URL overrides. The 20-section contract drops it entirely —
    // URLs live only in .env now — so it must hard-fail with a diagnostic
    // naming the replacement instead of silently parsing.
    let result = load_toml_config_from_str(
        r#"
[services]
qdrant-url = "http://custom-qdrant:6333"
tei-url = "http://custom-tei:80"
chrome-remote-url = "http://custom-chrome:6000"
"#,
    );
    let err = match result {
        Ok(_) => panic!("[services] should be rejected as a deprecated section"),
        Err(e) => e,
    };
    assert!(
        err.contains("services") && err.contains(".env"),
        "error should name [services] and point at .env, got: {err}"
    );
}

#[test]
fn removed_ask_backend_is_rejected() {
    let err = load_toml_config_from_str("[ask]\nbackend = \"headless\"")
        .err()
        .expect("removed [ask].backend must fail");
    assert!(err.contains("backend"), "unexpected error: {err}");
    assert!(err.contains("providers.llm") || err.contains("AXON_LLM_BACKEND"));
}

#[test]
fn removed_vector_legacy_hnsw_is_rejected() {
    let err = load_toml_config_from_str("[providers.vector]\nhnsw-ef-legacy = 64")
        .err()
        .expect("removed hnsw-ef-legacy must fail");
    assert!(err.contains("hnsw-ef-legacy"), "unexpected error: {err}");
    assert!(err.contains("hnsw-ef"), "missing canonical key: {err}");
}

#[test]
fn deprecated_top_level_sections_are_rejected() {
    for (old, needle) in [
        ("[llm]\ncompletion-concurrency = 4", "providers.llm"),
        ("[tei]\nmax-retries = 3", "providers.embedding"),
        ("[scrape]\nrespect-robots = true", "crawl"),
        ("[workers]\ningest-lanes = 2", "pipeline"),
        ("[chrome]\nbypass-csp = true", "providers.render"),
    ] {
        let err = match load_toml_config_from_str(old) {
            Ok(_) => panic!("deprecated section should be rejected: {old}"),
            Err(e) => e,
        };
        assert!(
            err.contains(needle),
            "error for {old:?} should mention {needle}, got: {err}"
        );
    }
}

#[test]
fn workers_job_wait_timeout_secs_parses() {
    let result = load_toml_config_from_str("[pipeline]\njob-wait-timeout-secs = 600");
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
        r#"[providers.render]
user-agent = "Mozilla/5.0 (Axon Test)""#,
    );
    assert!(
        result.is_ok(),
        "render user-agent should parse: {:?}",
        result.err()
    );
    assert_eq!(
        result.unwrap().chrome.user_agent.as_deref(),
        Some("Mozilla/5.0 (Axon Test)")
    );
}

#[test]
fn chrome_bootstrap_knobs_parse() {
    let result = load_toml_config_from_str(
        r#"[providers.render]
bootstrap-timeout-ms = 750
bootstrap-retries = 4"#,
    );
    assert!(
        result.is_ok(),
        "render bootstrap knobs should parse: {:?}",
        result.err()
    );
    let chrome = result.unwrap().chrome;
    assert_eq!(chrome.bootstrap_timeout_ms, Some(750));
    assert_eq!(chrome.bootstrap_retries, Some(4));
}

#[test]
fn live_tuning_sections_parse_without_weakening_unknown_field_rejection() {
    let result = load_toml_config_from_str(
        r#"
[providers.embedding]
max-concurrent-requests = 8
max-in-flight-inputs = 512
pool-max-inputs = 1024
prep-concurrency = 12
max-chunks-per-doc = 0
max-source-chunks-per-doc = 0
dedupe-exact-chunks = true

[pipeline.chunking]
markdown-min-chars = 500
markdown-max-chars = 2000
overlap-chars = 200

[providers.vector]
upsert-batch-points = 1024
write-concurrency = 1
bulk-load = false
bulk-indexing-threshold-kb = 10485760
indexing-threshold-kb = 20000
hnsw-m = 32
hnsw-ef-construct = 256
payload-index-profile = "full"
payload-index-parallelism = 16
hnsw-on-disk = false
quantization-always-ram = true

[sources.code-search]
freshness-ttl-secs = 30
reindex-timeout-secs = 300
max-file-bytes = 10485760
changed-file-batch-size = 64

[watch]
tick-secs = 15
lease-secs = 300

[pipeline.endpoints]
bundle-concurrency = 8
chrome-concurrency = 1
verify-concurrency = 16
probe-concurrency = 4

[server.mcp]
task-result-wait-timeout-secs = 300

[server.mcp.embed]
max-local-bytes = 10485760
max-local-depth = 16
max-local-entries = 10000
"#,
    );
    assert!(
        result.is_ok(),
        "durable live tuning sections should parse: {:?}",
        result.err()
    );
}

#[test]
fn unknown_logging_section_is_rejected() {
    // [logging] was never part of the contract — deny_unknown_fields must reject it.
    let result = load_toml_config_from_str("[logging]\nmax-bytes = 5242880");
    assert!(
        result.is_err(),
        "[logging] section should be rejected since log rotation is env-only"
    );
}

#[test]
fn unknown_workers_field_fails_parse() {
    // deny_unknown_fields on RawPipelineSection must reject typos
    let result = load_toml_config_from_str("[pipeline]\nmax-pending-embed-job = 10");
    assert!(
        result.is_err(),
        "unknown [pipeline] field should be rejected by deny_unknown_fields"
    );
}
