use super::*;
use crate::runtime::ServiceJobRuntime;
use crate::search_source_index::enqueue_web_source_auto_index;
use crate::types::ResearchHit;
use axon_api::source::{JobId, LifecycleStatus, PipelinePhase, SourceScope};
use axon_core::config::CommandKind;
use std::error::Error as StdError;
use std::sync::Arc;

/// Set a fresh temp-dir `sqlite_path` on `cfg` so each test gets an isolated
/// unified job store. The tempdir is intentionally leaked (`mem::forget`) for
/// the test process's lifetime, matching `crawl_tests.rs::test_ctx_with_workers`.
fn with_isolated_sqlite_path(mut cfg: Config) -> Config {
    let dir = tempfile::tempdir().expect("tempdir");
    cfg.sqlite_path = dir.path().join("jobs.db");
    std::mem::forget(dir);
    cfg
}

/// Real in-memory `ServiceContext` with unified-store-backed workers, mirroring
/// `crawl_tests.rs::test_ctx_with_workers`. Crawl now enqueues onto the
/// unified job store, so `search_crawl`'s
/// tests need a real store to enqueue against and (for the wait-mode tests)
/// to drive terminal status transitions on directly. Takes `cfg` by value
/// (isolating only `sqlite_path`) so `service_context.cfg.job_wait_timeout_secs`
/// (read by `wait_for_unified_crawl_job`) matches whatever the test configured,
/// rather than silently reverting to `Config::test_default()`.
async fn test_ctx_with_workers(cfg: Config) -> ServiceContext {
    ServiceContext::new_with_workers(Arc::new(with_isolated_sqlite_path(cfg)))
        .await
        .expect("service context")
}

/// Like [`test_ctx_with_workers`], but with no in-process workers attached —
/// for tests that need a real unified store to enqueue against and drive
/// terminal status transitions on directly, without racing a real background
/// worker for control of the same rows (which produces intermittent SQLite
/// `database is locked` errors under the shared connection pool).
async fn test_ctx_no_workers(cfg: Config) -> ServiceContext {
    ServiceContext::new(Arc::new(with_isolated_sqlite_path(cfg)))
        .await
        .expect("service context")
}

/// A `ServiceContext` whose `unified_job_store()` returns `None` — simulates
/// an enqueue-only runtime with no in-process workers attached, so
/// source enqueue fails with "unified job store is not available",
/// reproducing the queue-full/enqueue-failure family of tests.
struct NoStoreRuntime;

#[async_trait::async_trait]
impl ServiceJobRuntime for NoStoreRuntime {
    fn mode_name(&self) -> &'static str {
        "test-no-store"
    }

    async fn enqueue(
        &self,
        _payload: axon_jobs::backend::JobPayload,
    ) -> axon_jobs::backend::BackendResult<uuid::Uuid> {
        Ok(uuid::Uuid::new_v4())
    }

    async fn wait_for_job(
        &self,
        _id: uuid::Uuid,
        _kind: axon_jobs::backend::JobKind,
    ) -> axon_jobs::backend::BackendResult<String> {
        Ok("completed".to_string())
    }

    async fn job_errors(
        &self,
        _id: uuid::Uuid,
        _kind: axon_jobs::backend::JobKind,
    ) -> axon_jobs::backend::BackendResult<Option<String>> {
        Ok(None)
    }

    async fn has_active_jobs(
        &self,
        _kind: axon_jobs::backend::JobKind,
    ) -> axon_jobs::backend::BackendResult<bool> {
        Ok(false)
    }

    async fn list_jobs(
        &self,
        _kind: axon_jobs::backend::JobKind,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<crate::types::ServiceJob>, Box<dyn StdError + Send + Sync>> {
        Ok(Vec::new())
    }

    async fn job_status(
        &self,
        _kind: axon_jobs::backend::JobKind,
        _id: uuid::Uuid,
    ) -> Result<Option<crate::types::ServiceJob>, Box<dyn StdError + Send + Sync>> {
        Ok(None)
    }

    async fn cancel_job(
        &self,
        _kind: axon_jobs::backend::JobKind,
        _id: uuid::Uuid,
    ) -> Result<bool, Box<dyn StdError + Send + Sync>> {
        Ok(false)
    }

    async fn cleanup_jobs(
        &self,
        _kind: axon_jobs::backend::JobKind,
    ) -> Result<u64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn clear_jobs(
        &self,
        _kind: axon_jobs::backend::JobKind,
    ) -> Result<u64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn recover_jobs(
        &self,
        _kind: axon_jobs::backend::JobKind,
        _stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs(
        &self,
        _kind: axon_jobs::backend::JobKind,
    ) -> Result<i64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs_by_status(
        &self,
        _kind: axon_jobs::backend::JobKind,
    ) -> Result<
        std::collections::HashMap<axon_jobs::status::JobStatus, i64>,
        Box<dyn StdError + Send + Sync>,
    > {
        Ok(std::collections::HashMap::new())
    }
}

fn make_cfg(query: &str) -> Config {
    let mut cfg = Config::test_default();
    cfg.command = CommandKind::Search;
    cfg.positional = vec![query.to_string()];
    cfg.tavily_api_key = "tvly-key".to_string();
    cfg
}

/// Transition a freshly enqueued job through `Running` to the given terminal
/// `LifecycleStatus`, matching the state machine's `Queued -> Running ->
/// terminal` requirement (a direct `Queued -> Completed` update is rejected).
async fn force_terminal_status(ctx: &ServiceContext, job_id: &str, status: LifecycleStatus) {
    let store = ctx.job_store().expect("unified job store must be attached");
    let job_id = JobId(uuid::Uuid::parse_str(job_id).expect("valid job id"));
    store
        .update_status(axon_api::source::JobStatusUpdate {
            job_id,
            source_id: None,
            status: LifecycleStatus::Running,
            phase: PipelinePhase::Fetching,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .expect("transition to running");
    store
        .update_status(axon_api::source::JobStatusUpdate {
            job_id,
            source_id: None,
            status,
            phase: PipelinePhase::Complete,
            stage_id: None,
            counts: None,
            current: None,
            message: None,
            error: None,
        })
        .await
        .expect("transition to terminal status");
}

#[tokio::test]
async fn enqueue_failure_is_rejected_not_fatal() {
    let cfg = make_cfg("rust");
    let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), Arc::new(NoStoreRuntime));
    let results = vec![serde_json::json!({
        "url": "http://93.184.216.34/",
        "title": "Example",
        "position": 1,
    })];

    let output = enqueue_search_crawls(&cfg, &ctx, &results).await;

    assert!(output.jobs.is_empty());
    assert_eq!(output.rejected.len(), 1);
    assert!(
        output.rejected[0]
            .reason
            .contains("unified job store is not available"),
        "expected queue error: {:?}",
        output.rejected
    );
}

#[tokio::test]
async fn uses_hardened_bounded_source_request() {
    let mut cfg = make_cfg("rust");
    cfg.max_pages = 500;
    cfg.max_depth = 12;
    cfg.discover_sitemaps = true;
    cfg.max_sitemaps = 512;
    cfg.custom_headers = vec!["Authorization: Bearer secret".to_string()];
    cfg.url_whitelist = vec![".*".to_string()];
    let ctx = test_ctx_with_workers(cfg.clone()).await;
    let results = vec![serde_json::json!({
        "url": "http://93.184.216.34/",
        "title": "Example",
        "position": 1,
    })];

    let output = enqueue_search_crawls(&cfg, &ctx, &results).await;

    assert_eq!(output.jobs.len(), 1);
    assert!(output.rejected.is_empty());
    let store = ctx.job_store().expect("unified job store must be attached");
    let job_id = JobId(uuid::Uuid::parse_str(&output.jobs[0].job_id).unwrap());
    let request_json = store
        .request_json(job_id)
        .await
        .expect("request_json")
        .expect("request json stored");
    let source_request = request_json
        .get("source_request")
        .expect("source_request field present");
    assert_eq!(
        source_request
            .pointer("/limits/max_pages")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        source_request
            .pointer("/limits/max_depth")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        source_request.get("intent").and_then(|v| v.as_str()),
        Some("acquire")
    );
    assert_eq!(
        source_request.get("scope").and_then(|v| v.as_str()),
        Some("page")
    );
    assert_eq!(
        source_request
            .pointer("/options/values/discover_sitemaps")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        source_request
            .pointer("/options/values/max_sitemaps")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        source_request.pointer("/options/values/url_whitelist"),
        Some(&serde_json::json!([]))
    );
    assert!(
        source_request
            .pointer("/options/values/custom_headers")
            .is_none(),
        "web SourceRequest options must not carry search caller headers"
    );
}

#[tokio::test]
async fn wait_mode_queues_all_search_results_before_waiting() {
    let mut cfg = make_cfg("rust");
    cfg.wait = true;
    // Bound the wait loop tightly so this test fails fast instead of waiting
    // out the default timeout for jobs that are deliberately never completed.
    cfg.job_wait_timeout_secs = 1;
    // No background worker: these jobs are meant to sit `Queued` until the
    // wait loop times out, not race a real crawl attempt.
    let ctx = test_ctx_no_workers(cfg.clone()).await;
    let results = vec![
        serde_json::json!({
            "url": "http://93.184.216.34/",
            "title": "Example",
            "position": 1,
        }),
        serde_json::json!({
            "url": "http://1.1.1.1/",
            "title": "Cloudflare",
            "position": 2,
        }),
    ];

    // No in-process worker will ever claim these jobs in this test (the
    // unified worker IS running, but nothing here drives them to a terminal
    // state before the 1s timeout), so the wait loop should reliably time out
    // for both queued jobs.
    let output = enqueue_search_crawls(&cfg, &ctx, &results).await;

    assert_eq!(output.jobs.len(), 2);
    assert_eq!(output.rejected.len(), 2);
    assert!(
        output.rejected.iter().all(|rejection| matches!(
            rejection.kind,
            SearchCrawlRejectionKind::WaitFailed
        ) && rejection.reason.contains("wait timed out")),
        "expected wait failures after both jobs were queued: {:?}",
        output.rejected
    );
    assert_eq!(crawl_status(&results, &output), "wait_failed");
}

#[tokio::test]
async fn wait_mode_reports_mixed_wait_outcomes_after_queueing_all_results() {
    let mut cfg = make_cfg("rust");
    cfg.wait = true;
    // No background worker here: this test drives terminal status by hand
    // and would otherwise race a real unified worker for control of the same
    // rows.
    let ctx = test_ctx_no_workers(cfg.clone()).await;
    let results = vec![
        serde_json::json!({
            "url": "http://93.184.216.34/",
            "title": "Example",
            "position": 1,
        }),
        serde_json::json!({
            "url": "http://1.1.1.1/",
            "title": "Cloudflare",
            "position": 2,
        }),
    ];

    // Enqueue directly (bypassing wait mode) so this test can force each
    // job's terminal status before `enqueue_search_crawls`'s wait loop
    // observes it, rather than racing a real source job.
    let auto_index_cfg = auto_index_config(&cfg);
    let mut jobs = Vec::new();
    for result in &results {
        let url = result["url"].as_str().unwrap();
        let job = enqueue_web_source_auto_index(
            &auto_index_cfg,
            &ctx,
            url,
            SourceScope::Page,
            1,
            0,
            auto_index_cfg.embed,
            "search",
        )
        .await
        .expect("enqueue");
        jobs.push(job.id.0.to_string());
    }
    force_terminal_status(&ctx, &jobs[0], LifecycleStatus::Completed).await;
    force_terminal_status(&ctx, &jobs[1], LifecycleStatus::Failed).await;

    let mut output = CrawlOutput {
        jobs: jobs
            .iter()
            .zip(results.iter())
            .map(|(job_id, result)| SearchCrawlJob {
                url: result["url"].as_str().unwrap().to_string(),
                job_id: job_id.clone(),
            })
            .collect(),
        rejected: Vec::new(),
    };
    wait_for_queued_crawls(&ctx, &mut output).await;

    assert_eq!(output.jobs.len(), 2);
    assert_eq!(output.rejected.len(), 1);
    assert!(matches!(
        output.rejected[0].kind,
        SearchCrawlRejectionKind::WaitFailed
    ));
    assert_eq!(crawl_status(&results, &output), "partial_wait_failed");
}

#[tokio::test]
async fn rejects_invalid_missing_and_duplicate_urls() {
    let cfg = make_cfg("rust");
    let ctx = test_ctx_with_workers(cfg.clone()).await;
    let results = vec![
        serde_json::json!({"url": "", "title": "Missing", "position": 1}),
        serde_json::json!({"url": "http://127.0.0.1/", "title": "Loopback", "position": 2}),
        serde_json::json!({"url": "http://169.254.169.254/", "title": "Metadata", "position": 3}),
        serde_json::json!({"url": "ftp://example.com/", "title": "FTP", "position": 4}),
        serde_json::json!({"url": "http://93.184.216.34/", "title": "Example", "position": 5}),
        serde_json::json!({"url": "http://93.184.216.34/", "title": "Duplicate", "position": 6}),
    ];

    let output = enqueue_search_crawls(&cfg, &ctx, &results).await;

    assert_eq!(output.jobs.len(), 1);
    assert_eq!(output.rejected.len(), 5);
    assert!(matches!(
        output.rejected[0].kind,
        SearchCrawlRejectionKind::MissingUrl
    ));
    assert!(
        output.rejected[1..4]
            .iter()
            .all(|r| matches!(r.kind, SearchCrawlRejectionKind::InvalidUrl))
    );
    assert!(matches!(
        output.rejected[4].kind,
        SearchCrawlRejectionKind::DuplicateUrl
    ));
}

#[tokio::test]
async fn auto_index_config_disables_wait_during_enqueue_phase() {
    let mut cfg = make_cfg("rust");
    cfg.wait = true;
    let c = auto_index_config(&cfg);
    assert!(!c.wait);
    assert_eq!(c.max_pages, 1);
    assert_eq!(c.max_depth, 0);
}

#[test]
fn crawl_status_variants() {
    let no_jobs = CrawlOutput::default();
    assert_eq!(crawl_status(&[], &no_jobs), "no_results");

    let results = vec![serde_json::json!({"url": "https://example.com"})];
    assert_eq!(crawl_status(&results, &no_jobs), "failed");

    let with_job = CrawlOutput {
        jobs: vec![SearchCrawlJob {
            url: "https://example.com".into(),
            job_id: "abc".into(),
        }],
        rejected: Vec::new(),
    };
    assert_eq!(crawl_status(&results, &with_job), "queued");

    let partial = CrawlOutput {
        jobs: vec![SearchCrawlJob {
            url: "https://example.com".into(),
            job_id: "abc".into(),
        }],
        rejected: vec![rejection(
            Some("https://bad.com"),
            None,
            None,
            SearchCrawlRejectionKind::InvalidUrl,
            "test",
        )],
    };
    assert_eq!(crawl_status(&results, &partial), "partial");

    let wait_failed = CrawlOutput {
        jobs: vec![SearchCrawlJob {
            url: "https://example.com".into(),
            job_id: "abc".into(),
        }],
        rejected: vec![rejection(
            Some("https://example.com"),
            None,
            None,
            SearchCrawlRejectionKind::WaitFailed,
            "wait timeout",
        )],
    };
    assert_eq!(crawl_status(&results, &wait_failed), "wait_failed");
}

#[tokio::test]
async fn enqueue_research_crawls_embeds_by_default_and_honors_skip_embed() {
    let mut cfg = make_cfg("research");
    cfg.embed = true;
    let ctx = test_ctx_with_workers(cfg.clone()).await;
    let hits = vec![ResearchHit {
        position: 1,
        title: "Official Docs".to_string(),
        url: "http://93.184.216.34/".to_string(),
        snippet: Some("docs".to_string()),
    }];

    let output = enqueue_research_crawls(&cfg, &ctx, &hits).await;

    assert_eq!(output.jobs.len(), 1);
    let source_request = crawl_job_source_request_json(&ctx, &output.jobs[0].job_id).await;
    assert!(
        source_request
            .get("embed")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        "research auto-index should embed by default"
    );

    cfg.embed = false;
    let ctx = test_ctx_with_workers(cfg.clone()).await;
    let output = enqueue_research_crawls(&cfg, &ctx, &hits).await;

    assert_eq!(output.jobs.len(), 1);
    let source_request = crawl_job_source_request_json(&ctx, &output.jobs[0].job_id).await;
    assert!(
        !source_request
            .get("embed")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
        "--skip-embed should carry into research source jobs"
    );
}

async fn crawl_job_source_request_json(ctx: &ServiceContext, job_id: &str) -> serde_json::Value {
    let store = ctx.job_store().expect("unified job store must be attached");
    let job_id = JobId(uuid::Uuid::parse_str(job_id).unwrap());
    let request_json = store
        .request_json(job_id)
        .await
        .expect("request_json")
        .expect("request json stored");
    request_json
        .get("source_request")
        .expect("source_request field present")
        .clone()
}
