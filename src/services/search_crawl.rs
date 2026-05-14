use crate::core::config::Config;
use crate::core::http::{normalize_url, validate_url_with_dns};
use crate::services::context::ServiceContext;
use crate::services::crawl as crawl_service;
use crate::services::search::search_batch;
use crate::services::types::SearchOptions;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::error::Error;

/// Typed result returned by [`search_and_crawl`].
///
/// Contains Tavily search results plus the outcome of auto-enqueueing
/// one shallow crawl job per result URL.
pub struct SearchAndCrawlResult {
    pub results: Vec<Value>,
    pub crawl_jobs: Vec<SearchCrawlJob>,
    pub crawl_rejected: Vec<SearchCrawlRejection>,
    pub auto_crawl_status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct SearchCrawlJob {
    pub url: String,
    pub job_id: String,
}

#[derive(Debug, Serialize)]
pub struct SearchCrawlRejection {
    pub url: Option<String>,
    pub position: Option<i64>,
    pub title: Option<String>,
    pub kind: SearchCrawlRejectionKind,
    pub reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchCrawlRejectionKind {
    DuplicateUrl,
    InvalidUrl,
    MissingUrl,
    QueueRejected,
}

/// Run a Tavily search and enqueue one shallow crawl job per result URL.
///
/// This is the canonical entry point for both the CLI and MCP search action.
/// Callers receive a typed result and decide their own UX (error on zero jobs,
/// include in JSON response, etc.) — this function never errors on partial
/// crawl failures.
pub async fn search_and_crawl(
    cfg: &Config,
    service_context: &ServiceContext,
    query: &str,
    opts: SearchOptions,
) -> Result<SearchAndCrawlResult, Box<dyn Error>> {
    let results = search_batch(cfg, &[query], opts, None).await?.results;
    let crawl_output = enqueue_search_crawls(cfg, service_context, &results).await;
    let auto_crawl_status = crawl_status(&results, &crawl_output);
    Ok(SearchAndCrawlResult {
        results,
        crawl_jobs: crawl_output.jobs,
        crawl_rejected: crawl_output.rejected,
        auto_crawl_status,
    })
}

// ── internals ────────────────────────────────────────────────────────────────

#[derive(Default)]
struct CrawlOutput {
    jobs: Vec<SearchCrawlJob>,
    rejected: Vec<SearchCrawlRejection>,
}

fn crawl_config(cfg: &Config) -> Config {
    // SECURITY: clear headers so auth meant for the search caller is never
    // replayed against URLs returned by Tavily.
    let mut c = cfg.clone();
    c.max_pages = 1;
    c.max_depth = 1;
    c.discover_sitemaps = false;
    c.max_sitemaps = 0;
    c.custom_headers = Vec::new();
    c.url_whitelist = Vec::new();
    c
}

async fn enqueue_search_crawls(
    cfg: &Config,
    service_context: &ServiceContext,
    results: &[Value],
) -> CrawlOutput {
    let crawl_cfg = crawl_config(cfg);
    let mut output = CrawlOutput::default();
    let mut seen = HashSet::new();

    for result in results {
        let Some(url) = result["url"].as_str().filter(|u| !u.is_empty()) else {
            output.rejected.push(result_rejection(
                result,
                SearchCrawlRejectionKind::MissingUrl,
                "search result missing url",
            ));
            continue;
        };
        let normalized = normalize_url(url).into_owned();
        if !seen.insert(normalized.clone()) {
            output.rejected.push(rejection(
                Some(&normalized),
                None,
                None,
                SearchCrawlRejectionKind::DuplicateUrl,
                "duplicate search result URL",
            ));
            continue;
        }
        match enqueue_one(&crawl_cfg, service_context, &normalized).await {
            Ok(job) => output.jobs.push(job),
            Err(r) => output.rejected.push(r),
        }
    }

    output
}

async fn enqueue_one(
    crawl_cfg: &Config,
    service_context: &ServiceContext,
    url: &str,
) -> Result<SearchCrawlJob, SearchCrawlRejection> {
    if let Err(e) = validate_url_with_dns(url).await {
        return Err(rejection(
            Some(url),
            None,
            None,
            SearchCrawlRejectionKind::InvalidUrl,
            e.to_string(),
        ));
    }

    let url_owned = url.to_string();
    match crawl_service::crawl_start_with_context(
        crawl_cfg,
        std::slice::from_ref(&url_owned),
        service_context,
        None,
    )
    .await
    {
        Ok(outcome) => {
            let Some(job) = outcome.result.jobs.first() else {
                return Err(rejection(
                    Some(url),
                    None,
                    None,
                    SearchCrawlRejectionKind::QueueRejected,
                    "crawl service returned no job id",
                ));
            };
            Ok(SearchCrawlJob {
                url: url_owned,
                job_id: job.job_id.clone(),
            })
        }
        Err(e) => {
            let reason = e.to_string();
            tracing::warn!(url = %url, error = %reason, "search auto-index: enqueue failed");
            Err(rejection(
                Some(url),
                None,
                None,
                SearchCrawlRejectionKind::QueueRejected,
                reason,
            ))
        }
    }
}

fn rejection(
    url: Option<&str>,
    position: Option<i64>,
    title: Option<&str>,
    kind: SearchCrawlRejectionKind,
    reason: impl Into<String>,
) -> SearchCrawlRejection {
    SearchCrawlRejection {
        url: url.map(str::to_string),
        position,
        title: title.map(str::to_string),
        kind,
        reason: reason.into(),
    }
}

fn result_rejection(
    result: &Value,
    kind: SearchCrawlRejectionKind,
    reason: impl Into<String>,
) -> SearchCrawlRejection {
    rejection(
        result["url"].as_str(),
        result["position"].as_i64(),
        result["title"].as_str(),
        kind,
        reason,
    )
}

fn crawl_status(results: &[Value], output: &CrawlOutput) -> &'static str {
    if results.is_empty() {
        "no_results"
    } else if output.jobs.is_empty() {
        "failed"
    } else if output.rejected.is_empty() {
        "queued"
    } else {
        "partial"
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::core::config::CommandKind;
    use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
    use crate::jobs::lite::config_snapshot::apply_lite_config_snapshot;
    use crate::services::runtime::ServiceJobRuntime;
    use crate::services::types::ServiceJob;
    use std::error::Error as StdError;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    struct EnqueueCapture {
        payloads: Mutex<Vec<JobPayload>>,
        fail: bool,
    }

    impl EnqueueCapture {
        fn new() -> Self {
            Self {
                payloads: Mutex::new(Vec::new()),
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                payloads: Mutex::new(Vec::new()),
                fail: true,
            }
        }

        fn payloads(&self) -> Vec<JobPayload> {
            self.payloads.lock().unwrap().clone()
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

        async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
            Ok("completed".to_string())
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

        async fn cleanup_jobs(
            &self,
            _kind: JobKind,
        ) -> Result<u64, Box<dyn StdError + Send + Sync>> {
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
    async fn uses_hardened_single_page_config() {
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
        let effective =
            apply_lite_config_snapshot(&Config::test_default(), config_json).expect("snapshot");
        assert_eq!(effective.max_pages, 1);
        assert_eq!(effective.max_depth, 1);
        assert!(!effective.discover_sitemaps);
        assert_eq!(effective.max_sitemaps, 0);
        assert!(effective.custom_headers.is_empty());
        assert!(effective.url_whitelist.is_empty());
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
    async fn crawl_config_preserves_wait_mode() {
        let mut cfg = make_cfg("rust");
        cfg.wait = true;
        let c = crawl_config(&cfg);
        assert!(c.wait);
        assert_eq!(c.max_pages, 1);
        assert_eq!(c.max_depth, 1);
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
    }
}
