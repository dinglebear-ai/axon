use super::config::Config;
use super::enums::{
    CommandKind, EvaluateResponsesMode, McpTransport, PerformanceProfile, RedditSort, RedditTime,
    RenderMode, ScrapeFormat,
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
            cache: true,
            cache_skip_browser: false,
            format: ScrapeFormat::Markdown,
            collection: "cortex".to_string(),
            embed: true,
            batch_concurrency: 16,
            wait: false,
            yes: false,
            performance_profile: PerformanceProfile::HighStable,
            crawl_concurrency_limit: None,
            backfill_concurrency_limit: None,
            sitemap_only: false,
            delay_ms: 0,
            request_timeout_ms: None,
            fetch_retries: 2,
            retry_backoff_ms: 250,
            shared_queue: true,
            pg_url: String::new(),
            redis_url: String::new(),
            amqp_url: String::new(),
            crawl_queue: "axon.crawl.jobs".to_string(),
            refresh_queue: "axon.refresh.jobs".to_string(),
            extract_queue: "axon.extract.jobs".to_string(),
            embed_queue: "axon.embed.jobs".to_string(),
            ingest_queue: "axon.ingest.jobs".to_string(),
            sessions_claude: false,
            sessions_codex: false,
            sessions_gemini: false,
            sessions_project: None,
            github_token: None,
            github_include_source: true,
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
            tavily_api_key: String::new(),
            neo4j_url: String::new(),
            neo4j_user: "neo4j".to_string(),
            neo4j_password: String::new(),
            graph_queue: "axon.graph.jobs".to_string(),
            graph_concurrency: 4,
            graph_llm_url: "http://localhost:11434".to_string(),
            graph_llm_model: "qwen3.5:2b".to_string(),
            graph_similarity_threshold: 0.75,
            graph_similarity_limit: 20,
            graph_context_max_chars: 2_000,
            graph_taxonomy_path: String::new(),
            web_allowed_origins: vec![],
            shell_allowed_origins: vec![],
            ask_diagnostics: false,
            ask_graph: false,
            evaluate_responses_mode: EvaluateResponsesMode::Inline,
            ask_max_context_chars: 120_000,
            ask_candidate_limit: 64,
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
            normalize: false,
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
            bypass_csp: false,
            accept_invalid_certs: false,
            screenshot_full_page: true,
            viewport_width: 1920,
            viewport_height: 1080,
            serve_port: 49000,
            mcp_transport: McpTransport::Http,
            mcp_http_host: "127.0.0.1".to_string(),
            mcp_http_port: 8001,
            custom_headers: vec![],
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
            .field("shared_queue", &self.shared_queue)
            .field("pg_url", &"[REDACTED]")
            .field("redis_url", &"[REDACTED]")
            .field("amqp_url", &"[REDACTED]")
            .field("crawl_queue", &self.crawl_queue)
            .field("refresh_queue", &self.refresh_queue)
            .field("extract_queue", &self.extract_queue)
            .field("embed_queue", &self.embed_queue)
            .field("ingest_queue", &self.ingest_queue)
            .field("sessions_claude", &self.sessions_claude)
            .field("sessions_codex", &self.sessions_codex)
            .field("sessions_gemini", &self.sessions_gemini)
            .field("sessions_project", &self.sessions_project)
            .field("github_token", &"[REDACTED]")
            .field("github_include_source", &self.github_include_source)
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
            .field("tavily_api_key", &"[REDACTED]")
            .field("neo4j_url", &"[REDACTED]")
            .field("neo4j_user", &self.neo4j_user)
            .field("neo4j_password", &"[REDACTED]")
            .field("graph_queue", &self.graph_queue)
            .field("graph_concurrency", &self.graph_concurrency)
            .field("graph_llm_url", &self.graph_llm_url)
            .field("graph_llm_model", &self.graph_llm_model)
            .field(
                "graph_similarity_threshold",
                &self.graph_similarity_threshold,
            )
            .field("graph_similarity_limit", &self.graph_similarity_limit)
            .field("graph_context_max_chars", &self.graph_context_max_chars)
            .field("graph_taxonomy_path", &self.graph_taxonomy_path)
            .field("web_allowed_origins", &self.web_allowed_origins)
            .field("shell_allowed_origins", &self.shell_allowed_origins)
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
            .field("cron_every_seconds", &self.cron_every_seconds)
            .field("cron_max_runs", &self.cron_max_runs)
            .field(
                "watchdog_stale_timeout_secs",
                &self.watchdog_stale_timeout_secs,
            )
            .field("watchdog_confirm_secs", &self.watchdog_confirm_secs)
            .field("json_output", &self.json_output)
            .field("reclaimed_status_only", &self.reclaimed_status_only)
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
            .field("bypass_csp", &self.bypass_csp)
            .field("accept_invalid_certs", &self.accept_invalid_certs)
            .field("screenshot_full_page", &self.screenshot_full_page)
            .field("viewport_width", &self.viewport_width)
            .field("viewport_height", &self.viewport_height)
            .field("serve_port", &self.serve_port)
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
            .finish()
    }
}
