use crate::crates::core::config::types::{PerformanceProfile, RenderMode, ScrapeFormat};
use clap::{ArgAction, Args};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub(in crate::crates::core::config) struct GlobalArgs {
    #[arg(global = true, long, default_value = "")]
    pub(in crate::crates::core::config) start_url: String,

    #[arg(global = true, long, default_value_t = 0)]
    pub(in crate::crates::core::config) max_pages: u32,

    #[arg(global = true, long, default_value_t = 5)]
    pub(in crate::crates::core::config) max_depth: usize,

    /// Crawl subdomains of the start URL's parent domain. Enable with `--include-subdomains true`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) include_subdomains: bool,

    #[arg(global = true, long = "exclude-path-prefix", value_delimiter = ',')]
    pub(in crate::crates::core::config) exclude_path_prefix: Vec<String>,

    #[arg(
        global = true,
        long,
        default_value = ".cache/axon-rust/output",
        env = "AXON_OUTPUT_DIR"
    )]
    pub(in crate::crates::core::config) output_dir: PathBuf,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) output: Option<PathBuf>,

    #[arg(global = true, long, value_enum, default_value_t = RenderMode::AutoSwitch)]
    pub(in crate::crates::core::config) render_mode: RenderMode,

    #[arg(global = true, long, env = "AXON_CHROME_REMOTE_URL")]
    pub(in crate::crates::core::config) chrome_remote_url: Option<String>,

    #[arg(global = true, long, env = "AXON_CHROME_PROXY")]
    pub(in crate::crates::core::config) chrome_proxy: Option<String>,

    #[arg(global = true, long, env = "AXON_CHROME_USER_AGENT")]
    pub(in crate::crates::core::config) chrome_user_agent: Option<String>,

    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) chrome_headless: bool,

    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) chrome_anti_bot: bool,

    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) chrome_intercept: bool,

    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) chrome_stealth: bool,

    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) chrome_bootstrap: bool,

    #[arg(global = true, long, default_value_t = 3000)]
    pub(in crate::crates::core::config) chrome_bootstrap_timeout_ms: u64,

    #[arg(global = true, long, default_value_t = 2)]
    pub(in crate::crates::core::config) chrome_bootstrap_retries: usize,

    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) respect_robots: bool,

    #[arg(global = true, long, default_value_t = 200)]
    pub(in crate::crates::core::config) min_markdown_chars: usize,

    /// Drop thin markdown pages. Disable with `--drop-thin-markdown false`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) drop_thin_markdown: bool,

    /// Discover and backfill sitemap URLs. Disable with `--discover-sitemaps false`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) discover_sitemaps: bool,

    /// Only backfill sitemap URLs with a `<lastmod>` date within the last N days (0 = no filter).
    /// URLs without a `<lastmod>` tag are always included.
    #[arg(global = true, long, default_value_t = 0)]
    pub(in crate::crates::core::config) sitemap_since_days: u32,

    /// Enable crawl cache reuse. Disable with `--cache false`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) cache: bool,

    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) cache_skip_browser: bool,

    #[arg(global = true, long, value_enum, default_value_t = ScrapeFormat::Markdown)]
    pub(in crate::crates::core::config) format: ScrapeFormat,

    #[arg(global = true, long, default_value_t = 10)]
    pub(in crate::crates::core::config) limit: usize,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) query: Option<String>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) urls: Option<String>,

    #[arg(global = true, long = "url-glob", value_delimiter = ',')]
    pub(in crate::crates::core::config) url_glob: Vec<String>,

    /// Trigger follow-up embed flows when supported. Disable with `--embed false`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) embed: bool,

    #[arg(global = true, long, env = "AXON_COLLECTION", default_value = "cortex")]
    pub(in crate::crates::core::config) collection: String,

    #[arg(global = true, long, default_value_t = 16)]
    pub(in crate::crates::core::config) batch_concurrency: usize,

    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) wait: bool,

    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::crates::core::config) yes: bool,

    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::crates::core::config) json: bool,

    #[arg(
        global = true,
        long,
        action = ArgAction::SetTrue,
        help = "Enable graph-enhanced retrieval (requires Neo4j)"
    )]
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

    #[arg(global = true, long, value_enum, default_value_t = PerformanceProfile::HighStable)]
    pub(in crate::crates::core::config) performance_profile: PerformanceProfile,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) concurrency_limit: Option<usize>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) crawl_concurrency_limit: Option<usize>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) backfill_concurrency_limit: Option<usize>,

    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::crates::core::config) sitemap_only: bool,

    #[arg(global = true, long, default_value_t = 0)]
    pub(in crate::crates::core::config) delay_ms: u64,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) request_timeout_ms: Option<u64>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) fetch_retries: Option<usize>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) retry_backoff_ms: Option<u64>,

    /// Share one queue across supported jobs. Disable with `--shared-queue false`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) shared_queue: bool,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) pg_url: Option<String>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) redis_url: Option<String>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) amqp_url: Option<String>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) crawl_queue: Option<String>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) refresh_queue: Option<String>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) extract_queue: Option<String>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) embed_queue: Option<String>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) ingest_queue: Option<String>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) tei_url: Option<String>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) qdrant_url: Option<String>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) openai_base_url: Option<String>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) openai_api_key: Option<String>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) openai_model: Option<String>,

    #[arg(
        global = true,
        long,
        env = "AXON_JOB_STALE_TIMEOUT_SECS",
        default_value_t = 300
    )]
    pub(in crate::crates::core::config) watchdog_stale_timeout_secs: i64,

    #[arg(
        global = true,
        long,
        env = "AXON_JOB_STALE_CONFIRM_SECS",
        default_value_t = 60
    )]
    pub(in crate::crates::core::config) watchdog_confirm_secs: i64,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) cron_every_seconds: Option<u64>,

    #[arg(global = true, long)]
    pub(in crate::crates::core::config) cron_max_runs: Option<usize>,

    /// Deduplicate trailing-slash URL variants during crawl. Disable with `--normalize false`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) normalize: bool,

    /// Seconds to wait for Chrome network idle before page capture. Default: 15.
    #[arg(global = true, long, default_value_t = 15)]
    pub(in crate::crates::core::config) chrome_network_idle_timeout: u64,

    /// Thin-page ratio to trigger auto-switch to Chrome (0.0–1.0). Default: 0.60.
    #[arg(global = true, long, default_value_t = 0.60)]
    pub(in crate::crates::core::config) auto_switch_thin_ratio: f64,

    /// Minimum pages before auto-switch eligibility check. Default: 10.
    #[arg(global = true, long, default_value_t = 10)]
    pub(in crate::crates::core::config) auto_switch_min_pages: usize,

    /// Only crawl URLs matching these regex patterns (repeatable). Default: none.
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) url_whitelist: Vec<String>,

    /// Block asset downloads (images/CSS/fonts) during crawl.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) block_assets: bool,

    /// Maximum response size per page in bytes (0 = unlimited). Default: 0.
    #[arg(global = true, long, default_value_t = 0)]
    pub(in crate::crates::core::config) max_page_bytes: u64,

    /// Only follow same-origin redirects (strict redirect policy).
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) redirect_policy_strict: bool,

    /// CSS selector to wait for before Chrome captures the page. Default: none.
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) chrome_wait_for_selector: Option<String>,

    /// CSS selector to scope content extraction (e.g. "article, main, .content"). Default: none.
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) root_selector: Option<String>,

    /// CSS selector to exclude elements from extraction (e.g. ".sidebar, .ads"). Default: none.
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) exclude_selector: Option<String>,

    /// Capture full-page PNG screenshots during Chrome crawl.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) chrome_screenshot: bool,

    /// Research crawl depth limit for the research command. Default: none.
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) research_depth: Option<usize>,

    /// Time range filter for search (day|week|month|year). Default: none.
    #[arg(global = true, long, value_parser = ["day", "week", "month", "year"])]
    pub(in crate::crates::core::config) search_time_range: Option<String>,

    /// Lower bound for temporal search filter. Formats: 7d, 30d, 1w, YYYY-MM-DD, RFC3339.
    /// Filters query/ask results to content indexed on or after this date. Default: none.
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) since: Option<String>,

    /// Upper bound for temporal search filter. Same formats as --since.
    /// Filters query/ask results to content indexed on or before this date. Default: none.
    #[arg(global = true, long)]
    pub(in crate::crates::core::config) before: Option<String>,

    /// Bypass Content Security Policy in Chrome. Helps pages that block inline JS via CSP. Default: false.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) bypass_csp: bool,

    /// Accept invalid or self-signed TLS certificates. Useful for internal/staging sites. Default: false.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::crates::core::config) accept_invalid_certs: bool,

    /// Capture full scrollable page (true) or viewport only (false). Default: true.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::crates::core::config) screenshot_full_page: bool,

    /// Viewport dimensions as WIDTHxHEIGHT (e.g. 1920x1080). Default: 1920x1080.
    #[arg(global = true, long, default_value = "1920x1080")]
    pub(in crate::crates::core::config) viewport: String,

    /// Custom HTTP request header in `Key: Value` format. Can be specified multiple times.
    #[arg(global = true, long = "header", value_name = "HEADER")]
    pub(in crate::crates::core::config) custom_headers: Vec<String>,
}
