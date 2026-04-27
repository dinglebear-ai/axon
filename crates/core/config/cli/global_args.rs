use crate::crates::core::config::types::{PerformanceProfile, RenderMode, ScrapeFormat};
use clap::{ArgAction, Args};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub(in crate::crates::core::config) struct GlobalArgs {
    #[arg(global = true, long, default_value = "")]
    pub(in crate::crates::core::config) start_url: String,

    /// Maximum pages to crawl per job (0 = unlimited)
    #[arg(global = true, long, default_value_t = 0)]
    pub(in crate::crates::core::config) max_pages: u32,

    /// Maximum crawl depth from the start URL
    #[arg(global = true, long, default_value_t = 5)]
    pub(in crate::crates::core::config) max_depth: usize,

    /// Crawl all subdomains of the start URL's parent domain
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) include_subdomains: bool,

    /// URL path prefixes to exclude from crawling (comma-separated)
    #[arg(global = true, long = "exclude-path-prefix", value_delimiter = ',')]
    pub(in crate::crates::core::config) exclude_path_prefix: Vec<String>,

    /// Directory for saved markdown/HTML output files
    #[arg(
        global = true,
        long,
        default_value = ".cache/axon-rust/output",
        env = "AXON_OUTPUT_DIR"
    )]
    pub(in crate::crates::core::config) output_dir: PathBuf,

    /// Explicit output file path (overrides --output-dir)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) output: Option<PathBuf>,

    /// Page fetch mode: http, chrome, or auto-switch
    #[arg(global = true, long, value_enum, default_value_t = RenderMode::AutoSwitch)]
    pub(in crate::crates::core::config) render_mode: RenderMode,

    /// Chrome DevTools Protocol management endpoint URL
    #[arg(global = true, long, env = "AXON_CHROME_REMOTE_URL")]
    pub(in crate::crates::core::config) chrome_remote_url: Option<String>,

    /// HTTP proxy URL for Chrome requests
    #[arg(global = true, long, env = "AXON_CHROME_PROXY")]
    pub(in crate::crates::core::config) chrome_proxy: Option<String>,

    /// Custom User-Agent header for Chrome requests
    #[arg(global = true, long, env = "AXON_CHROME_USER_AGENT")]
    pub(in crate::crates::core::config) chrome_user_agent: Option<String>,

    /// Run Chrome in headless mode (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) chrome_headless: bool,

    /// Enable Chrome anti-bot evasion mode (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) chrome_anti_bot: bool,

    /// Enable Chrome network interception for blocking ads/trackers (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) chrome_intercept: bool,

    /// Enable Chrome stealth mode to patch navigator.webdriver (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) chrome_stealth: bool,

    /// Bootstrap Chrome connection before starting the crawl (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) chrome_bootstrap: bool,

    /// Timeout in milliseconds for Chrome bootstrap (default: 3000)
    #[arg(global = true, long, default_value_t = 3000)]
    pub(in crate::crates::core::config) chrome_bootstrap_timeout_ms: u64,

    /// Number of retries for Chrome bootstrap failures (default: 2)
    #[arg(global = true, long, default_value_t = 2)]
    pub(in crate::crates::core::config) chrome_bootstrap_retries: usize,

    /// Respect robots.txt directives (default: false)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) respect_robots: bool,

    /// Minimum content length; shorter pages are flagged thin (default: 200)
    #[arg(global = true, long, default_value_t = 200)]
    pub(in crate::crates::core::config) min_markdown_chars: usize,

    /// Skip thin pages — do not save or embed them (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) drop_thin_markdown: bool,

    /// Discover and backfill URLs from sitemap.xml after crawl (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) discover_sitemaps: bool,

    /// Only backfill sitemap URLs with a `<lastmod>` date within the last N days (0 = no filter).
    /// URLs without a `<lastmod>` tag are always included.
    #[arg(global = true, long, default_value_t = 0)]
    pub(in crate::crates::core::config) sitemap_since_days: u32,

    /// Maximum number of sitemap URLs to process per map/backfill operation (default: 512)
    #[arg(global = true, long, default_value_t = 512)]
    pub(in crate::crates::core::config) max_sitemaps: usize,

    /// Enable crawl cache reuse. Disable with `--cache false`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) cache: bool,

    /// Skip cache for browser (Chrome) fetches only
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) cache_skip_browser: bool,

    /// Output format: markdown, html, rawHtml, json
    #[arg(global = true, long, value_enum, default_value_t = ScrapeFormat::Markdown)]
    pub(in crate::crates::core::config) format: ScrapeFormat,

    /// Maximum number of results to return (default: 10)
    #[arg(global = true, long, default_value_t = 10)]
    pub(in crate::crates::core::config) limit: usize,

    /// Query text (alternative to positional argument)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) query: Option<String>,

    /// Comma-separated list of URLs (alternative to positional arguments)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) urls: Option<String>,

    /// URL glob patterns to expand into seed URLs (comma-separated)
    #[arg(global = true, long = "url-glob", value_delimiter = ',')]
    pub(in crate::crates::core::config) url_glob: Vec<String>,

    /// Auto-embed scraped content into Qdrant (default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) embed: bool,

    /// Qdrant collection name (default: cortex)
    #[arg(global = true, long, env = "AXON_COLLECTION", default_value = "cortex")]
    pub(in crate::crates::core::config) collection: String,

    /// Concurrent connections for batch operations (1-512)
    #[arg(global = true, long, default_value_t = 16)]
    pub(in crate::crates::core::config) batch_concurrency: usize,

    /// Block until async job completes (default: false)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) wait: bool,

    /// Run without Postgres/Redis/RabbitMQ. Uses SQLite + in-process workers.
    #[arg(global = true, long, default_value_t = false)]
    pub(in crate::crates::core::config) lite: bool,

    /// Path to the SQLite jobs database (lite mode only).
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) sqlite_path: Option<PathBuf>,

    /// Skip confirmation prompts (non-interactive mode)
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::crates::core::config) yes: bool,

    /// Output results as machine-readable JSON
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::crates::core::config) json: bool,

    /// Enable graph-enhanced retrieval for ask (requires Neo4j)
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::crates::core::config) graph: bool,

    /// Status mode: show only watchdog-reclaimed jobs.
    #[arg(
        global = true,
        long,
        action = ArgAction::SetTrue,
        conflicts_with_all = ["active", "recent"]
    )]
    pub(in crate::crates::core::config) reclaimed: bool,

    /// Show only active jobs (running/pending) in list and status views.
    #[arg(global = true, long, action = ArgAction::SetTrue, conflicts_with = "recent")]
    pub(in crate::crates::core::config) active: bool,

    /// Show active + completed jobs (hide failed/canceled) in list and status views.
    #[arg(global = true, long, action = ArgAction::SetTrue, conflicts_with = "active")]
    pub(in crate::crates::core::config) recent: bool,

    /// Concurrency preset: high-stable, balanced, extreme, max
    #[arg(global = true, long, value_enum, default_value_t = PerformanceProfile::HighStable)]
    pub(in crate::crates::core::config) performance_profile: PerformanceProfile,

    /// Override all concurrency limits (crawl, sitemap, backfill) at once
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) concurrency_limit: Option<usize>,

    /// Override crawl concurrency (default: from profile)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) crawl_concurrency_limit: Option<usize>,

    /// Override backfill concurrency (default: from profile)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) backfill_concurrency_limit: Option<usize>,

    /// Only run sitemap discovery, not a full crawl
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::crates::core::config) sitemap_only: bool,

    /// Delay between requests in milliseconds (polite crawling)
    #[arg(global = true, long, default_value_t = 0)]
    pub(in crate::crates::core::config) delay_ms: u64,

    /// Per-request HTTP timeout in milliseconds (default: from profile)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) request_timeout_ms: Option<u64>,

    /// Number of retries on failed fetches (default: from profile)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) fetch_retries: Option<usize>,

    /// Backoff between retries in milliseconds (default: from profile)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) retry_backoff_ms: Option<u64>,

    /// Share one queue across supported jobs. Disable with `--shared-queue false`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) shared_queue: bool,

    /// PostgreSQL connection URL (overrides AXON_PG_URL)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) pg_url: Option<String>,

    /// Redis connection URL (overrides AXON_REDIS_URL)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) redis_url: Option<String>,

    /// RabbitMQ AMQP URL (overrides AXON_AMQP_URL)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) amqp_url: Option<String>,

    /// Crawl job queue name (overrides AXON_CRAWL_QUEUE)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) crawl_queue: Option<String>,

    /// Refresh job queue name (overrides AXON_REFRESH_QUEUE)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) refresh_queue: Option<String>,

    /// Extract job queue name (overrides AXON_EXTRACT_QUEUE)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) extract_queue: Option<String>,

    /// Embed job queue name (overrides AXON_EMBED_QUEUE)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) embed_queue: Option<String>,

    /// Ingest job queue name (overrides AXON_INGEST_QUEUE)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) ingest_queue: Option<String>,

    /// Text Embeddings Inference server URL (overrides TEI_URL)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) tei_url: Option<String>,

    /// Qdrant server URL (overrides QDRANT_URL)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) qdrant_url: Option<String>,

    /// OpenAI-compatible API base URL (overrides OPENAI_BASE_URL)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) openai_base_url: Option<String>,

    /// OpenAI API key (overrides OPENAI_API_KEY)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) openai_api_key: Option<String>,

    /// LLM model name for ACP completion (overrides OPENAI_MODEL)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) openai_model: Option<String>,

    /// Seconds before a running job is considered stale by the watchdog
    #[arg(
        global = true,
        long,
        env = "AXON_JOB_STALE_TIMEOUT_SECS",
        default_value_t = 300
    )]
    pub(in crate::crates::core::config) watchdog_stale_timeout_secs: i64,

    /// Additional grace period (seconds) before a stale job is reclaimed
    #[arg(
        global = true,
        long,
        env = "AXON_JOB_STALE_CONFIRM_SECS",
        default_value_t = 60
    )]
    pub(in crate::crates::core::config) watchdog_confirm_secs: i64,

    /// Cron interval: re-run the command every N seconds
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) cron_every_seconds: Option<u64>,

    /// Stop cron after N runs (default: run forever)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) cron_max_runs: Option<usize>,

    /// Deduplicate trailing-slash URL variants during crawl. Disable with `--normalize false`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) normalize: bool,

    /// Seconds to wait for Chrome network idle before page capture (default: 15)
    #[arg(global = true, long, default_value_t = 15)]
    pub(in crate::crates::core::config) chrome_network_idle_timeout: u64,

    /// Thin-page ratio to trigger auto-switch to Chrome (0.0-1.0, default: 0.60)
    #[arg(global = true, long, default_value_t = 0.60)]
    pub(in crate::crates::core::config) auto_switch_thin_ratio: f64,

    /// Minimum pages before auto-switch eligibility check (default: 10)
    #[arg(global = true, long, default_value_t = 10)]
    pub(in crate::crates::core::config) auto_switch_min_pages: usize,

    /// Only crawl URLs matching these regex patterns (repeatable)
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) url_whitelist: Vec<String>,

    /// Block asset downloads (images/CSS/fonts) during crawl
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) block_assets: bool,

    /// Maximum response size per page in bytes (0 = unlimited)
    #[arg(global = true, long, default_value_t = 0)]
    pub(in crate::crates::core::config) max_page_bytes: u64,

    /// Only follow same-origin redirects (strict redirect policy)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) redirect_policy_strict: bool,

    /// CSS selector to wait for before Chrome captures the page
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) chrome_wait_for_selector: Option<String>,

    /// CSS selector to scope content extraction (e.g. "article, main, .content")
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) root_selector: Option<String>,

    /// CSS selector to exclude elements from extraction (e.g. ".sidebar, .ads")
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) exclude_selector: Option<String>,

    /// Capture full-page PNG screenshots during Chrome crawl
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) chrome_screenshot: bool,

    /// Research crawl depth limit for the research command
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) research_depth: Option<usize>,

    /// Time range filter for search (day|week|month|year)
    #[arg(global = true, long, value_parser = ["day", "week", "month", "year"])]
    pub(in crate::crates::core::config) search_time_range: Option<String>,

    /// Lower bound for temporal search filter. Formats: 7d, 30d, 1w, YYYY-MM-DD, RFC3339.
    /// Filters query/ask results to content indexed on or after this date.
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) since: Option<String>,

    /// Upper bound for temporal search filter. Same formats as --since.
    /// Filters query/ask results to content indexed on or before this date.
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) before: Option<String>,

    /// Bypass Content Security Policy in Chrome (helps pages that block inline JS via CSP)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) bypass_csp: bool,

    /// Accept invalid or self-signed TLS certificates (for internal/staging sites)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) accept_invalid_certs: bool,

    /// Capture full scrollable page (true) or viewport only (false, default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) screenshot_full_page: bool,

    /// Viewport dimensions as WIDTHxHEIGHT (e.g. 1920x1080)
    #[arg(global = true, long, default_value = "1920x1080")]
    pub(in crate::crates::core::config) viewport: String,

    /// Custom HTTP request header in 'Key: Value' format (repeatable)
    #[arg(global = true, long = "header", value_name = "HEADER")]
    pub(in crate::crates::core::config) custom_headers: Vec<String>,

    /// Suppress spinners and progress output (useful in scripts). JSON data is unaffected.
    #[arg(global = true, long, action = ArgAction::SetTrue, default_value_t = false)]
    pub(in crate::crates::core::config) quiet: bool,
}
