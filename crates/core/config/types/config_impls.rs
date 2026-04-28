use super::config::Config;
use super::enums::{
    CommandKind, EvaluateResponsesMode, MapFallback, McpTransport, PerformanceProfile, RedditSort,
    RedditTime, RenderMode, ScrapeFormat,
};
use std::env;
use std::fmt;
use std::path::PathBuf;

impl Default for Config {
    fn default() -> Self {
        Self {
            command: CommandKind::Status,
            start_url: String::new(),
            positional: Vec::new(),
            urls_csv: None,
            url_glob: Vec::new(),
            query: None,
            search_limit: 10,
            max_pages: 0,
            max_depth: 5,
            include_subdomains: false,
            exclude_path_prefix: Vec::new(),
            output_dir: PathBuf::from(".cache/axon-rust/output"),
            output_path: None,
            export_include_history: false,
            export_verify_input: None,
            render_mode: RenderMode::AutoSwitch,
            chrome_remote_url: None,
            chrome_proxy: None,
            chrome_user_agent: None,
            chrome_headless: true,
            chrome_anti_bot: true,
            chrome_intercept: true,
            chrome_stealth: true,
            chrome_bootstrap: true,
            chrome_bootstrap_timeout_ms: 3_000,
            chrome_bootstrap_retries: 2,
            respect_robots: false,
            min_markdown_chars: 200,
            drop_thin_markdown: true,
            discover_sitemaps: true,
            sitemap_since_days: 0,
            map_fallback: MapFallback::Structure,
            max_sitemaps: 512,
            cache: true,
            cache_skip_browser: false,
            format: ScrapeFormat::Markdown,
            collection: "cortex".to_string(),
            embed: true,
            batch_concurrency: 16,
            wait: false,
            lite_mode: false,
            sqlite_path: crate::crates::core::paths::axon_data_base_dir()
                .join("axon")
                .join("jobs.db"),
            yes: false,
            performance_profile: PerformanceProfile::HighStable,
            crawl_concurrency_limit: None,
            backfill_concurrency_limit: None,
            sitemap_only: false,
            delay_ms: 0,
            request_timeout_ms: None,
            fetch_retries: 2,
            retry_backoff_ms: 250,
            sessions_claude: false,
            sessions_codex: false,
            sessions_gemini: false,
            sessions_project: None,
            github_token: None,
            github_include_source: true,
            github_max_issues: 100,
            github_max_prs: 100,
            reddit_client_id: None,
            reddit_client_secret: None,
            reddit_sort: RedditSort::Hot,
            reddit_time: RedditTime::Day,
            reddit_max_posts: 25,
            reddit_min_score: 0,
            reddit_depth: 2,
            reddit_scrape_links: false,
            tei_url: String::new(),
            qdrant_url: "http://127.0.0.1:53333".to_string(),
            openai_base_url: String::new(),
            openai_api_key: String::new(),
            openai_model: String::new(),
            acp_adapter_cmd: None,
            acp_adapter_args: None,
            acp_prewarm: true,
            acp_ws_url: None,
            acp_ws_token: None,
            tavily_api_key: String::new(),
            mcp_allowed_origins: vec![],
            ask_diagnostics: false,
            ask_graph: false,
            evaluate_responses_mode: EvaluateResponsesMode::Inline,
            ask_max_context_chars: 120_000,
            ask_candidate_limit: 150,
            ask_chunk_limit: 10,
            ask_full_docs: 4,
            ask_backfill_chunks: 3,
            ask_doc_fetch_concurrency: 4,
            ask_doc_chunk_limit: 192,
            ask_min_relevance_score: 0.45,
            ask_authoritative_domains: vec![],
            ask_authoritative_boost: 0.0,
            ask_authoritative_allowlist: vec![],
            ask_min_citations_nontrivial: 2,
            hybrid_search_enabled: true,
            hybrid_search_candidates: 100,
            ask_hybrid_candidates: 150,
            cron_every_seconds: None,
            cron_max_runs: None,
            watchdog_stale_timeout_secs: env::var("AXON_JOB_STALE_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
            watchdog_confirm_secs: env::var("AXON_JOB_STALE_CONFIRM_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            json_output: false,
            reclaimed_status_only: false,
            active_status_only: false,
            recent_status_only: false,
            normalize: true,
            chrome_network_idle_timeout_secs: 15,
            auto_switch_thin_ratio: 0.60,
            auto_switch_min_pages: 10,
            crawl_broadcast_buffer_min: 4096,
            crawl_broadcast_buffer_max: 16_384,
            url_whitelist: vec![],
            block_assets: false,
            max_page_bytes: None,
            redirect_policy_strict: false,
            chrome_wait_for_selector: None,
            root_selector: None,
            exclude_selector: None,
            chrome_screenshot: false,
            research_depth: None,
            search_time_range: None,
            since: None,
            before: None,
            bypass_csp: false,
            accept_invalid_certs: false,
            screenshot_full_page: true,
            viewport_width: 1920,
            viewport_height: 1080,
            mcp_transport: McpTransport::Http,
            mcp_http_host: "0.0.0.0".to_string(),
            mcp_http_port: 8001,
            custom_headers: vec![],
            quiet: false,
        }
    }
}

impl Config {
    /// A minimal Config used by LiteBackend — lite mode enabled, no external services required.
    pub fn default_lite() -> Self {
        Self {
            lite_mode: true,
            ..Default::default()
        }
    }
}

#[cfg(test)]
impl Config {
    /// Construct a minimal `Config` suitable for unit tests.
    /// Uses `Config::default()` as the base so new non-Option fields
    /// do not require manual updates across test helpers.
    pub fn test_default() -> Self {
        Self {
            openai_base_url: "http://localhost:11434/v1".to_string(),
            openai_model: "test-model".to_string(),
            tavily_api_key: "test-key".to_string(),
            ..Default::default()
        }
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("command", &self.command)
            .field("start_url", &self.start_url)
            .field("positional", &self.positional)
            .field("urls_csv", &self.urls_csv)
            .field("url_glob", &self.url_glob)
            .field("query", &self.query)
            .field("search_limit", &self.search_limit)
            .field("max_pages", &self.max_pages)
            .field("max_depth", &self.max_depth)
            .field("include_subdomains", &self.include_subdomains)
            .field("exclude_path_prefix", &self.exclude_path_prefix)
            .field("output_dir", &self.output_dir)
            .field("output_path", &self.output_path)
            .field("export_include_history", &self.export_include_history)
            .field("export_verify_input", &self.export_verify_input)
            .field("render_mode", &self.render_mode)
            .field("chrome_remote_url", &self.chrome_remote_url)
            .field("chrome_proxy", &self.chrome_proxy)
            .field("chrome_user_agent", &self.chrome_user_agent)
            .field("chrome_headless", &self.chrome_headless)
            .field("chrome_anti_bot", &self.chrome_anti_bot)
            .field("chrome_intercept", &self.chrome_intercept)
            .field("chrome_stealth", &self.chrome_stealth)
            .field("chrome_bootstrap", &self.chrome_bootstrap)
            .field(
                "chrome_bootstrap_timeout_ms",
                &self.chrome_bootstrap_timeout_ms,
            )
            .field("chrome_bootstrap_retries", &self.chrome_bootstrap_retries)
            .field("respect_robots", &self.respect_robots)
            .field("min_markdown_chars", &self.min_markdown_chars)
            .field("drop_thin_markdown", &self.drop_thin_markdown)
            .field("discover_sitemaps", &self.discover_sitemaps)
            .field("sitemap_since_days", &self.sitemap_since_days)
            .field("cache", &self.cache)
            .field("cache_skip_browser", &self.cache_skip_browser)
            .field("format", &self.format)
            .field("collection", &self.collection)
            .field("embed", &self.embed)
            .field("batch_concurrency", &self.batch_concurrency)
            .field("wait", &self.wait)
            .field("lite_mode", &self.lite_mode)
            .field("sqlite_path", &self.sqlite_path)
            .field("yes", &self.yes)
            .field("performance_profile", &self.performance_profile)
            .field("crawl_concurrency_limit", &self.crawl_concurrency_limit)
            .field(
                "backfill_concurrency_limit",
                &self.backfill_concurrency_limit,
            )
            .field("sitemap_only", &self.sitemap_only)
            .field("delay_ms", &self.delay_ms)
            .field("request_timeout_ms", &self.request_timeout_ms)
            .field("fetch_retries", &self.fetch_retries)
            .field("retry_backoff_ms", &self.retry_backoff_ms)
            .field("sessions_claude", &self.sessions_claude)
            .field("sessions_codex", &self.sessions_codex)
            .field("sessions_gemini", &self.sessions_gemini)
            .field("sessions_project", &self.sessions_project)
            .field("github_token", &"[REDACTED]")
            .field("github_include_source", &self.github_include_source)
            .field("github_max_issues", &self.github_max_issues)
            .field("github_max_prs", &self.github_max_prs)
            .field("reddit_client_id", &"[REDACTED]")
            .field("reddit_client_secret", &"[REDACTED]")
            .field("reddit_sort", &self.reddit_sort)
            .field("reddit_time", &self.reddit_time)
            .field("reddit_max_posts", &self.reddit_max_posts)
            .field("reddit_min_score", &self.reddit_min_score)
            .field("reddit_depth", &self.reddit_depth)
            .field("reddit_scrape_links", &self.reddit_scrape_links)
            .field("tei_url", &self.tei_url)
            .field("qdrant_url", &self.qdrant_url)
            .field("openai_base_url", &self.openai_base_url)
            .field("openai_api_key", &"[REDACTED]")
            .field("openai_model", &self.openai_model)
            .field("acp_adapter_cmd", &self.acp_adapter_cmd)
            .field("acp_adapter_args", &self.acp_adapter_args)
            .field("acp_prewarm", &self.acp_prewarm)
            .field("acp_ws_url", &self.acp_ws_url)
            .field("acp_ws_token", &"[REDACTED]")
            .field("tavily_api_key", &"[REDACTED]")
            .field("mcp_allowed_origins", &self.mcp_allowed_origins)
            .field("ask_diagnostics", &self.ask_diagnostics)
            .field("ask_graph", &self.ask_graph)
            .field("evaluate_responses_mode", &self.evaluate_responses_mode)
            .field("ask_max_context_chars", &self.ask_max_context_chars)
            .field("ask_candidate_limit", &self.ask_candidate_limit)
            .field("ask_chunk_limit", &self.ask_chunk_limit)
            .field("ask_full_docs", &self.ask_full_docs)
            .field("ask_backfill_chunks", &self.ask_backfill_chunks)
            .field("ask_doc_fetch_concurrency", &self.ask_doc_fetch_concurrency)
            .field("ask_doc_chunk_limit", &self.ask_doc_chunk_limit)
            .field("ask_min_relevance_score", &self.ask_min_relevance_score)
            .field("ask_authoritative_domains", &self.ask_authoritative_domains)
            .field("ask_authoritative_boost", &self.ask_authoritative_boost)
            .field(
                "ask_authoritative_allowlist",
                &self.ask_authoritative_allowlist,
            )
            .field(
                "ask_min_citations_nontrivial",
                &self.ask_min_citations_nontrivial,
            )
            .field("hybrid_search_enabled", &self.hybrid_search_enabled)
            .field("hybrid_search_candidates", &self.hybrid_search_candidates)
            .field("ask_hybrid_candidates", &self.ask_hybrid_candidates)
            .field("cron_every_seconds", &self.cron_every_seconds)
            .field("cron_max_runs", &self.cron_max_runs)
            .field(
                "watchdog_stale_timeout_secs",
                &self.watchdog_stale_timeout_secs,
            )
            .field("watchdog_confirm_secs", &self.watchdog_confirm_secs)
            .field("json_output", &self.json_output)
            .field("reclaimed_status_only", &self.reclaimed_status_only)
            .field("active_status_only", &self.active_status_only)
            .field("recent_status_only", &self.recent_status_only)
            .field("normalize", &self.normalize)
            .field(
                "chrome_network_idle_timeout_secs",
                &self.chrome_network_idle_timeout_secs,
            )
            .field("auto_switch_thin_ratio", &self.auto_switch_thin_ratio)
            .field("auto_switch_min_pages", &self.auto_switch_min_pages)
            .field(
                "crawl_broadcast_buffer_min",
                &self.crawl_broadcast_buffer_min,
            )
            .field(
                "crawl_broadcast_buffer_max",
                &self.crawl_broadcast_buffer_max,
            )
            .field("url_whitelist", &self.url_whitelist)
            .field("block_assets", &self.block_assets)
            .field("max_page_bytes", &self.max_page_bytes)
            .field("redirect_policy_strict", &self.redirect_policy_strict)
            .field("chrome_wait_for_selector", &self.chrome_wait_for_selector)
            .field("root_selector", &self.root_selector)
            .field("exclude_selector", &self.exclude_selector)
            .field("chrome_screenshot", &self.chrome_screenshot)
            .field("research_depth", &self.research_depth)
            .field("search_time_range", &self.search_time_range)
            .field("since", &self.since)
            .field("before", &self.before)
            .field("bypass_csp", &self.bypass_csp)
            .field("accept_invalid_certs", &self.accept_invalid_certs)
            .field("screenshot_full_page", &self.screenshot_full_page)
            .field("viewport_width", &self.viewport_width)
            .field("viewport_height", &self.viewport_height)
            .field("mcp_transport", &self.mcp_transport)
            .field("mcp_http_host", &self.mcp_http_host)
            .field("mcp_http_port", &self.mcp_http_port)
            .field(
                "custom_headers",
                &self
                    .custom_headers
                    .iter()
                    .map(|h| match h.split_once(": ") {
                        Some((name, _)) => format!("{name}: [REDACTED]"),
                        None => "[MALFORMED]".to_string(),
                    })
                    .collect::<Vec<_>>(),
            )
            .field("quiet", &self.quiet)
            .finish()
    }
}
