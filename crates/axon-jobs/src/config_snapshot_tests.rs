use crate::config_snapshot::{
    apply_config_snapshot, apply_config_snapshot_for_container, config_snapshot_json,
};
use axon_core::config::Config;
use axon_core::config::RenderMode;
use std::path::PathBuf;

#[test]
fn config_snapshot_applies_submitted_non_secret_values() {
    let mut submitted = Config::test_default();
    submitted.collection = "submitted_collection".to_string();
    submitted.output_dir = PathBuf::from("/tmp/axon-submitted");
    submitted.render_mode = RenderMode::Chrome;
    submitted.max_pages = 37;
    submitted.max_depth = 4;
    submitted.embed = false;
    submitted.query = Some("submitted prompt".to_string());
    submitted.request_timeout_ms = Some(12_345);
    submitted.fetch_retries = 7;
    submitted.qdrant_url = "http://submitted-qdrant:6333".to_string();
    submitted.tei_url = "http://submitted-tei:80".to_string();
    submitted.llm_backend = axon_core::llm::LlmBackendKind::OpenAiCompat;
    submitted.openai_base_url = "http://submitted-openai:8080/v1".to_string();
    submitted.openai_api_key = "submitted-openai-secret".to_string();
    submitted.openai_model = "submitted-gemma".to_string();
    submitted.headless_gemini_model = "gemini-submitted".to_string();
    submitted.headless_gemini_cmd = "/opt/submitted/gemini".to_string();
    submitted.headless_gemini_home = Some(PathBuf::from("/tmp/submitted-gemini-home"));
    submitted.llm_completion_concurrency = 2;
    submitted.llm_completion_timeout_secs = 17;
    submitted.chrome_proxy = Some("http://submitted-proxy:8080".to_string());
    submitted.custom_headers = vec![
        "Authorization: Bearer submitted".to_string(),
        "X-Crawl-Variant: submitted".to_string(),
    ];
    submitted.discover_llms_txt = false;
    submitted.max_llms_txt_urls = 77;
    submitted.adaptive_concurrency.enabled = true;
    submitted.adaptive_concurrency.min = 2;
    submitted.adaptive_concurrency.max = Some(32);
    submitted.chrome_remote_local_policy = true;
    submitted.tei_max_retries = 11;
    submitted.tei_request_timeout_ms = 45_678;
    submitted.tei_max_client_batch_size = 73;
    submitted.embed_tei_max_concurrent = 13;
    submitted.embed_tei_max_in_flight_inputs = 777;
    submitted.embed_tei_retry_backoff_ms = 321;
    submitted.embed_tei_cooldown_after_failures = 7;
    submitted.embed_tei_cooldown_secs = 91;
    submitted.embed_tei_interactive_reserved_requests = 3;
    submitted.embed_tei_background_max_concurrent_requests = 5;
    submitted.embed_tei_maintenance_max_concurrent_requests = 2;
    submitted.embed_tei_query_instruction_enabled = false;
    submitted.embed_cache_enabled = true;
    submitted.embed_cache_max_entries = 54_321;
    submitted.embed_pool_max_inputs = 1_234;
    submitted.document_batch_size = 23;
    submitted.document_status_batch_size = 321;
    submitted.embed_tei_max_batch_tokens = 77_777;
    submitted.embed_scheduler_enabled = false;
    submitted.vector_upsert_embed_overlap = false;
    submitted.embed_prepared_byte_budget = 33_554_432;
    submitted.embed_prep_concurrency = 7;
    submitted.embed_prep_max_in_flight_bytes = 22_020_096;
    submitted.embed_scheduler_flush_ms = 987;
    submitted.chunking_markdown_max_chars = 3_333;
    submitted.chunking_markdown_min_chars = 777;
    submitted.chunking_overlap_chars = 123;
    submitted.embed_max_chunks_per_doc = Some(37);
    submitted.embed_max_source_chunks_per_doc = Some(41);
    submitted.embed_dedupe_exact_chunks = false;
    submitted.openai_embed_model = "submitted-embed".into();
    submitted.openai_embed_max_client_batch_size = 17;
    submitted.openai_embed_max_concurrent = 9;
    submitted.openai_embed_max_in_flight_inputs = 99;
    submitted.openai_embed_pool_max_inputs = 333;
    submitted.unified_worker_concurrency = 11;
    submitted.source_job_concurrency_limit = 6;
    submitted.embed_doc_timeout_secs = 444;

    let mut worker = Config::test_default();
    worker.collection = "worker_collection".to_string();
    worker.output_dir = PathBuf::from("/tmp/axon-worker");
    worker.render_mode = RenderMode::Http;
    worker.max_pages = 1;
    worker.max_depth = 1;
    worker.embed = true;
    worker.query = Some("worker prompt".to_string());
    worker.request_timeout_ms = Some(999);
    worker.fetch_retries = 1;
    worker.qdrant_url = "http://worker-qdrant:6333".to_string();
    worker.tei_url = "http://worker-tei:80".to_string();
    worker.llm_backend = axon_core::llm::LlmBackendKind::GeminiHeadless;
    worker.openai_base_url = "http://worker-openai:8080/v1".to_string();
    worker.openai_api_key = "worker-openai-secret".to_string();
    worker.openai_model = "worker-gemma".to_string();
    worker.headless_gemini_model = "gemini-worker".to_string();
    worker.headless_gemini_cmd = "/opt/worker/gemini".to_string();
    worker.headless_gemini_home = Some(PathBuf::from("/tmp/worker-gemini-home"));
    worker.llm_completion_concurrency = 8;
    worker.llm_completion_timeout_secs = 99;
    worker.chrome_proxy = Some("http://worker-proxy:8080".to_string());
    worker.custom_headers = vec!["Authorization: Bearer worker".to_string()];
    worker.discover_llms_txt = true;
    worker.max_llms_txt_urls = 512;
    worker.adaptive_concurrency.enabled = false;
    worker.adaptive_concurrency.min = 1;
    worker.adaptive_concurrency.max = None;
    worker.chrome_remote_local_policy = false;

    let config_json = match config_snapshot_json(&submitted) {
        Ok(json) => json,
        Err(err) => panic!("snapshot should encode: {err}"),
    };
    let effective = match apply_config_snapshot(&worker, &config_json) {
        Ok(cfg) => cfg,
        Err(err) => panic!("snapshot should apply: {err}"),
    };

    assert_eq!(effective.collection, "submitted_collection");
    assert_eq!(effective.output_dir, PathBuf::from("/tmp/axon-submitted"));
    assert_eq!(effective.render_mode, RenderMode::Chrome);
    assert_eq!(effective.max_pages, 37);
    assert_eq!(effective.max_depth, 4);
    assert!(!effective.embed);
    assert_eq!(effective.query.as_deref(), Some("submitted prompt"));
    assert_eq!(effective.request_timeout_ms, Some(12_345));
    assert_eq!(effective.fetch_retries, 7);
    assert_eq!(effective.qdrant_url, "http://submitted-qdrant:6333");
    assert_eq!(effective.tei_url, "http://submitted-tei:80");
    assert_eq!(
        effective.llm_backend,
        axon_core::llm::LlmBackendKind::OpenAiCompat
    );
    assert_eq!(effective.openai_base_url, "http://submitted-openai:8080/v1");
    assert_eq!(effective.openai_api_key, "worker-openai-secret");
    assert_eq!(effective.openai_model, "submitted-gemma");
    assert_eq!(effective.headless_gemini_model, "gemini-submitted");
    assert_eq!(effective.headless_gemini_cmd, "/opt/submitted/gemini");
    assert_eq!(
        effective.headless_gemini_home,
        Some(PathBuf::from("/tmp/submitted-gemini-home"))
    );
    assert_eq!(effective.llm_completion_concurrency, 2);
    assert_eq!(effective.llm_completion_timeout_secs, 17);
    assert_eq!(
        effective.chrome_proxy.as_deref(),
        Some("http://submitted-proxy:8080")
    );
    assert_eq!(
        effective.custom_headers,
        vec![
            "Authorization: Bearer worker".to_string(),
            "X-Crawl-Variant: submitted".to_string(),
        ]
    );
    // llms.txt overrides must survive the enqueue→worker snapshot round-trip,
    // matching the sitemap-discovery parity (async crawl is the common override path).
    assert!(!effective.discover_llms_txt);
    assert_eq!(effective.max_llms_txt_urls, 77);
    assert!(effective.adaptive_concurrency.enabled);
    assert_eq!(effective.adaptive_concurrency.min, 2);
    assert_eq!(effective.adaptive_concurrency.max, Some(32));
    assert!(effective.chrome_remote_local_policy);
    assert_eq!(effective.tei_max_retries, submitted.tei_max_retries);
    assert_eq!(
        effective.tei_request_timeout_ms,
        submitted.tei_request_timeout_ms
    );
    assert_eq!(
        effective.tei_max_client_batch_size,
        submitted.tei_max_client_batch_size
    );
    assert_eq!(
        effective.embed_tei_max_concurrent,
        submitted.embed_tei_max_concurrent
    );
    assert_eq!(
        effective.embed_tei_max_in_flight_inputs,
        submitted.embed_tei_max_in_flight_inputs
    );
    assert_eq!(
        effective.embed_tei_retry_backoff_ms,
        submitted.embed_tei_retry_backoff_ms
    );
    assert_eq!(
        effective.embed_tei_cooldown_after_failures,
        submitted.embed_tei_cooldown_after_failures
    );
    assert_eq!(
        effective.embed_tei_cooldown_secs,
        submitted.embed_tei_cooldown_secs
    );
    assert_eq!(
        effective.embed_tei_interactive_reserved_requests,
        submitted.embed_tei_interactive_reserved_requests
    );
    assert_eq!(
        effective.embed_tei_background_max_concurrent_requests,
        submitted.embed_tei_background_max_concurrent_requests
    );
    assert_eq!(
        effective.embed_tei_maintenance_max_concurrent_requests,
        submitted.embed_tei_maintenance_max_concurrent_requests
    );
    assert_eq!(
        effective.embed_tei_query_instruction_enabled,
        submitted.embed_tei_query_instruction_enabled
    );
    assert_eq!(effective.embed_cache_enabled, submitted.embed_cache_enabled);
    assert_eq!(
        effective.embed_cache_max_entries,
        submitted.embed_cache_max_entries
    );
    assert_eq!(
        effective.embed_pool_max_inputs,
        submitted.embed_pool_max_inputs
    );
    assert_eq!(effective.document_batch_size, submitted.document_batch_size);
    assert_eq!(
        effective.document_status_batch_size,
        submitted.document_status_batch_size
    );
    assert_eq!(
        effective.embed_tei_max_batch_tokens,
        submitted.embed_tei_max_batch_tokens
    );
    assert_eq!(
        effective.embed_scheduler_enabled,
        submitted.embed_scheduler_enabled
    );
    assert_eq!(
        effective.vector_upsert_embed_overlap,
        submitted.vector_upsert_embed_overlap
    );
    assert_eq!(
        effective.embed_prepared_byte_budget,
        submitted.embed_prepared_byte_budget
    );
    assert_eq!(
        effective.embed_prep_concurrency,
        submitted.embed_prep_concurrency
    );
    assert_eq!(
        effective.embed_prep_max_in_flight_bytes,
        submitted.embed_prep_max_in_flight_bytes
    );
    assert_eq!(
        effective.embed_scheduler_flush_ms,
        submitted.embed_scheduler_flush_ms
    );
    assert_eq!(
        effective.chunking_markdown_max_chars,
        submitted.chunking_markdown_max_chars
    );
    assert_eq!(
        effective.chunking_markdown_min_chars,
        submitted.chunking_markdown_min_chars
    );
    assert_eq!(
        effective.chunking_overlap_chars,
        submitted.chunking_overlap_chars
    );
    assert_eq!(
        effective.embed_max_chunks_per_doc,
        submitted.embed_max_chunks_per_doc
    );
    assert_eq!(
        effective.embed_max_source_chunks_per_doc,
        submitted.embed_max_source_chunks_per_doc
    );
    assert_eq!(
        effective.embed_dedupe_exact_chunks,
        submitted.embed_dedupe_exact_chunks
    );
    assert_eq!(effective.openai_embed_model, submitted.openai_embed_model);
    assert_eq!(
        effective.openai_embed_max_client_batch_size,
        submitted.openai_embed_max_client_batch_size
    );
    assert_eq!(
        effective.openai_embed_max_concurrent,
        submitted.openai_embed_max_concurrent
    );
    assert_eq!(
        effective.openai_embed_max_in_flight_inputs,
        submitted.openai_embed_max_in_flight_inputs
    );
    assert_eq!(
        effective.openai_embed_pool_max_inputs,
        submitted.openai_embed_pool_max_inputs
    );
    assert_eq!(
        effective.unified_worker_concurrency,
        submitted.unified_worker_concurrency
    );
    assert_eq!(
        effective.source_job_concurrency_limit,
        submitted.source_job_concurrency_limit
    );
    assert_eq!(
        effective.embed_doc_timeout_secs,
        submitted.embed_doc_timeout_secs
    );
}

#[test]
fn config_snapshot_omits_secrets() {
    let mut cfg = Config::test_default();
    cfg.tavily_api_key = "tvly-SECRET_TAVILY".to_string();
    cfg.github_token = Some("ghp_SECRET_GITHUB".to_string());
    cfg.reddit_client_secret = Some("REDDIT_SECRET".to_string());
    cfg.openai_api_key = "OPENAI_COMPAT_SECRET".to_string();

    let snapshot = config_snapshot_json(&cfg).expect("snapshot should encode");

    assert!(
        !snapshot.contains("tvly-SECRET_TAVILY"),
        "snapshot must not contain tavily_api_key"
    );
    assert!(
        !snapshot.contains("ghp_SECRET_GITHUB"),
        "snapshot must not contain github_token"
    );
    assert!(
        !snapshot.contains("REDDIT_SECRET"),
        "snapshot must not contain reddit_client_secret"
    );
    assert!(
        !snapshot.contains("OPENAI_COMPAT_SECRET"),
        "snapshot must not contain openai_api_key"
    );
}

#[test]
fn config_snapshot_omits_mixed_case_credential_headers() {
    let mut cfg = Config::test_default();
    cfg.custom_headers = vec![
        "aUtHoRiZaTiOn: Bearer secret-a".into(),
        "X-API-Key: secret-b".into(),
        "Cookie: session=secret-c".into(),
        "X-Safe-Metadata: retained".into(),
    ];
    let snapshot = config_snapshot_json(&cfg).unwrap();
    assert!(!snapshot.contains("secret-a"));
    assert!(!snapshot.contains("secret-b"));
    assert!(!snapshot.contains("secret-c"));
    assert!(snapshot.contains("X-Safe-Metadata"));
}

#[test]
fn config_snapshot_preserves_codex_llm_backend_fields() {
    let worker = Config {
        codex_cmd: "/usr/local/bin/codex".to_string(),
        codex_home: Some(PathBuf::from("/home/worker/.codex")),
        ..Config::default()
    };
    let cfg = Config {
        llm_backend: axon_core::llm::LlmBackendKind::CodexAppServer,
        codex_cmd: "/opt/codex/bin/codex".to_string(),
        codex_home: Some(PathBuf::from("/home/example/.codex")),
        codex_model: "gpt-5.5".to_string(),
        codex_completion_concurrency: 2,
        codex_load_user_config: true,
        ..Config::default()
    };

    let json = config_snapshot_json(&cfg).expect("snapshot json");
    assert!(
        !json.contains("/home/example/.codex"),
        "submitter-local codex_home must not be serialized"
    );
    assert!(
        !json.contains("/opt/codex/bin/codex"),
        "submitter-local codex_cmd must not be serialized"
    );
    let restored = apply_config_snapshot(&worker, &json).expect("apply snapshot");

    assert_eq!(
        restored.llm_backend,
        axon_core::llm::LlmBackendKind::CodexAppServer
    );
    assert_eq!(restored.codex_cmd, "/usr/local/bin/codex");
    assert_eq!(
        restored.codex_home,
        Some(PathBuf::from("/home/worker/.codex"))
    );
    assert_eq!(restored.codex_model, "gpt-5.5");
    assert_eq!(restored.codex_completion_concurrency, 2);
    assert!(restored.codex_load_user_config);
}

#[test]
fn config_snapshot_maps_default_output_dir_when_container_env_is_set() {
    let mut submitted = Config::test_default();
    submitted.output_dir = PathBuf::from("/home/jmagar/.axon/output");
    let mut worker = Config::test_default();
    worker.output_dir = PathBuf::from("/home/axon/.axon/output");

    let config_json = config_snapshot_json(&submitted).expect("encode snapshot");
    let effective =
        apply_config_snapshot_for_container(&worker, &config_json, true).expect("apply snapshot");

    assert_eq!(
        effective.output_dir,
        PathBuf::from("/home/axon/.axon/output")
    );
}

#[test]
fn config_snapshot_keeps_default_output_dir_when_container_env_is_unset() {
    let mut submitted = Config::test_default();
    submitted.output_dir = PathBuf::from("/home/jmagar/.axon/output");
    let mut worker = Config::test_default();
    worker.output_dir = PathBuf::from("/home/axon/.axon/output");

    let config_json = config_snapshot_json(&submitted).expect("encode snapshot");
    let effective =
        apply_config_snapshot_for_container(&worker, &config_json, false).expect("apply snapshot");

    assert_eq!(
        effective.output_dir,
        PathBuf::from("/home/jmagar/.axon/output")
    );
}

#[test]
fn config_snapshot_exactly_replays_submitted_none_options() {
    let mut submitted = Config::test_default();
    submitted.output_path = None;
    submitted.request_timeout_ms = None;
    submitted.chrome_wait_for_selector = None;
    let mut worker = Config::test_default();
    worker.output_path = Some(PathBuf::from("/tmp/worker-output.md"));
    worker.request_timeout_ms = Some(999);
    worker.chrome_wait_for_selector = Some("#app".to_string());
    let config_json = config_snapshot_json(&submitted).expect("encode snapshot");
    let effective = apply_config_snapshot(&worker, &config_json).expect("apply snapshot");

    assert_eq!(effective.output_path, None);
    assert_eq!(effective.request_timeout_ms, None);
    assert_eq!(effective.chrome_wait_for_selector, None);
}

#[test]
fn config_snapshot_does_not_serialize_credential_bearing_endpoint_urls() {
    let mut submitted = Config::test_default();
    submitted.tei_url = "http://user:secret@tei.example/embed?token=abc#frag".to_string();
    submitted.qdrant_url = "http://qdrant.example:6333?api_key=secret".to_string();
    submitted.openai_base_url = "http://token:secret@llm.example/v1?api_key=secret".to_string();
    let mut worker = Config::test_default();
    worker.tei_url = "http://worker-tei:80".to_string();
    worker.qdrant_url = "http://worker-qdrant:6333".to_string();
    worker.openai_base_url = "http://worker-openai:8080/v1".to_string();
    let config_json = config_snapshot_json(&submitted).expect("encode snapshot");
    assert!(!config_json.contains("secret"));
    assert!(!config_json.contains("token=abc"));
    assert!(!config_json.contains("api_key"));
    assert!(!config_json.contains("user:"));

    let effective = apply_config_snapshot(&worker, &config_json).expect("apply snapshot");
    assert_eq!(effective.tei_url, "http://worker-tei:80");
    assert_eq!(effective.qdrant_url, "http://worker-qdrant:6333");
    assert_eq!(effective.openai_base_url, "http://worker-openai:8080/v1");
}

#[test]
fn config_snapshot_rejects_malformed_endpoint_urls() {
    let mut submitted = Config::test_default();
    submitted.tei_url = "not a url".to_string();

    let err = config_snapshot_json(&submitted).expect_err("malformed endpoint fails");

    assert!(
        err.to_string().contains("invalid tei_url"),
        "expected invalid endpoint error, got: {err}"
    );
}

#[test]
fn config_snapshot_does_not_serialize_process_local_endpoint_urls() {
    let mut submitted = Config::test_default();
    submitted.tei_url = "http://127.0.0.1:52000".to_string();
    submitted.qdrant_url = "http://localhost:53333".to_string();
    submitted.chrome_remote_url = Some("http://127.0.0.1:6000".to_string());
    submitted.openai_base_url = "http://localhost:8080/v1".to_string();
    let mut worker = Config::test_default();
    worker.tei_url = "http://worker-tei:80".to_string();
    worker.qdrant_url = "http://worker-qdrant:6333".to_string();
    worker.chrome_remote_url = Some("http://axon-chrome:6000".to_string());
    worker.openai_base_url = "http://worker-openai:8080/v1".to_string();

    let config_json = config_snapshot_json(&submitted).expect("encode snapshot");
    assert!(!config_json.contains("127.0.0.1"));
    assert!(!config_json.contains("localhost"));

    let effective = apply_config_snapshot(&worker, &config_json).expect("apply snapshot");
    assert_eq!(effective.tei_url, "http://worker-tei:80");
    assert_eq!(effective.qdrant_url, "http://worker-qdrant:6333");
    assert_eq!(
        effective.chrome_remote_url.as_deref(),
        Some("http://axon-chrome:6000")
    );
    assert_eq!(effective.openai_base_url, "http://worker-openai:8080/v1");
}

#[test]
fn config_snapshot_does_not_replay_docker_chrome_endpoint_urls() {
    let mut submitted = Config::test_default();
    submitted.chrome_remote_url = Some("http://axon-chrome:6000".to_string());
    let mut worker = Config::test_default();
    worker.chrome_remote_url = Some("http://worker-chrome:6000".to_string());

    let config_json = config_snapshot_json(&submitted).expect("encode snapshot");
    assert!(!config_json.contains("axon-chrome"));

    let effective = apply_config_snapshot(&worker, &config_json).expect("apply snapshot");
    assert_eq!(
        effective.chrome_remote_url.as_deref(),
        Some("http://worker-chrome:6000")
    );
}

#[test]
fn config_snapshot_rejects_adaptive_max_above_worker_cap() {
    let mut submitted = Config::test_default();
    submitted.adaptive_concurrency.enabled = true;
    submitted.adaptive_concurrency.min = 1;
    submitted.adaptive_concurrency.max = Some(2048);
    let mut worker = Config::test_default();
    worker.crawl_broadcast_buffer_max = 1024;

    let config_json = config_snapshot_json(&submitted).expect("encode snapshot");
    let err = apply_config_snapshot(&worker, &config_json)
        .expect_err("adaptive snapshot above worker cap must fail");

    assert!(
        err.to_string()
            .contains("workers.adaptive-concurrency.max must be <="),
        "unexpected error: {err}"
    );
}

#[test]
fn config_snapshot_resolves_adaptive_default_max_after_option_fields() {
    let mut submitted = Config::test_default();
    submitted.crawl_concurrency_limit = Some(64);
    submitted.adaptive_concurrency.enabled = true;
    submitted.adaptive_concurrency.min = 2;
    submitted.adaptive_concurrency.max = None;

    let mut worker = Config::test_default();
    worker.crawl_concurrency_limit = Some(1);
    worker.crawl_broadcast_buffer_max = 128;

    let config_json = config_snapshot_json(&submitted).expect("encode snapshot");
    let effective = apply_config_snapshot(&worker, &config_json).expect("apply snapshot");

    assert_eq!(effective.crawl_concurrency_limit, Some(64));
    assert_eq!(effective.adaptive_concurrency.min, 2);
    assert_eq!(
        effective.adaptive_concurrency.max,
        Some(64),
        "adaptive max=None must resolve against the restored snapshot crawl limit, not the worker process default"
    );
}

#[test]
fn config_snapshot_rejects_invalid_llm_backend_values() {
    let worker = Config::test_default();
    let config_json = r#"{
        "version": 2,
        "config": {
            "llm_backend": "openai-compatible"
        }
    }"#;

    let err = apply_config_snapshot(&worker, config_json).expect_err("invalid backend fails");

    assert!(
        err.to_string().contains("invalid llm_backend"),
        "expected invalid backend error, got: {err}"
    );
}
