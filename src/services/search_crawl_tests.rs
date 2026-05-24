use super::*;
use crate::core::config::CommandKind;
use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
use crate::jobs::config_snapshot::apply_config_snapshot;
use crate::services::runtime::ServiceJobRuntime;
use crate::services::types::ServiceJob;
use std::error::Error as StdError;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub(crate) struct EnqueueCapture {
    payloads: Mutex<Vec<JobPayload>>,
    waits: Mutex<Vec<Uuid>>,
    wait_results: Mutex<Vec<Result<String, String>>>,
    fail: bool,
}

impl EnqueueCapture {
    pub(crate) fn new() -> Self {
        Self {
            payloads: Mutex::new(Vec::new()),
            waits: Mutex::new(Vec::new()),
            wait_results: Mutex::new(Vec::new()),
            fail: false,
        }
    }

    pub(crate) fn failing() -> Self {
        Self {
            payloads: Mutex::new(Vec::new()),
            waits: Mutex::new(Vec::new()),
            wait_results: Mutex::new(Vec::new()),
            fail: true,
        }
    }

    fn failing_wait() -> Self {
        Self::with_wait_results(vec![
            Err("wait timeout".to_string()),
            Err("wait timeout".to_string()),
        ])
    }

    fn with_wait_results(wait_results: Vec<Result<String, String>>) -> Self {
        Self {
            payloads: Mutex::new(Vec::new()),
            waits: Mutex::new(Vec::new()),
            wait_results: Mutex::new(wait_results),
            fail: false,
        }
    }

    pub(crate) fn payloads(&self) -> Vec<JobPayload> {
        self.payloads.lock().unwrap().clone()
    }

    fn waits(&self) -> Vec<Uuid> {
        self.waits.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl ServiceJobRuntime for EnqueueCapture {
    fn mode_name(&self) -> &'static str {
        "test"
    }

    async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid> {
        if self.fail {
            return Err("queue cap exceeded".into());
        }
        self.payloads.lock().unwrap().push(payload);
        Ok(Uuid::new_v4())
    }

    async fn wait_for_job(&self, id: Uuid, _kind: JobKind) -> BackendResult<String> {
        self.waits.lock().unwrap().push(id);
        let result = {
            let mut results = self.wait_results.lock().unwrap();
            if results.is_empty() {
                Ok("completed".to_string())
            } else {
                results.remove(0)
            }
        };
        match result {
            Ok(status) => Ok(status),
            Err(error) => Err(error.into()),
        }
    }

    async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
        Ok(None)
    }

    async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
        Ok(false)
    }

    async fn list_jobs(
        &self,
        _kind: JobKind,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn StdError + Send + Sync>> {
        Ok(Vec::new())
    }

    async fn job_status(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn StdError + Send + Sync>> {
        Ok(None)
    }

    async fn cancel_job(
        &self,
        _kind: JobKind,
        _id: Uuid,
    ) -> Result<bool, Box<dyn StdError + Send + Sync>> {
        Ok(false)
    }

    async fn cleanup_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn clear_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn recover_jobs(
        &self,
        _kind: JobKind,
        _stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs(&self, _kind: JobKind) -> Result<i64, Box<dyn StdError + Send + Sync>> {
        Ok(0)
    }

    async fn count_jobs_by_status(
        &self,
        _kind: JobKind,
    ) -> Result<
        std::collections::HashMap<crate::jobs::status::JobStatus, i64>,
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

pub(crate) fn make_noop_ctx() -> ServiceContext {
    ServiceContext::from_runtime(
        Arc::new(Config::test_default()),
        Arc::new(EnqueueCapture::new()),
    )
}

fn make_ctx(runtime: Arc<dyn ServiceJobRuntime>) -> ServiceContext {
    ServiceContext::from_runtime(Arc::new(Config::test_default()), runtime)
}

#[tokio::test]
async fn enqueue_failure_is_rejected_not_fatal() {
    let cfg = make_cfg("rust");
    let ctx = make_ctx(Arc::new(EnqueueCapture::failing()));
    let results = vec![serde_json::json!({
        "url": "http://93.184.216.34/",
        "title": "Example",
        "position": 1,
    })];

    let output = enqueue_search_crawls(&cfg, &ctx, &results).await;

    assert!(output.jobs.is_empty());
    assert_eq!(output.rejected.len(), 1);
    assert!(
        output.rejected[0].reason.contains("queue cap exceeded"),
        "expected queue error: {:?}",
        output.rejected
    );
}

#[tokio::test]
async fn uses_hardened_bounded_crawl_config() {
    let mut cfg = make_cfg("rust");
    cfg.max_pages = 500;
    cfg.max_depth = 12;
    cfg.discover_sitemaps = true;
    cfg.max_sitemaps = 512;
    cfg.custom_headers = vec!["Authorization: Bearer secret".to_string()];
    cfg.url_whitelist = vec![".*".to_string()];
    let runtime = Arc::new(EnqueueCapture::new());
    let ctx = make_ctx(runtime.clone());
    let results = vec![serde_json::json!({
        "url": "http://93.184.216.34/",
        "title": "Example",
        "position": 1,
    })];

    let output = enqueue_search_crawls(&cfg, &ctx, &results).await;

    assert_eq!(output.jobs.len(), 1);
    assert!(output.rejected.is_empty());
    let payloads = runtime.payloads();
    let JobPayload::Crawl { config_json, .. } = &payloads[0] else {
        panic!("expected crawl payload: {:?}", payloads[0]);
    };
    let effective = apply_config_snapshot(&Config::test_default(), config_json).expect("snapshot");
    assert_eq!(effective.max_pages, 200);
    assert_eq!(effective.max_depth, 10);
    assert!(!effective.discover_sitemaps);
    assert_eq!(effective.max_sitemaps, 0);
    assert!(effective.custom_headers.is_empty());
    assert!(effective.url_whitelist.is_empty());
}

#[tokio::test]
async fn wait_mode_queues_all_search_results_before_waiting() {
    let mut cfg = make_cfg("rust");
    cfg.wait = true;
    let runtime = Arc::new(EnqueueCapture::failing_wait());
    let ctx = make_ctx(runtime.clone());
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

    let output = enqueue_search_crawls(&cfg, &ctx, &results).await;

    assert_eq!(output.jobs.len(), 2);
    assert_eq!(runtime.payloads().len(), 2);
    assert_eq!(runtime.waits().len(), 2);
    assert_eq!(output.rejected.len(), 2);
    assert!(
        output.rejected.iter().all(|rejection| matches!(
            rejection.kind,
            SearchCrawlRejectionKind::WaitFailed
        ) && rejection.reason.contains("wait timeout")),
        "expected wait failures after both jobs were queued: {:?}",
        output.rejected
    );
    assert_eq!(crawl_status(&results, &output), "wait_failed");
}

#[tokio::test]
async fn wait_mode_reports_mixed_wait_outcomes_after_queueing_all_results() {
    let mut cfg = make_cfg("rust");
    cfg.wait = true;
    let runtime = Arc::new(EnqueueCapture::with_wait_results(vec![
        Ok("completed".to_string()),
        Ok("failed".to_string()),
    ]));
    let ctx = make_ctx(runtime.clone());
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

    let output = enqueue_search_crawls(&cfg, &ctx, &results).await;

    assert_eq!(output.jobs.len(), 2);
    assert_eq!(runtime.payloads().len(), 2);
    assert_eq!(runtime.waits().len(), 2);
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
    let runtime = Arc::new(EnqueueCapture::new());
    let ctx = make_ctx(runtime.clone());
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
    assert_eq!(runtime.payloads().len(), 1);
}

#[tokio::test]
async fn crawl_config_disables_wait_during_enqueue_phase() {
    let mut cfg = make_cfg("rust");
    cfg.wait = true;
    let c = crawl_config(&cfg);
    assert!(!c.wait);
    assert_eq!(c.max_pages, 200);
    assert_eq!(c.max_depth, 10);
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
