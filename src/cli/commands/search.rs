use crate::cli::commands::common::parse_service_time_range;
use crate::cli::commands::resolve_input_text;
use crate::core::config::Config;
use crate::core::http::validate_url_with_dns;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::core::ui::{muted, primary, print_phase};
use crate::services::context::ServiceContext;
use crate::services::crawl as crawl_service;
use crate::services::search::search_batch;
use crate::services::types::SearchOptions as ServiceSearchOptions;
use serde_json::Value;
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

    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
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

    if !cfg.quiet {
        log_done(&format!(
            "command=search complete query_len={} results={} duration_ms={duration_ms}",
            query.len(),
            results.len()
        ));
    }
    Ok(())
}

struct SearchCrawlOutput {
    jobs: Vec<Value>,
    rejected: Vec<Value>,
}

fn search_crawl_config(cfg: &Config) -> Config {
    // SECURITY: clear headers so auth meant for the search caller is never replayed
    // against URLs returned by Tavily.
    let mut search_cfg = cfg.clone();
    search_cfg.max_pages = 1;
    search_cfg.max_depth = 1;
    search_cfg.wait = false;
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
    let mut output = SearchCrawlOutput {
        jobs: Vec::new(),
        rejected: Vec::new(),
    };

    for result in results {
        let Some(url) = result["url"].as_str().filter(|url| !url.is_empty()) else {
            continue;
        };
        enqueue_search_crawl_url(&search_cfg, service_context, url, &mut output).await;
    }

    output
}

async fn enqueue_search_crawl_url(
    search_cfg: &Config,
    service_context: &ServiceContext,
    url: &str,
    output: &mut SearchCrawlOutput,
) {
    if let Err(error) = validate_url_with_dns(url).await {
        if !search_cfg.quiet {
            log_warn(&format!("search auto-index: skipped invalid URL: {error}"));
        }
        output
            .rejected
            .push(serde_json::json!({"url": url, "reason": error.to_string()}));
        return;
    }

    let url = url.to_string();
    match crawl_service::crawl_start_with_context(
        search_cfg,
        std::slice::from_ref(&url),
        service_context,
        None,
    )
    .await
    {
        Ok(outcome) => {
            if let Some(job) = outcome.result.jobs.first() {
                output
                    .jobs
                    .push(serde_json::json!({"url": url, "job_id": job.job_id}));
            }
        }
        Err(error) => {
            let reason = error.to_string();
            tracing::warn!(url = %url, error = %reason, "search auto-index: enqueue failed");
            output
                .rejected
                .push(serde_json::json!({"url": url, "reason": reason}));
        }
    }
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
    if !output.rejected.is_empty() && !cfg.quiet {
        log_warn(&format!(
            "search auto-index: {} URL(s) could not be queued",
            output.rejected.len()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::CommandKind;
    use crate::jobs::backend::{BackendResult, JobKind, JobPayload};
    use crate::services::runtime::ServiceJobRuntime;
    use crate::services::types::ServiceJob;
    use spider_agent::TimeRange;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    struct EnqueueCapture {
        calls: Mutex<Vec<String>>,
        fail: bool,
    }

    impl EnqueueCapture {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail: true,
            }
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
            if let JobPayload::Crawl { url, .. } = &payload {
                self.calls.lock().unwrap().push(url.clone());
            }
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

    fn make_ctx(runtime: impl ServiceJobRuntime + 'static) -> ServiceContext {
        ServiceContext::from_runtime(Arc::new(Config::test_default()), Arc::new(runtime))
    }

    fn parse_search_time_range(value: Option<&str>) -> Option<TimeRange> {
        match value.map(str::trim).filter(|v| !v.is_empty()) {
            Some("day") => Some(TimeRange::Day),
            Some("week") => Some(TimeRange::Week),
            Some("month") => Some(TimeRange::Month),
            Some("year") => Some(TimeRange::Year),
            Some(other) => {
                log_warn(&format!("Unknown search_time_range '{other}'; ignoring"));
                None
            }
            None => None,
        }
    }

    #[tokio::test]
    async fn test_run_search_rejects_empty_tavily_key() {
        let cfg = make_search_cfg("", "rust async");
        let ctx = make_ctx(EnqueueCapture::new());
        let err = run_search(&cfg, &ctx).await.unwrap_err();
        assert!(
            err.to_string().contains("TAVILY_API_KEY"),
            "expected TAVILY_API_KEY error, got: {err}"
        );
    }

    #[tokio::test]
    async fn search_enqueue_failure_is_rejected_not_fatal() {
        let cfg = make_search_cfg("tvly-key", "rust programming language");
        let ctx = make_ctx(EnqueueCapture::failing());
        let results = vec![serde_json::json!({
            "url": "http://93.184.216.34/",
            "title": "Example",
            "position": 1,
        })];

        let output = enqueue_search_crawls(&cfg, &ctx, &results).await;

        assert!(output.jobs.is_empty());
        assert_eq!(output.rejected.len(), 1);
        assert!(
            output.rejected[0]["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("queue cap exceeded")),
            "expected queue error in rejected output: {:?}",
            output.rejected
        );
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
    fn parse_search_time_range_supports_known_values() {
        assert!(matches!(
            parse_search_time_range(Some("day")),
            Some(TimeRange::Day)
        ));
        assert!(matches!(
            parse_search_time_range(Some("week")),
            Some(TimeRange::Week)
        ));
        assert!(matches!(
            parse_search_time_range(Some("month")),
            Some(TimeRange::Month)
        ));
        assert!(matches!(
            parse_search_time_range(Some("year")),
            Some(TimeRange::Year)
        ));
    }

    #[test]
    fn parse_search_time_range_rejects_unknown_values() {
        assert!(parse_search_time_range(Some("decade")).is_none());
        assert!(parse_search_time_range(Some("")).is_none());
        assert!(parse_search_time_range(None).is_none());
    }
}
