//! MCP origin / URL-required env tests.
//! Test BODIES unchanged from the previous flat `mod tests` (bead 2j9.6).

#![allow(clippy::needless_pass_by_value)]

use super::*;

#[allow(unsafe_code)]
#[test]
fn into_config_parses_mcp_origin_allowlist_from_env() {
    let _guard = env_guard();
    const MCP: &str = "AXON_ALLOWED_ORIGINS";

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
    let _guard = env_guard();
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
    let _guard = env_guard();
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
fn into_config_reads_openai_compat_env_settings() {
    let _guard = env_guard();
    with_env_saved(
        &[
            "AXON_LLM_BACKEND",
            "AXON_OPENAI_BASE_URL",
            "AXON_OPENAI_API_KEY",
            "AXON_SYNTHESIS_OPENAI_MODEL",
        ],
        || unsafe {
            env::set_var("AXON_LLM_BACKEND", "openai-compat");
            env::set_var("AXON_OPENAI_BASE_URL", "http://127.0.0.1:8080/v1");
            env::set_var("AXON_OPENAI_API_KEY", "local-key");
            env::set_var("AXON_SYNTHESIS_OPENAI_MODEL", "gemma-4-e4b");
            let cfg = into_config_via_args(&["status"]).expect("status config");
            let backend = crate::llm::LlmBackendConfig::from_config(&cfg);
            assert_eq!(backend.kind, crate::llm::LlmBackendKind::OpenAiCompat);
            assert_eq!(
                backend.openai_base_url.as_deref(),
                Some("http://127.0.0.1:8080/v1")
            );
            assert_eq!(backend.openai_api_key.as_deref(), Some("local-key"));
            assert_eq!(backend.openai_model.as_deref(), Some("gemma-4-e4b"));
        },
    );
}

#[allow(unsafe_code)]
#[test]
fn into_config_reads_codex_app_server_env_settings() {
    let _guard = env_guard();
    with_env_saved(
        &[
            "AXON_LLM_BACKEND",
            "AXON_CODEX_CMD",
            "AXON_CODEX_HOME",
            "AXON_SYNTHESIS_CODEX_MODEL",
            "AXON_CODEX_MODEL",
            "AXON_CODEX_COMPLETION_CONCURRENCY",
            "AXON_CODEX_LOAD_USER_CONFIG",
        ],
        || unsafe {
            env::set_var("AXON_LLM_BACKEND", "codex-app-server");
            env::set_var("AXON_CODEX_CMD", "/opt/codex/bin/codex");
            env::set_var("AXON_CODEX_HOME", "/home/example/.codex");
            env::set_var("AXON_CODEX_MODEL", "legacy-model");
            env::set_var("AXON_SYNTHESIS_CODEX_MODEL", "gpt-5.5");
            env::set_var("AXON_CODEX_COMPLETION_CONCURRENCY", "2");
            env::set_var("AXON_CODEX_LOAD_USER_CONFIG", "true");

            let cfg = into_config_via_args(&["status"]).expect("status config");
            let backend = crate::llm::LlmBackendConfig::from_config(&cfg);
            assert!(backend.codex_load_user_config);

            assert_eq!(backend.kind, crate::llm::LlmBackendKind::CodexAppServer);
            assert_eq!(backend.codex_cmd, "/opt/codex/bin/codex");
            assert_eq!(
                backend.codex_home.as_deref(),
                Some(Path::new("/home/example/.codex"))
            );
            assert_eq!(backend.codex_model.as_deref(), Some("gpt-5.5"));
            assert_eq!(backend.completion_concurrency, 2);
        },
    );
}

#[allow(unsafe_code)]
#[test]
fn into_config_reads_split_synthesis_and_chat_models() {
    let _guard = env_guard();
    with_env_saved(
        &[
            "AXON_LLM_BACKEND",
            "AXON_SYNTHESIS_OPENAI_MODEL",
            "AXON_CHAT_OPENAI_MODEL",
            "AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL",
            "AXON_HEADLESS_GEMINI_MODEL",
            "AXON_CHAT_HEADLESS_GEMINI_MODEL",
        ],
        || unsafe {
            env::set_var("AXON_LLM_BACKEND", "openai-compat");
            env::set_var("AXON_SYNTHESIS_OPENAI_MODEL", "explicit-synthesis");
            env::set_var("AXON_CHAT_OPENAI_MODEL", "direct-chat");
            env::set_var("AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL", "gemini-synthesis");
            env::set_var("AXON_CHAT_HEADLESS_GEMINI_MODEL", "gemini-chat");

            let cfg = into_config_via_args(&["status"]).expect("status config");

            assert_eq!(cfg.openai_model, "explicit-synthesis");
            assert_eq!(cfg.openai_chat_model, "direct-chat");
            assert_eq!(cfg.headless_gemini_model, "gemini-synthesis");
            assert_eq!(cfg.headless_gemini_chat_model, "gemini-chat");
        },
    );
}

#[allow(unsafe_code)]
#[test]
fn into_config_rejects_removed_env_aliases_with_guidance() {
    let _guard = env_guard();
    for (removed, replacement) in [
        ("AXON_OPENAI_MODEL", "AXON_SYNTHESIS_OPENAI_MODEL"),
        (
            "AXON_MCP_EMBED_ALLOWED_ROOTS",
            "AXON_SOURCE_LOCAL_ALLOWED_ROOTS",
        ),
        ("AXON_HNSW_EF_SEARCH_LEGACY", "AXON_HNSW_EF_SEARCH"),
    ] {
        with_env_saved(&[removed], || unsafe {
            env::set_var(removed, "removed-value");
            let err = into_config_via_args(&["status"]).expect_err("removed env must fail");
            assert!(err.contains(removed), "missing removed key in: {err}");
            assert!(err.contains(replacement), "missing replacement in: {err}");
        });
    }
}

#[allow(unsafe_code)]
#[test]
fn into_config_reads_canonical_source_local_roots() {
    let _guard = env_guard();
    with_env_saved(&["AXON_SOURCE_LOCAL_ALLOWED_ROOTS"], || unsafe {
        env::set_var(
            "AXON_SOURCE_LOCAL_ALLOWED_ROOTS",
            "/srv/axon/sources, /home/axon/projects",
        );
        let cfg = into_config_via_args(&["status"]).expect("status config");
        assert_eq!(
            cfg.mcp_embed_allowed_roots,
            vec![
                std::path::PathBuf::from("/srv/axon/sources"),
                std::path::PathBuf::from("/home/axon/projects"),
            ]
        );
    });
}

#[allow(unsafe_code)]
#[test]
fn into_config_rejects_unknown_llm_backend() {
    let _guard = env_guard();
    with_env_saved(&["AXON_LLM_BACKEND"], || unsafe {
        env::set_var("AXON_LLM_BACKEND", "llama");
        let err = into_config_via_args(&["status"]).unwrap_err();
        assert!(err.contains("AXON_LLM_BACKEND"));
        assert!(err.contains("openai-compat"));
    });
}

#[allow(unsafe_code)]
#[test]
fn into_config_reads_canonical_llm_backend_from_toml() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    write!(f, "[providers.llm]\nbackend = \"openai-compat\"\n").unwrap();
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_LLM_BACKEND"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_LLM_BACKEND");
        let cfg = into_config_via_args(&["status"]).expect("status config");
        assert_eq!(cfg.llm_backend, crate::llm::LlmBackendKind::OpenAiCompat);
    });
}

#[allow(unsafe_code)]
#[test]
fn into_config_ignores_removed_openai_model_env() {
    let _guard = env_guard();
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
fn into_config_reads_ask_chunk_limit_from_toml() {
    let _guard = env_guard();
    let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
    // The old `[ask].backend = "headless"` compat field was dropped in the
    // 20-section config contract clean break; `[ask].chunk-limit` still parses.
    write!(f, "[ask]\nchunk-limit = 8\n").unwrap();
    with_env_saved(&["AXON_CONFIG_PATH", "AXON_ASK_CHUNK_LIMIT"], || unsafe {
        env::set_var("AXON_CONFIG_PATH", f.path());
        env::remove_var("AXON_ASK_CHUNK_LIMIT");
        let cfg = into_config_via_args(&["status"]).expect("status config");
        assert_eq!(cfg.ask_chunk_limit, 8);
    });
}

#[allow(unsafe_code)]
#[test]
fn into_config_falls_back_to_default_on_unparseable_llm_runtime_env() {
    // AXON_LLM_COMPLETION_CONCURRENCY is a MoveToml tuning knob (config.toml
    // `llm.completion-concurrency`); like its sibling TEI/ask/search knobs, an
    // unparseable env value warns and falls back to TOML/default rather than
    // hard-failing config construction.
    let _guard = env_guard();
    with_env_saved(&["AXON_LLM_COMPLETION_CONCURRENCY"], || unsafe {
        env::set_var("AXON_LLM_COMPLETION_CONCURRENCY", "abc");
        let cfg = into_config_via_args(&["status"]).expect("status config");
        assert_eq!(cfg.llm_completion_concurrency, 4);
    });
}

#[allow(unsafe_code)]
#[test]
fn into_config_errors_when_qdrant_url_missing() {
    let _guard = env_guard();
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
    let _guard = env_guard();
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
