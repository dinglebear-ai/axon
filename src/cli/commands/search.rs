use crate::cli::commands::common::parse_service_time_range;
use crate::cli::commands::resolve_input_text;
use crate::core::config::Config;
use crate::core::http::{normalize_url, validate_url_with_dns};
use crate::core::logging::{log_done, log_info, log_warn};
use crate::core::ui::{muted, primary, print_phase};
use crate::services::context::ServiceContext;
use crate::services::crawl as crawl_service;
use crate::services::search::search_batch;
use crate::services::types::SearchOptions as ServiceSearchOptions;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::error::Error;

pub async fn run_search(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    if cfg.tavily_api_key.is_empty() {
        return Err(anyhow::anyhow!(
            "search requires TAVILY_API_KEY — set it in .env (run 'axon doctor' to check service connectivity)"
        )
        .into());
    }

    let query = resolve_input_text(cfg)
        .ok_or_else(|| anyhow::anyhow!("search requires a query (positional or --query)"))?;

    if !cfg.quiet && !cfg.json_output {
        log_info(&format!("command=search query_len={}", query.len()));
        print_phase("\u{25d0}", "Searching", &query);
    }

    let opts = ServiceSearchOptions {
        limit: cfg.search_limit,
        offset: 0,
        time_range: parse_service_time_range(cfg.search_time_range.as_deref()),
    };

    let search_start = std::time::Instant::now();
    let results = search_batch(cfg, &[query.as_str()], opts, None)
        .await?
        .results;
    let duration_ms = search_start.elapsed().as_millis();

    let crawl_output = enqueue_search_crawls(cfg, service_context, &results).await;
    let no_crawl_jobs_queued = !results.is_empty() && crawl_output.jobs.is_empty();
    let first_rejection = crawl_output.rejected.first();
    let auto_crawl_status = search_crawl_status(&results, &crawl_output);

    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "auto_crawl_status": auto_crawl_status,
                "query": query,
                "limit": cfg.search_limit,
                "offset": 0,
                "search_time_range": cfg.search_time_range.as_deref(),
                "results": results,
                "crawl_jobs": crawl_output.jobs,
                "crawl_jobs_rejected": crawl_output.rejected,
            }))?
        );
        return Ok(());
    }

    print_search_results(&query, &results);
    log_search_crawl_summary(cfg, &crawl_output);
    if no_crawl_jobs_queued {
        return Err(search_crawl_total_failure(first_rejection).into());
    }

    if !cfg.quiet {
        log_done(&format!(
            "command=search complete query_len={} results={} duration_ms={duration_ms}",
            query.len(),
            results.len()
        ));
    }
    Ok(())
}

#[derive(Default)]
struct SearchCrawlOutput {
    jobs: Vec<SearchCrawlJob>,
    rejected: Vec<SearchCrawlRejection>,
}

#[derive(Debug, Serialize)]
struct SearchCrawlJob {
    url: String,
    job_id: String,
}

#[derive(Debug, Serialize)]
struct SearchCrawlRejection {
    url: Option<String>,
    position: Option<i64>,
    title: Option<String>,
    kind: SearchCrawlRejectionKind,
    reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum SearchCrawlRejectionKind {
    DuplicateUrl,
    InvalidUrl,
    MissingUrl,
    QueueRejected,
}

fn search_crawl_config(cfg: &Config) -> Config {
    // SECURITY: clear headers so auth meant for the search caller is never replayed
    // against URLs returned by Tavily.
    let mut search_cfg = cfg.clone();
    search_cfg.max_pages = 1;
    search_cfg.max_depth = 1;
    search_cfg.discover_sitemaps = false;
    search_cfg.max_sitemaps = 0;
    search_cfg.custom_headers = Vec::new();
    search_cfg.url_whitelist = Vec::new();
    search_cfg
}

async fn enqueue_search_crawls(
    cfg: &Config,
    service_context: &ServiceContext,
    results: &[Value],
) -> SearchCrawlOutput {
    let search_cfg = search_crawl_config(cfg);
    let mut output = SearchCrawlOutput::default();
    let mut seen = HashSet::new();

    for result in results {
        let Some(url) = result["url"].as_str().filter(|url| !url.is_empty()) else {
            output.rejected.push(search_crawl_result_rejection(
                result,
                SearchCrawlRejectionKind::MissingUrl,
            ));
            log_search_crawl_rejection(&search_cfg, output.rejected.last().expect("just pushed"));
            continue;
        };
        let normalized = normalize_url(url).into_owned();
        if !seen.insert(normalized.clone()) {
            output.rejected.push(search_crawl_rejection(
                Some(&normalized),
                SearchCrawlRejectionKind::DuplicateUrl,
                "duplicate search result URL",
            ));
            log_search_crawl_rejection(&search_cfg, output.rejected.last().expect("just pushed"));
            continue;
        };
        match enqueue_search_crawl_url(&search_cfg, service_context, &normalized).await {
            Ok(job) => output.jobs.push(job),
            Err(rejection) => output.rejected.push(rejection),
        }
    }

    output
}

async fn enqueue_search_crawl_url(
    search_cfg: &Config,
    service_context: &ServiceContext,
    url: &str,
) -> Result<SearchCrawlJob, SearchCrawlRejection> {
    if let Err(error) = validate_url_with_dns(url).await {
        let rejection = search_crawl_rejection(
            Some(url),
            SearchCrawlRejectionKind::InvalidUrl,
            error.to_string(),
        );
        log_search_crawl_rejection(search_cfg, &rejection);
        return Err(rejection);
    }

    let url = url.to_string();
    let outcome = crawl_service::crawl_start_with_context(
        search_cfg,
        std::slice::from_ref(&url),
        service_context,
        None,
    )
    .await;

    match outcome {
        Ok(outcome) => {
            let Some(job) = outcome.result.jobs.first() else {
                return Err(search_crawl_rejection(
                    Some(&url),
                    SearchCrawlRejectionKind::QueueRejected,
                    "crawl service returned no job id",
                ));
            };
            Ok(SearchCrawlJob {
                url,
                job_id: job.job_id.clone(),
            })
        }
        Err(error) => {
            let reason = error.to_string();
            tracing::warn!(url = %url, error = %reason, "search auto-index: enqueue failed");
            Err(search_crawl_rejection(
                Some(&url),
                SearchCrawlRejectionKind::QueueRejected,
                reason,
            ))
        }
    }
}

fn search_crawl_rejection(
    url: Option<&str>,
    kind: SearchCrawlRejectionKind,
    reason: impl Into<String>,
) -> SearchCrawlRejection {
    SearchCrawlRejection {
        url: url.map(str::to_string),
        position: None,
        title: None,
        kind,
        reason: reason.into(),
    }
}

fn search_crawl_result_rejection(
    result: &Value,
    kind: SearchCrawlRejectionKind,
) -> SearchCrawlRejection {
    SearchCrawlRejection {
        url: result["url"].as_str().map(str::to_string),
        position: result["position"].as_i64(),
        title: result["title"].as_str().map(str::to_string),
        reason: "search result missing url".to_string(),
        kind,
    }
}

fn search_crawl_total_failure(first: Option<&SearchCrawlRejection>) -> anyhow::Error {
    let reason = first
        .map(|rejection| rejection.reason.as_str())
        .unwrap_or("unknown rejection");
    anyhow::anyhow!(
        "search completed, but no result URLs were queued for crawl; first failure: {reason}"
    )
}

fn log_search_crawl_rejection(cfg: &Config, rejection: &SearchCrawlRejection) {
    if cfg.json_output {
        return;
    }
    let url = rejection.url.as_deref().unwrap_or("<missing>");
    log_warn(&format!(
        "search auto-index: skipped {url}: {}",
        rejection.reason
    ));
}

fn print_search_results(query: &str, results: &[Value]) {
    println!("{}", primary(&format!("Search Results for \"{query}\"")));
    println!("{} {}\n", muted("Found"), results.len());

    for result in results {
        let position = result["position"].as_i64().unwrap_or(0);
        let title = result["title"].as_str().unwrap_or("");
        let url = result["url"].as_str().unwrap_or("");
        println!("{}. {}", position, primary(title));
        println!("   {}", muted(url));
        if let Some(s) = result["snippet"].as_str() {
            println!("   {s}");
        }
        println!();
    }
}

fn log_search_crawl_summary(cfg: &Config, output: &SearchCrawlOutput) {
    if !output.jobs.is_empty() && !cfg.quiet {
        log_info(&format!(
            "search auto-index: queued {} crawl job(s). Run 'axon serve' or 'axon crawl worker' if workers are not running.",
            output.jobs.len()
        ));
    }
    if !output.rejected.is_empty() && !cfg.json_output {
        log_warn(&format!(
            "search auto-index: {} URL(s) could not be queued; indexing is partial",
            output.rejected.len()
        ));
        for rejection in &output.rejected {
            let url = rejection.url.as_deref().unwrap_or("<missing>");
            log_warn(&format!("  - {url}: {}", rejection.reason));
        }
    }
}

fn search_crawl_status(results: &[Value], output: &SearchCrawlOutput) -> &'static str {
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
mod tests {
    use super::*;
    use crate::core::config::CommandKind;
    use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
    use crate::jobs::lite::config_snapshot::apply_lite_config_snapshot;
    use crate::services::runtime::ServiceJobRuntime;
    use crate::services::types::{ServiceJob, ServiceTimeRange};
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
        ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
            Ok(Vec::new())
        }

        async fn job_status(
            &self,
            _kind: JobKind,
            _id: Uuid,
        ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> {
            Ok(None)
        }

        async fn cancel_job(
            &self,
            _kind: JobKind,
            _id: Uuid,
        ) -> Result<bool, Box<dyn Error + Send + Sync>> {
            Ok(false)
        }

        async fn cleanup_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn clear_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn recover_jobs(
            &self,
            _kind: JobKind,
            _stale_threshold_ms: i64,
        ) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn count_jobs(&self, _kind: JobKind) -> Result<i64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }
    }

    fn make_search_cfg(key: &str, query: &str) -> Config {
        let mut cfg = Config::test_default();
        cfg.command = CommandKind::Search;
        cfg.positional = vec![query.to_string()];
        cfg.tavily_api_key = key.to_string();
        cfg
    }

    fn make_ctx(runtime: Arc<dyn ServiceJobRuntime>) -> ServiceContext {
        ServiceContext::from_runtime(Arc::new(Config::test_default()), runtime)
    }

    #[tokio::test]
    async fn test_run_search_rejects_empty_tavily_key() {
        let cfg = make_search_cfg("", "rust async");
        let ctx = make_ctx(Arc::new(EnqueueCapture::new()));
        let err = run_search(&cfg, &ctx).await.unwrap_err();
        assert!(
            err.to_string().contains("TAVILY_API_KEY"),
            "expected TAVILY_API_KEY error, got: {err}"
        );
    }

    #[tokio::test]
    async fn search_enqueue_failure_is_rejected_not_fatal() {
        let cfg = make_search_cfg("tvly-key", "rust programming language");
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
            "expected queue error in rejected output: {:?}",
            output.rejected
        );
    }

    #[tokio::test]
    async fn search_auto_crawl_uses_hardened_single_page_config() {
        let mut cfg = make_search_cfg("tvly-key", "rust programming language");
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
        assert_eq!(payloads.len(), 1);
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
    async fn search_auto_crawl_rejects_invalid_missing_and_duplicate_urls() {
        let cfg = make_search_cfg("tvly-key", "rust programming language");
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
                .all(|rejection| matches!(rejection.kind, SearchCrawlRejectionKind::InvalidUrl))
        );
        assert!(matches!(
            output.rejected[4].kind,
            SearchCrawlRejectionKind::DuplicateUrl
        ));
        assert_eq!(runtime.payloads().len(), 1);
    }

    #[test]
    fn search_cfg_time_range_defaults_to_none() {
        let cfg = make_search_cfg("tvly-key", "rust async");
        assert!(
            cfg.search_time_range.is_none(),
            "search_time_range should default to None"
        );
    }

    #[test]
    fn search_crawl_config_preserves_wait_mode() {
        let mut cfg = make_search_cfg("tvly-key", "rust async");
        cfg.wait = true;

        let search_cfg = search_crawl_config(&cfg);

        assert!(search_cfg.wait);
        assert_eq!(search_cfg.max_pages, 1);
        assert_eq!(search_cfg.max_depth, 1);
    }

    #[test]
    fn parse_search_time_range_supports_known_values() {
        assert!(matches!(
            parse_service_time_range(Some("day")),
            Some(ServiceTimeRange::Day)
        ));
        assert!(matches!(
            parse_service_time_range(Some("week")),
            Some(ServiceTimeRange::Week)
        ));
        assert!(matches!(
            parse_service_time_range(Some("month")),
            Some(ServiceTimeRange::Month)
        ));
        assert!(matches!(
            parse_service_time_range(Some("year")),
            Some(ServiceTimeRange::Year)
        ));
    }

    #[test]
    fn parse_search_time_range_rejects_unknown_values() {
        assert!(parse_service_time_range(Some("decade")).is_none());
        assert!(parse_service_time_range(Some("")).is_none());
        assert!(parse_service_time_range(None).is_none());
    }
}
