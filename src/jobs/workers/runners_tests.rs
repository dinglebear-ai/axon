use crate::core::config::Config;
use crate::core::config::RenderMode;
use crate::jobs::config_snapshot::{
    apply_config_snapshot, apply_config_snapshot_for_container, config_snapshot_json,
    decode_ingest_job_config, ingest_config_json,
};
use crate::jobs::ingest::IngestSource;
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
    submitted.llm_backend = crate::core::llm::LlmBackendKind::OpenAiCompat;
    submitted.openai_base_url = "http://submitted-openai:8080/v1".to_string();
    submitted.openai_api_key = "submitted-openai-secret".to_string();
    submitted.openai_model = "submitted-gemma".to_string();
    submitted.headless_gemini_model = "gemini-submitted".to_string();
    submitted.headless_gemini_cmd = "/opt/submitted/gemini".to_string();
    submitted.headless_gemini_home = Some(PathBuf::from("/tmp/submitted-gemini-home"));
    submitted.llm_completion_concurrency = 2;
    submitted.llm_completion_timeout_secs = 17;
    submitted.chrome_proxy = Some("http://submitted-proxy:8080".to_string());
    submitted.custom_headers = vec!["Authorization: Bearer submitted".to_string()];
    submitted.discover_llms_txt = false;
    submitted.max_llms_txt_urls = 77;

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
    worker.llm_backend = crate::core::llm::LlmBackendKind::GeminiHeadless;
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
        crate::core::llm::LlmBackendKind::OpenAiCompat
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
        vec!["Authorization: Bearer submitted".to_string()]
    );
    // llms.txt overrides must survive the enqueue→worker snapshot round-trip,
    // matching the sitemap-discovery parity (async crawl is the common override path).
    assert!(!effective.discover_llms_txt);
    assert_eq!(effective.max_llms_txt_urls, 77);
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
fn config_snapshot_preserves_codex_llm_backend_fields() {
    let worker = Config {
        codex_cmd: "/usr/local/bin/codex".to_string(),
        codex_home: Some(PathBuf::from("/home/worker/.codex")),
        ..Config::default()
    };
    let cfg = Config {
        llm_backend: crate::core::llm::LlmBackendKind::CodexAppServer,
        codex_cmd: "/opt/codex/bin/codex".to_string(),
        codex_home: Some(PathBuf::from("/home/example/.codex")),
        codex_model: "gpt-5.5".to_string(),
        codex_completion_concurrency: 2,
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
        crate::core::llm::LlmBackendKind::CodexAppServer
    );
    assert_eq!(restored.codex_cmd, "/usr/local/bin/codex");
    assert_eq!(
        restored.codex_home,
        Some(PathBuf::from("/home/worker/.codex"))
    );
    assert_eq!(restored.codex_model, "gpt-5.5");
    assert_eq!(restored.codex_completion_concurrency, 2);
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

#[test]
fn ingest_config_snapshot_rejects_invalid_llm_backend_values() {
    let worker = Config::test_default();
    let config_json = r#"{
        "version": 2,
        "source": {
            "source_type": "github",
            "repo": "owner/repo",
            "include_source": true
        },
        "config": {
            "llm_backend": "openai-compatible"
        }
    }"#;

    let err = decode_ingest_job_config(&worker, config_json).expect_err("invalid backend fails");

    assert!(
        err.to_string().contains("invalid llm_backend"),
        "expected invalid backend error, got: {err}"
    );
}

#[test]
fn ingest_job_config_preserves_source_and_supports_legacy_rows() {
    let mut submitted = Config::test_default();
    submitted.collection = "submitted_collection".to_string();
    let source = IngestSource::Github {
        repo: "owner/repo".to_string(),
        include_source: false,
    };

    let mut worker = Config::test_default();
    worker.collection = "worker_collection".to_string();

    let config_json = ingest_config_json(&submitted, &source).expect("encode ingest config");
    let (decoded_source, effective) =
        decode_ingest_job_config(&worker, &config_json).expect("decode ingest config");
    assert!(matches!(
        decoded_source,
        IngestSource::Github {
            ref repo,
            include_source: false,
        } if repo == "owner/repo"
    ));
    assert_eq!(effective.collection, "submitted_collection");

    let legacy_json = serde_json::to_string(&source).expect("encode legacy source");
    let (legacy_source, legacy_effective) =
        decode_ingest_job_config(&worker, &legacy_json).expect("decode legacy ingest config");
    assert!(matches!(
        legacy_source,
        IngestSource::Github {
            ref repo,
            include_source: false,
        } if repo == "owner/repo"
    ));
    assert_eq!(legacy_effective.collection, "worker_collection");
}

#[tokio::test]
async fn extract_runner_returns_canceled_when_token_pre_cancelled() {
    use super::run_extract_job;
    use crate::core::config::Config;
    use crate::jobs::backend::{JobKind, JobPayload};
    use crate::jobs::ops::{claim_next_pending, enqueue_job};
    use crate::jobs::store::open_sqlite_pool;
    use tokio_util::sync::CancellationToken;

    let pool = open_sqlite_pool(":memory:").await.expect("pool");
    // Encode a urls list that would otherwise drive a real fetch — but the
    // pre-cancelled token short-circuits the per-URL loop before any fetch.
    let urls_json = serde_json::to_string(&vec!["https://example.invalid/"]).unwrap();
    let id = enqueue_job(
        &pool,
        &JobPayload::Extract {
            urls: vec!["https://example.invalid/".into()],
            config_json: "{}".into(),
        },
        &Config::default_minimal(),
    )
    .await
    .expect("enqueue");
    // Sanity: enqueue persisted with the urls_json shape we expect.
    let row: (String,) = sqlx::query_as("SELECT urls_json FROM axon_extract_jobs WHERE id = ?")
        .bind(id.to_string())
        .fetch_one(&pool)
        .await
        .expect("fetch");
    assert_eq!(row.0, urls_json);

    claim_next_pending(&pool, JobKind::Extract)
        .await
        .expect("claim");

    let token = CancellationToken::new();
    token.cancel();

    let cfg = Config::default_minimal();
    let result = run_extract_job(&pool, &cfg, id, Some(token)).await;
    let err = result.expect_err("pre-cancelled token must short-circuit extract runner");
    assert!(
        err.to_string().contains("canceled"),
        "expected 'canceled' in error, got: {err}"
    );
}
