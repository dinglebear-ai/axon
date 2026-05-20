use super::config::Config;
use super::enums::{
    ClientMode, CommandKind, EvaluateResponsesMode, MapFallback, McpTransport, PerformanceProfile,
    RedditSort, RedditTime, RenderMode, ScrapeFormat,
};
use super::subconfigs::AskConfig;
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
            retrieve_max_points: None,
            train_best_rank: None,
            train_notes: None,
            max_pages: 0,
            max_depth: 10,
            include_subdomains: false,
            exclude_path_prefix: Vec::new(),
            output_dir: PathBuf::from(".cache/axon-rust/output"),
            output_path: None,
            render_mode: RenderMode::AutoSwitch,
            chrome_remote_url: None,
            chrome_proxy: None,
            user_agent: None,
            chrome_user_agent: None,
            chrome_bootstrap_timeout_ms: 3_000,
            chrome_bootstrap_retries: 2,
            respect_robots: false,
            min_markdown_chars: 200,
            drop_thin_markdown: true,
            discover_sitemaps: true,
            sitemap_since_days: 0,
            map_fallback: MapFallback::Structure,
            endpoints_include_bundles: true,
            endpoints_first_party_only: false,
            endpoints_unique_only: true,
            endpoints_max_scripts: 40,
            endpoints_max_scan_bytes: 8 * 1024 * 1024,
            endpoints_verify: false,
            endpoints_capture_network: false,
            max_sitemaps: 512,
            cache: true,
            cache_http_only: false,
            format: ScrapeFormat::Markdown,
            collection: "axon".to_string(),
            embed: true,
            batch_concurrency: 16,
            wait: false,
            sqlite_path: crate::core::paths::axon_data_base_dir().join("jobs.db"),
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
            headless_gemini_model: String::new(),
            headless_gemini_cmd: "gemini".to_string(),
            headless_gemini_home: None,
            llm_completion_concurrency: 4,
            llm_completion_timeout_secs: 300,
            tavily_api_key: String::new(),
            mcp_allowed_origins: vec![],
            ask_diagnostics: false,
            ask_explain: false,
            ask_stream: false,
            ask_follow_up: false,
            ask_session: None,
            ask_follow_up_context: None,
            ask_reset_session: false,
            ask_new_session: false,
            ask_list_sessions: false,
            evaluate_responses_mode: EvaluateResponsesMode::Inline,
            ask_max_context_chars: 300_000,
            ask_candidate_limit: 250,
            ask_chunk_limit: 20,
            ask_full_docs: 6,
            ask_full_docs_explicit: false,
            ask_backfill_chunks: 5,
            ask_doc_fetch_concurrency: 4,
            ask_doc_chunk_limit: 96,
            ask_min_relevance_score: 0.45,
            ask_authoritative_domains: vec![],
            ask_authoritative_boost: 0.0,
            ask_min_citations_nontrivial: 2,
            hybrid_search_enabled: true,
            hybrid_search_candidates: 100,
            ask_hybrid_candidates: 150,
            ask_cache_enabled: false,
            ask_cache_max_capacity_bytes: 256 * 1024 * 1024,
            ask_cache_ttl_secs: 300,
            ask_fulldoc_skip_enabled: false,
            ask_fulldoc_skip_min_urls: 3,
            ask_fulldoc_skip_min_chars: 4000,
            ask_fulldoc_skip_score_delta: 0.15,
            tei_max_retries: 5,
            tei_request_timeout_ms: 30_000,
            tei_max_client_batch_size: 64,
            ingest_lanes: 2,
            embed_lanes: 2,
            embed_doc_timeout_secs: 300,
            queue_summary_secs: 30,
            qdrant_point_buffer: 256,
            max_pending_crawl_jobs: 100,
            max_pending_embed_jobs: 50,
            max_pending_extract_jobs: 50,
            max_pending_ingest_jobs: 50,
            hnsw_ef_search: 128,
            hnsw_ef_search_legacy: 64,
            evaluate_retrieval_ab: false,
            cron_every_seconds: None,
            cron_max_runs: None,
            watchdog_stale_timeout_secs: 300,
            watchdog_confirm_secs: 60,
            watchdog_sweep_secs: 15,
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
            sources_by_schema_version: false,
            bypass_csp: false,
            accept_invalid_certs: false,
            screenshot_full_page: true,
            viewport_width: 1920,
            viewport_height: 1080,
            mcp_transport: McpTransport::Stdio,
            mcp_http_host: "127.0.0.1".to_string(),
            mcp_http_port: 8001,
            custom_headers: vec![],
            quiet: false,
            log_level: None,
            client_mode: ClientMode::Local,
            local_mode: false,
            server_url: None,
            job_wait_timeout_secs: 300,

            // ── Webclaw port (axon_rust-zehr) ─────────────────────────────
            enable_verticals: true,
            auto_dispatch_skip: Vec::new(),
            vertical_cache_ttl_secs: {
                let mut m = std::collections::HashMap::new();
                m.insert("github".to_string(), 86_400);
                m.insert("reddit".to_string(), 3_600);
                m.insert("hn".to_string(), 21_600);
                m
            },
            structured_data_max_bytes: 65_536,
            ladder_word_threshold_strategy1: 30,
            ladder_word_threshold_strategy2: 200,
            ladder_body_multiplier: 2.0,
            antibot_cookie_warmup: true,
            antibot_max_body_scan_bytes: 150_000,
        }
    }
}

impl Config {
    pub(crate) fn ask_config(&self) -> AskConfig {
        AskConfig {
            ask_max_context_chars: self.ask_max_context_chars,
            ask_candidate_limit: self.ask_candidate_limit,
            ask_chunk_limit: self.ask_chunk_limit,
            ask_full_docs: self.ask_full_docs,
            ask_backfill_chunks: self.ask_backfill_chunks,
            ask_doc_fetch_concurrency: self.ask_doc_fetch_concurrency,
            ask_doc_chunk_limit: self.ask_doc_chunk_limit,
            ask_min_relevance_score: self.ask_min_relevance_score,
            ask_authoritative_domains: self.ask_authoritative_domains.clone(),
            ask_authoritative_boost: self.ask_authoritative_boost,
            ask_min_citations_nontrivial: self.ask_min_citations_nontrivial,
            ask_diagnostics: self.ask_diagnostics,
            ask_hybrid_candidates: self.ask_hybrid_candidates,
        }
    }
}

impl Config {
    /// Construct a minimal `Config` with default tuning applied.
    pub fn default_minimal() -> Self {
        let mut cfg = Self::default();
        crate::core::config::parse::tuning::apply_default_minimal_tuning(&mut cfg);
        cfg
    }
}

#[cfg(test)]
impl Config {
    /// Construct a minimal `Config` suitable for unit tests.
    /// Uses `Config::default()` as the base so new non-Option fields
    /// do not require manual updates across test helpers.
    pub fn test_default() -> Self {
        Self {
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
            .field("retrieve_max_points", &self.retrieve_max_points)
            .field("train_best_rank", &self.train_best_rank)
            .field("train_notes", &self.train_notes)
            .field("max_pages", &self.max_pages)
            .field("max_depth", &self.max_depth)
            .field("include_subdomains", &self.include_subdomains)
            .field("exclude_path_prefix", &self.exclude_path_prefix)
            .field("output_dir", &self.output_dir)
            .field("output_path", &self.output_path)
            .field("render_mode", &self.render_mode)
            .field("chrome_remote_url", &self.chrome_remote_url)
            .field("chrome_proxy", &self.chrome_proxy)
            .field("user_agent", &self.user_agent)
            .field("chrome_user_agent", &self.chrome_user_agent)
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
            .field("map_fallback", &self.map_fallback)
            .field("endpoints_include_bundles", &self.endpoints_include_bundles)
            .field(
                "endpoints_first_party_only",
                &self.endpoints_first_party_only,
            )
            .field("endpoints_unique_only", &self.endpoints_unique_only)
            .field("endpoints_max_scripts", &self.endpoints_max_scripts)
            .field("endpoints_max_scan_bytes", &self.endpoints_max_scan_bytes)
            .field("endpoints_verify", &self.endpoints_verify)
            .field("endpoints_capture_network", &self.endpoints_capture_network)
            .field("max_sitemaps", &self.max_sitemaps)
            .field("cache", &self.cache)
            .field("cache_http_only", &self.cache_http_only)
            .field("format", &self.format)
            .field("collection", &self.collection)
            .field("embed", &self.embed)
            .field("batch_concurrency", &self.batch_concurrency)
            .field("wait", &self.wait)
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
            .field("headless_gemini_model", &self.headless_gemini_model)
            .field("headless_gemini_cmd", &self.headless_gemini_cmd)
            .field("headless_gemini_home", &self.headless_gemini_home)
            .field(
                "llm_completion_concurrency",
                &self.llm_completion_concurrency,
            )
            .field(
                "llm_completion_timeout_secs",
                &self.llm_completion_timeout_secs,
            )
            .field("tavily_api_key", &"[REDACTED]")
            .field("mcp_allowed_origins", &self.mcp_allowed_origins)
            .field("ask_diagnostics", &self.ask_diagnostics)
            .field("ask_explain", &self.ask_explain)
            .field("ask_stream", &self.ask_stream)
            .field("ask_follow_up", &self.ask_follow_up)
            .field("ask_session", &self.ask_session)
            .field(
                "ask_follow_up_context",
                &self.ask_follow_up_context.as_ref().map(|_| "[REDACTED]"),
            )
            .field("ask_reset_session", &self.ask_reset_session)
            .field("ask_new_session", &self.ask_new_session)
            .field("ask_list_sessions", &self.ask_list_sessions)
            .field("evaluate_responses_mode", &self.evaluate_responses_mode)
            .field("ask_max_context_chars", &self.ask_max_context_chars)
            .field("ask_candidate_limit", &self.ask_candidate_limit)
            .field("ask_chunk_limit", &self.ask_chunk_limit)
            .field("ask_full_docs", &self.ask_full_docs)
            .field("ask_full_docs_explicit", &self.ask_full_docs_explicit)
            .field("ask_backfill_chunks", &self.ask_backfill_chunks)
            .field("ask_doc_fetch_concurrency", &self.ask_doc_fetch_concurrency)
            .field("ask_doc_chunk_limit", &self.ask_doc_chunk_limit)
            .field("ask_min_relevance_score", &self.ask_min_relevance_score)
            .field("ask_authoritative_domains", &self.ask_authoritative_domains)
            .field("ask_authoritative_boost", &self.ask_authoritative_boost)
            .field(
                "ask_min_citations_nontrivial",
                &self.ask_min_citations_nontrivial,
            )
            .field("hybrid_search_enabled", &self.hybrid_search_enabled)
            .field("hybrid_search_candidates", &self.hybrid_search_candidates)
            .field("ask_hybrid_candidates", &self.ask_hybrid_candidates)
            .field("tei_max_retries", &self.tei_max_retries)
            .field("tei_request_timeout_ms", &self.tei_request_timeout_ms)
            .field("tei_max_client_batch_size", &self.tei_max_client_batch_size)
            .field("ingest_lanes", &self.ingest_lanes)
            .field("embed_lanes", &self.embed_lanes)
            .field("embed_doc_timeout_secs", &self.embed_doc_timeout_secs)
            .field("queue_summary_secs", &self.queue_summary_secs)
            .field("qdrant_point_buffer", &self.qdrant_point_buffer)
            .field("max_pending_crawl_jobs", &self.max_pending_crawl_jobs)
            .field("max_pending_embed_jobs", &self.max_pending_embed_jobs)
            .field("max_pending_extract_jobs", &self.max_pending_extract_jobs)
            .field("max_pending_ingest_jobs", &self.max_pending_ingest_jobs)
            .field("hnsw_ef_search", &self.hnsw_ef_search)
            .field("hnsw_ef_search_legacy", &self.hnsw_ef_search_legacy)
            .field("evaluate_retrieval_ab", &self.evaluate_retrieval_ab)
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
            .field("job_wait_timeout_secs", &self.job_wait_timeout_secs)
            .field("client_mode", &self.client_mode)
            .field("local_mode", &self.local_mode)
            .field(
                "server_url",
                &self.server_url.as_ref().map(|url| {
                    if url.password().is_some() || !url.username().is_empty() {
                        format!(
                            "{}://[REDACTED]@{}{}",
                            url.scheme(),
                            url.host_str().unwrap_or(""),
                            url.path()
                        )
                    } else {
                        url.to_string()
                    }
                }),
            )
            .finish()
    }
}
