use crate::core::config::types::{PerformanceProfile, RenderMode, ScrapeFormat};
use clap::{ArgAction, Args};
use std::path::PathBuf;

pub(in crate::core::config) const DEFAULT_OUTPUT_DIR: &str = ".cache/axon-rust/output";

#[derive(Debug, Args)]
pub(in crate::core::config) struct GlobalArgs {
    #[arg(global = true, long, default_value = "")]
    pub(in crate::core::config) start_url: String,

    /// Maximum pages to crawl per job (0 = unlimited)
    #[arg(global = true, long, default_value_t = 0)]
    pub(in crate::core::config) max_pages: u32,

    /// Maximum crawl depth from the start URL
    #[arg(global = true, long, default_value_t = 10)]
    pub(in crate::core::config) max_depth: usize,

    /// Crawl all subdomains of the start URL's parent domain
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::core::config) include_subdomains: bool,

    /// URL path prefixes to exclude from crawling (comma-separated)
    #[arg(global = true, long = "exclude-path-prefix", value_delimiter = ',')]
    pub(in crate::core::config) exclude_path_prefix: Vec<String>,

    /// Directory for saved markdown/HTML output files
    #[arg(global = true, long, default_value = DEFAULT_OUTPUT_DIR)]
    pub(in crate::core::config) output_dir: PathBuf,

    /// Explicit output file path (overrides --output-dir)
    #[arg(global = true, long)]
    pub(in crate::core::config) output: Option<PathBuf>,

    /// Page fetch mode: http, chrome, or auto-switch
    #[arg(global = true, long, value_enum, default_value_t = RenderMode::AutoSwitch)]
    pub(in crate::core::config) render_mode: RenderMode,

    /// Chrome DevTools Protocol management endpoint URL
    #[arg(
        global = true,
        long,
        env = "AXON_CHROME_REMOTE_URL",
        hide_env_values = true
    )]
    pub(in crate::core::config) chrome_remote_url: Option<String>,

    /// HTTP proxy URL for Chrome requests
    #[arg(global = true, long, env = "AXON_CHROME_PROXY", hide_env_values = true)]
    pub(in crate::core::config) chrome_proxy: Option<String>,

    /// Custom User-Agent header for Chrome requests
    #[arg(
        global = true,
        long,
        env = "AXON_CHROME_USER_AGENT",
        hide_env_values = true
    )]
    pub(in crate::core::config) chrome_user_agent: Option<String>,

    /// Run Chrome in headless mode (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::core::config) chrome_headless: bool,

    /// Enable Chrome anti-bot evasion mode (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::core::config) chrome_anti_bot: bool,

    /// Enable Chrome network interception for blocking ads/trackers (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::core::config) chrome_intercept: bool,

    /// Enable Chrome stealth mode to patch navigator.webdriver (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::core::config) chrome_stealth: bool,

    /// Bootstrap Chrome connection before starting the crawl (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::core::config) chrome_bootstrap: bool,

    /// Timeout in milliseconds for Chrome bootstrap (default: 3000)
    #[arg(global = true, long, default_value_t = 3000)]
    pub(in crate::core::config) chrome_bootstrap_timeout_ms: u64,

    /// Number of retries for Chrome bootstrap failures (default: 2)
    #[arg(global = true, long, default_value_t = 2)]
    pub(in crate::core::config) chrome_bootstrap_retries: usize,

    /// Respect robots.txt directives (default: false)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::core::config) respect_robots: bool,

    /// Minimum content length; shorter pages are flagged thin (default: 200)
    #[arg(global = true, long, default_value_t = 200)]
    pub(in crate::core::config) min_markdown_chars: usize,

    /// Skip thin pages — do not save or embed them (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::core::config) drop_thin_markdown: bool,

    /// Discover and backfill URLs from sitemap.xml after crawl (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::core::config) discover_sitemaps: bool,

    /// Only backfill sitemap URLs with a `<lastmod>` date within the last N days (0 = no filter).
    /// URLs without a `<lastmod>` tag are always included.
    #[arg(global = true, long, default_value_t = 0)]
    pub(in crate::core::config) sitemap_since_days: u32,

    /// Maximum number of sitemap documents to parse per map/backfill operation
    /// (0 = unlimited, default: 512)
    #[arg(global = true, long, default_value_t = 512)]
    pub(in crate::core::config) max_sitemaps: usize,

    /// Enable crawl cache reuse. Disable with `--cache false`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::core::config) cache: bool,

    /// Skip cache for browser (Chrome) fetches only
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::core::config) cache_skip_browser: bool,

    /// Output format: markdown, html, rawHtml, json
    #[arg(global = true, long, value_enum, default_value_t = ScrapeFormat::Markdown)]
    pub(in crate::core::config) format: ScrapeFormat,

    /// Maximum number of results to return (default: 10)
    #[arg(global = true, long, default_value_t = 10)]
    pub(in crate::core::config) limit: usize,

    /// Query text (alternative to positional argument)
    #[arg(global = true, long)]
    pub(in crate::core::config) query: Option<String>,

    /// Comma-separated list of URLs (alternative to positional arguments)
    #[arg(global = true, long)]
    pub(in crate::core::config) urls: Option<String>,

    /// URL glob patterns to expand into seed URLs (comma-separated)
    #[arg(global = true, long = "url-glob", value_delimiter = ',')]
    pub(in crate::core::config) url_glob: Vec<String>,

    /// Auto-embed scraped content into Qdrant (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::core::config) embed: bool,

    /// Qdrant collection name (default: cortex)
    #[arg(
        global = true,
        long,
        env = "AXON_COLLECTION",
        hide_env_values = true,
        default_value = "cortex"
    )]
    pub(in crate::core::config) collection: String,

    /// Concurrent connections for batch operations (1-512)
    #[arg(global = true, long, default_value_t = 16)]
    pub(in crate::core::config) batch_concurrency: usize,

    /// Block until async job completes; false enqueues and returns (default: false)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::core::config) wait: bool,

    /// Compatibility flag. SQLite + in-process workers are always used.
    #[arg(global = true, long, default_value_t = false, hide = true)]
    pub(in crate::core::config) lite: bool,

    /// Path to the SQLite jobs database.
    #[arg(global = true, long, hide = true)]
    pub(in crate::core::config) sqlite_path: Option<PathBuf>,

    /// Skip confirmation prompts (non-interactive mode)
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::core::config) yes: bool,

    /// Output results as machine-readable JSON
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::core::config) json: bool,

    /// Compatibility flag. Graph retrieval is not available in production.
    #[arg(global = true, long, action = ArgAction::SetTrue, hide = true)]
    pub(in crate::core::config) graph: bool,

    /// Status mode: show only watchdog-reclaimed jobs.
    #[arg(
        global = true,
        long,
        action = ArgAction::SetTrue,
        conflicts_with_all = ["active", "recent"]
    )]
    pub(in crate::core::config) reclaimed: bool,

    /// Show only active jobs (running/pending) in list and status views.
    #[arg(global = true, long, action = ArgAction::SetTrue, conflicts_with = "recent")]
    pub(in crate::core::config) active: bool,

    /// Show active + completed jobs (hide failed/canceled) in list and status views.
    #[arg(global = true, long, action = ArgAction::SetTrue, conflicts_with = "active")]
    pub(in crate::core::config) recent: bool,

    /// Concurrency preset: high-stable, balanced, extreme, max
    #[arg(global = true, long, value_enum, default_value_t = PerformanceProfile::HighStable)]
    pub(in crate::core::config) performance_profile: PerformanceProfile,

    /// Override all concurrency limits (crawl, sitemap, backfill) at once
    #[arg(global = true, long)]
    pub(in crate::core::config) concurrency_limit: Option<usize>,

    /// Override crawl concurrency (default: from profile)
    #[arg(global = true, long)]
    pub(in crate::core::config) crawl_concurrency_limit: Option<usize>,

    /// Override backfill concurrency (default: from profile)
    #[arg(global = true, long)]
    pub(in crate::core::config) backfill_concurrency_limit: Option<usize>,

    /// Only run sitemap discovery, not a full crawl
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::core::config) sitemap_only: bool,

    /// Delay between requests in milliseconds (polite crawling)
    #[arg(global = true, long, default_value_t = 0)]
    pub(in crate::core::config) delay_ms: u64,

    /// Per-request HTTP timeout in milliseconds (default: from profile)
    #[arg(global = true, long)]
    pub(in crate::core::config) request_timeout_ms: Option<u64>,

    /// Number of retries on failed fetches (default: from profile)
    #[arg(global = true, long)]
    pub(in crate::core::config) fetch_retries: Option<usize>,

    /// Backoff between retries in milliseconds (default: from profile)
    #[arg(global = true, long)]
    pub(in crate::core::config) retry_backoff_ms: Option<u64>,

    /// Text Embeddings Inference server URL (overrides TEI_URL)
    #[arg(global = true, long)]
    pub(in crate::core::config) tei_url: Option<String>,

    /// Qdrant server URL (overrides QDRANT_URL)
    #[arg(global = true, long)]
    pub(in crate::core::config) qdrant_url: Option<String>,

    /// OpenAI-compatible API base URL (overrides OPENAI_BASE_URL)
    #[arg(global = true, long)]
    pub(in crate::core::config) openai_base_url: Option<String>,

    /// OpenAI API key (overrides OPENAI_API_KEY)
    #[arg(global = true, long)]
    pub(in crate::core::config) openai_api_key: Option<String>,

    /// Compatibility LLM model name. Gemini headless only reuses gemini-* values.
    #[arg(global = true, long)]
    pub(in crate::core::config) openai_model: Option<String>,

    /// Seconds before a running job is considered stale by the watchdog
    #[arg(
        global = true,
        long,
        env = "AXON_JOB_STALE_TIMEOUT_SECS",
        hide_env_values = true,
        default_value_t = 300
    )]
    pub(in crate::core::config) watchdog_stale_timeout_secs: i64,

    /// Additional grace period (seconds) before a stale job is reclaimed
    #[arg(
        global = true,
        long,
        env = "AXON_JOB_STALE_CONFIRM_SECS",
        hide_env_values = true,
        default_value_t = 60
    )]
    pub(in crate::core::config) watchdog_confirm_secs: i64,

    /// Seconds between periodic watchdog sweeps. Smaller = stale jobs are
    /// reclaimed sooner, larger = fewer SQL writes when nothing is stale.
    #[arg(
        global = true,
        long,
        env = "AXON_WATCHDOG_SWEEP_SECS",
        hide_env_values = true,
        default_value_t = 15
    )]
    pub(in crate::core::config) watchdog_sweep_secs: i64,

    /// Cron interval: re-run the command every N seconds
    #[arg(global = true, long)]
    pub(in crate::core::config) cron_every_seconds: Option<u64>,

    /// Stop cron after N runs (default: run forever)
    #[arg(global = true, long)]
    pub(in crate::core::config) cron_max_runs: Option<usize>,

    /// Deduplicate trailing-slash URL variants during crawl. Disable with `--normalize false`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::core::config) normalize: bool,

    /// Seconds to wait for Chrome network idle before page capture (default: 15)
    #[arg(global = true, long, default_value_t = 15)]
    pub(in crate::core::config) chrome_network_idle_timeout: u64,

    /// Thin-page ratio to trigger auto-switch to Chrome (0.0-1.0, default: 0.60)
    #[arg(global = true, long, default_value_t = 0.60)]
    pub(in crate::core::config) auto_switch_thin_ratio: f64,

    /// Minimum pages before auto-switch eligibility check (default: 10)
    #[arg(global = true, long, default_value_t = 10)]
    pub(in crate::core::config) auto_switch_min_pages: usize,

    /// Only crawl URLs matching these regex patterns (repeatable)
    #[arg(global = true, long)]
    pub(in crate::core::config) url_whitelist: Vec<String>,

    /// Block asset downloads (images/CSS/fonts) during crawl
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::core::config) block_assets: bool,

    /// Maximum response size per page in bytes (0 = unlimited)
    #[arg(global = true, long, default_value_t = 0)]
    pub(in crate::core::config) max_page_bytes: u64,

    /// Only follow same-origin redirects (strict redirect policy)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::core::config) redirect_policy_strict: bool,

    /// CSS selector to wait for before Chrome captures the page
    #[arg(global = true, long)]
    pub(in crate::core::config) chrome_wait_for_selector: Option<String>,

    /// CSS selector to scope content extraction (e.g. "article, main, .content")
    #[arg(global = true, long)]
    pub(in crate::core::config) root_selector: Option<String>,

    /// CSS selector to exclude elements from extraction (e.g. ".sidebar, .ads")
    #[arg(global = true, long)]
    pub(in crate::core::config) exclude_selector: Option<String>,

    /// Capture full-page PNG screenshots during Chrome crawl
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::core::config) chrome_screenshot: bool,

    /// Research crawl depth limit for the research command
    #[arg(global = true, long)]
    pub(in crate::core::config) research_depth: Option<usize>,

    /// Time range filter for search (day|week|month|year)
    #[arg(global = true, long, value_parser = ["day", "week", "month", "year"])]
    pub(in crate::core::config) search_time_range: Option<String>,

    /// Lower bound for temporal search filter. Formats: 7d, 30d, 1w, YYYY-MM-DD, RFC3339.
    /// Filters query/ask results to content indexed on or after this date.
    #[arg(global = true, long)]
    pub(in crate::core::config) since: Option<String>,

    /// Upper bound for temporal search filter. Same formats as --since.
    /// Filters query/ask results to content indexed on or before this date.
    #[arg(global = true, long)]
    pub(in crate::core::config) before: Option<String>,

    /// Include per-payload-schema-version chunk-count breakdown on `axon sources`.
    /// Triggers a full collection scroll; opt-in only. See bead axon_rust-lu6a.
    #[arg(global = true, long = "by-schema-version", action = ArgAction::SetTrue)]
    pub(in crate::core::config) sources_by_schema_version: bool,

    /// Bypass Content Security Policy in Chrome (helps pages that block inline JS via CSP)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::core::config) bypass_csp: bool,

    /// Accept invalid or self-signed TLS certificates (for internal/staging sites)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::core::config) accept_invalid_certs: bool,

    /// Capture full scrollable page (true) or viewport only (false, default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::core::config) screenshot_full_page: bool,

    /// Viewport dimensions as WIDTHxHEIGHT (e.g. 1920x1080)
    #[arg(global = true, long, default_value = "1920x1080")]
    pub(in crate::core::config) viewport: String,

    /// Custom HTTP request header in 'Key: Value' format (repeatable)
    #[arg(global = true, long = "header", value_name = "HEADER")]
    pub(in crate::core::config) custom_headers: Vec<String>,

    /// Disable hybrid (dense + BM42 sparse + RRF) search; force dense-only retrieval.
    /// Overrides `AXON_HYBRID_SEARCH=true`. Useful for A/B comparing retrieval quality.
    #[arg(global = true, long = "no-hybrid-search", action = ArgAction::SetTrue, default_value_t = false)]
    pub(in crate::core::config) no_hybrid_search: bool,

    /// Suppress spinners and progress output (useful in scripts). JSON data is unaffected.
    #[arg(global = true, long, action = ArgAction::SetTrue, default_value_t = false)]
    pub(in crate::core::config) quiet: bool,

    /// Override log level. Accepts tracing filter syntax (e.g. debug, info, warn, error,
    /// or crate=level). Applied before tracing init; does not override an explicit RUST_LOG.
    #[arg(global = true, long, env = "AXON_LOG_LEVEL", hide_env_values = true)]
    pub(in crate::core::config) log_level: Option<String>,

    /// Force in-process local execution even when a server URL is configured.
    #[arg(
        global = true,
        long,
        env = "AXON_LOCAL_MODE",
        hide_env_values = true,
        action = ArgAction::SetTrue,
        default_value_t = false
    )]
    pub(in crate::core::config) local: bool,

    /// Route supported commands through a running `axon serve` HTTP endpoint.
    /// Example: `--server-url http://127.0.0.1:8001`. Env: `AXON_SERVER_URL`.
    /// Parsed into a `url::Url` at config-build time; malformed values are rejected with a
    /// clear error before any command runs. If the resolved scheme is `http` and the host
    /// is non-loopback, the CLI refuses to attach `AXON_MCP_HTTP_TOKEN` (cleartext-bearer
    /// guard); set `AXON_SERVER_INSECURE=1` to override.
    #[arg(global = true, long, env = "AXON_SERVER_URL", hide_env_values = true)]
    pub(in crate::core::config) server_url: Option<String>,
}
