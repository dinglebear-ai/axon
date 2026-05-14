mod crawl;
mod embed;
mod extract;
mod ingest;

pub(super) use crawl::run_crawl_job_lite;
pub(super) use embed::run_embed_job_lite;
pub(super) use extract::run_extract_job_lite;
pub(super) use ingest::run_ingest_job_lite;

pub(super) type JobResult =
    Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;

#[cfg(test)]
mod tests {
    use crate::core::config::Config;
    use crate::core::config::RenderMode;
    use crate::jobs::ingest::IngestSource;
    use crate::jobs::lite::config_snapshot::{
        apply_lite_config_snapshot, decode_ingest_job_config, ingest_config_json,
        lite_config_snapshot_json,
    };
    use std::path::PathBuf;

    #[test]
    fn lite_config_snapshot_applies_submitted_non_secret_values() {
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
        submitted.openai_model = "submitted-model".to_string();
        submitted.headless_gemini_model = "gemini-submitted".to_string();
        submitted.headless_gemini_cmd = "/opt/submitted/gemini".to_string();
        submitted.headless_gemini_home = Some(PathBuf::from("/tmp/submitted-gemini-home"));
        submitted.llm_completion_concurrency = 2;
        submitted.llm_completion_timeout_secs = 17;
        submitted.openai_api_key = "submitted-secret".to_string();
        submitted.chrome_proxy = Some("http://submitted-proxy:8080".to_string());
        submitted.custom_headers = vec!["Authorization: Bearer submitted".to_string()];

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
        worker.openai_model = "worker-model".to_string();
        worker.headless_gemini_model = "gemini-worker".to_string();
        worker.headless_gemini_cmd = "/opt/worker/gemini".to_string();
        worker.headless_gemini_home = Some(PathBuf::from("/tmp/worker-gemini-home"));
        worker.llm_completion_concurrency = 8;
        worker.llm_completion_timeout_secs = 99;
        worker.openai_api_key = "worker-secret".to_string();
        worker.chrome_proxy = Some("http://worker-proxy:8080".to_string());
        worker.custom_headers = vec!["Authorization: Bearer worker".to_string()];

        let config_json = match lite_config_snapshot_json(&submitted) {
            Ok(json) => json,
            Err(err) => panic!("snapshot should encode: {err}"),
        };
        let effective = match apply_lite_config_snapshot(&worker, &config_json) {
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
        assert_eq!(effective.openai_model, "submitted-model");
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
        assert_eq!(effective.openai_api_key, "worker-secret");
        assert_eq!(
            effective.custom_headers,
            vec!["Authorization: Bearer submitted".to_string()]
        );
    }

    #[test]
    fn lite_config_snapshot_maps_host_axon_output_dir_for_container_workers() {
        let mut submitted = Config::test_default();
        submitted.output_dir = PathBuf::from("/home/jmagar/.axon/output");

        let mut worker = Config::test_default();
        worker.output_dir = PathBuf::from("/home/axon/.axon/output");

        let config_json = lite_config_snapshot_json(&submitted).expect("snapshot should encode");
        let mut effective =
            apply_lite_config_snapshot(&worker, &config_json).expect("snapshot should apply");

        crate::jobs::lite::config_snapshot::normalize_container_output_dir(
            &worker,
            &mut effective,
            true,
        );

        assert_eq!(
            effective.output_dir,
            PathBuf::from("/home/axon/.axon/output")
        );
    }

    #[test]
    fn lite_config_snapshot_exactly_replays_submitted_none_options() {
        let mut submitted = Config::test_default();
        submitted.output_path = None;
        submitted.request_timeout_ms = None;
        submitted.chrome_wait_for_selector = None;
        let mut worker = Config::test_default();
        worker.output_path = Some(PathBuf::from("/tmp/worker-output.md"));
        worker.request_timeout_ms = Some(999);
        worker.chrome_wait_for_selector = Some("#app".to_string());
        let config_json = lite_config_snapshot_json(&submitted).expect("encode snapshot");
        let effective = apply_lite_config_snapshot(&worker, &config_json).expect("apply snapshot");

        assert_eq!(effective.output_path, None);
        assert_eq!(effective.request_timeout_ms, None);
        assert_eq!(effective.chrome_wait_for_selector, None);
    }

    #[test]
    fn lite_config_snapshot_does_not_serialize_credential_bearing_endpoint_urls() {
        let mut submitted = Config::test_default();
        submitted.tei_url = "http://user:secret@tei.example/embed?token=abc#frag".to_string();
        submitted.qdrant_url = "http://qdrant.example:6333?api_key=secret".to_string();
        submitted.openai_base_url = "https://llm.example/v1?token=secret".to_string();
        let mut worker = Config::test_default();
        worker.tei_url = "http://worker-tei:80".to_string();
        worker.qdrant_url = "http://worker-qdrant:6333".to_string();
        worker.openai_base_url = "http://worker-llm/v1".to_string();
        let config_json = lite_config_snapshot_json(&submitted).expect("encode snapshot");
        assert!(!config_json.contains("secret"));
        assert!(!config_json.contains("token=abc"));
        assert!(!config_json.contains("api_key"));
        assert!(!config_json.contains("user:"));

        let effective = apply_lite_config_snapshot(&worker, &config_json).expect("apply snapshot");
        assert_eq!(effective.tei_url, "http://worker-tei:80");
        assert_eq!(effective.qdrant_url, "http://worker-qdrant:6333");
        assert_eq!(effective.openai_base_url, "http://worker-llm/v1");
    }

    #[test]
    fn lite_config_snapshot_does_not_serialize_process_local_endpoint_urls() {
        let mut submitted = Config::test_default();
        submitted.tei_url = "http://127.0.0.1:52000".to_string();
        submitted.qdrant_url = "http://localhost:53333".to_string();
        submitted.openai_base_url = "http://[::1]:8317/v1".to_string();
        let mut worker = Config::test_default();
        worker.tei_url = "http://worker-tei:80".to_string();
        worker.qdrant_url = "http://worker-qdrant:6333".to_string();
        worker.openai_base_url = "http://worker-llm/v1".to_string();

        let config_json = lite_config_snapshot_json(&submitted).expect("encode snapshot");
        assert!(!config_json.contains("127.0.0.1"));
        assert!(!config_json.contains("localhost"));
        assert!(!config_json.contains("[::1]"));

        let effective = apply_lite_config_snapshot(&worker, &config_json).expect("apply snapshot");
        assert_eq!(effective.tei_url, "http://worker-tei:80");
        assert_eq!(effective.qdrant_url, "http://worker-qdrant:6333");
        assert_eq!(effective.openai_base_url, "http://worker-llm/v1");
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
        use super::run_extract_job_lite;
        use crate::core::config::Config;
        use crate::jobs::backend::{JobKind, JobPayload};
        use crate::jobs::lite::ops::{claim_next_pending, enqueue_job};
        use crate::jobs::lite::store::open_sqlite_pool;
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
            &Config::default_lite(),
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

        let cfg = Config::default_lite();
        let result = run_extract_job_lite(&pool, &cfg, id, Some(token)).await;
        let err = result.expect_err("pre-cancelled token must short-circuit extract runner");
        assert!(
            err.to_string().contains("canceled"),
            "expected 'canceled' in error, got: {err}"
        );
    }
}
