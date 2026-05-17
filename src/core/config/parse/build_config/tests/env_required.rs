//! MCP origin / URL-required env tests.
//! Test BODIES unchanged from the previous flat `mod tests` (bead 2j9.6).

#![allow(clippy::needless_pass_by_value)]

use super::*;

#[allow(unsafe_code)]
#[test]
fn into_config_parses_mcp_origin_allowlist_from_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    const MCP: &str = "AXON_MCP_ALLOWED_ORIGINS";

    unsafe {
        env::set_var(MCP, " https://axon.example.com , http://localhost:49010 ");
    }

    let cli = Cli::parse_from([
        "axon",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "--tei-url",
        "http://127.0.0.1:52000",
        "status",
    ]);
    let cfg = into_config(cli).expect("status config should parse");

    assert_eq!(
        cfg.mcp_allowed_origins,
        vec![
            "https://axon.example.com".to_string(),
            "http://localhost:49010".to_string(),
        ]
    );

    unsafe {
        env::remove_var(MCP);
    }
}

#[test]
fn into_config_normalizes_tei_url_like_other_services() {
    let _guard = ENV_LOCK.lock().unwrap();
    let cli = Cli::parse_from([
        "axon",
        "--qdrant-url",
        "http://127.0.0.1:53333",
        "--tei-url",
        "http://axon-tei:80",
        "status",
    ]);
    let cfg = into_config(cli).expect("status config should parse");
    assert_eq!(
        cfg.tei_url,
        normalize_local_service_url("http://axon-tei:80".to_string())
    );
}

#[allow(unsafe_code)]
#[test]
fn into_config_reads_gemini_env_settings() {
    let _guard = ENV_LOCK.lock().unwrap();
    with_env_saved(
        &[
            "AXON_HEADLESS_GEMINI_MODEL",
            "AXON_HEADLESS_GEMINI_CMD",
            "AXON_HEADLESS_GEMINI_HOME",
            "AXON_LLM_COMPLETION_CONCURRENCY",
            "AXON_LLM_COMPLETION_TIMEOUT_SECS",
        ],
        || unsafe {
            env::set_var("AXON_HEADLESS_GEMINI_MODEL", "gemini-explicit");
            env::set_var("AXON_HEADLESS_GEMINI_CMD", "/usr/local/bin/gemini");
            env::set_var("AXON_HEADLESS_GEMINI_HOME", "/tmp/gemini-home");
            env::set_var("AXON_LLM_COMPLETION_CONCURRENCY", "3");
            env::set_var("AXON_LLM_COMPLETION_TIMEOUT_SECS", "42");
            let cfg = into_config_via_args(&["status"]).expect("status config");
            assert_eq!(cfg.headless_gemini_model, "gemini-explicit");
            assert_eq!(cfg.headless_gemini_cmd, "/usr/local/bin/gemini");
            assert_eq!(
                cfg.headless_gemini_home,
                Some(std::path::PathBuf::from("/tmp/gemini-home"))
            );
            assert_eq!(cfg.llm_completion_concurrency, 3);
            assert_eq!(cfg.llm_completion_timeout_secs, 42);
        },
    );
}

#[allow(unsafe_code)]
#[test]
fn into_config_ignores_removed_openai_model_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    with_env_saved(&["OPENAI_MODEL", "AXON_HEADLESS_GEMINI_MODEL"], || unsafe {
        env::set_var("OPENAI_MODEL", "gemini-legacy");
        env::remove_var("AXON_HEADLESS_GEMINI_MODEL");
        let cfg = into_config_via_args(&["status"]).expect("status config");
        // OPENAI_MODEL is no longer read; only AXON_HEADLESS_GEMINI_MODEL is canonical.
        assert_eq!(cfg.headless_gemini_model, "");
    });
}

#[allow(unsafe_code)]
#[test]
fn into_config_accepts_deprecated_ask_backend_toml() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    write!(f, "[ask]\nbackend = \"headless\"\nchunk-limit = 8\n").unwrap();
    with_env_saved(&["AXON_CONFIG_PATH"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        let cfg = into_config_via_args(&["status"]).expect("status config");
        assert_eq!(cfg.ask_chunk_limit, 8);
    });
}

#[allow(unsafe_code)]
#[test]
fn into_config_rejects_invalid_llm_runtime_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    with_env_saved(&["AXON_LLM_COMPLETION_CONCURRENCY"], || unsafe {
        env::set_var("AXON_LLM_COMPLETION_CONCURRENCY", "abc");
        let err = into_config_via_args(&["status"]).unwrap_err();
        assert!(err.contains("AXON_LLM_COMPLETION_CONCURRENCY"));
    });
}

#[allow(unsafe_code)]
#[test]
fn into_config_errors_when_qdrant_url_missing() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe {
        env::remove_var("QDRANT_URL");
    }

    let cli = Cli::parse_from(["axon", "--tei-url", "http://127.0.0.1:52000", "status"]);
    let err = into_config(cli).unwrap_err();
    assert!(
        err.contains("QDRANT_URL"),
        "expected QDRANT_URL error, got: {err}"
    );
}

#[allow(unsafe_code)]
#[test]
fn into_config_errors_when_tei_url_missing() {
    let _guard = ENV_LOCK.lock().unwrap();
    let orig_tei_url = env::var("TEI_URL").ok();
    unsafe {
        env::remove_var("TEI_URL");
    }

    let cli = Cli::parse_from(["axon", "--qdrant-url", "http://127.0.0.1:53333", "status"]);
    let err = into_config(cli).unwrap_err();
    assert!(
        err.contains("TEI_URL"),
        "expected TEI_URL error, got: {err}"
    );

    unsafe {
        if let Some(val) = orig_tei_url {
            env::set_var("TEI_URL", val);
        }
    }
}
