use crate::config::types::{ColorChoice, PerformanceProfile, RenderMode, ScrapeFormat};
use clap::{ArgAction, Args};
use std::path::PathBuf;

pub(in crate::config) const DEFAULT_OUTPUT_DIR: &str = ".cache/axon-rust/output";

#[derive(Debug, Args)]
pub(in crate::config) struct GlobalArgs {
    /// Maximum pages to crawl per job (crawl defaults to 2000; 0 = unlimited)
    #[arg(global = true, long)]
    pub(in crate::config) max_pages: Option<u32>,

    /// Maximum crawl depth from the start URL
    #[arg(global = true, long, default_value_t = 10)]
    pub(in crate::config) max_depth: usize,

    /// Crawl all subdomains of the start URL's parent domain
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::config) include_subdomains: bool,

    /// URL path prefixes to exclude from crawling (comma-separated)
    #[arg(global = true, long = "exclude-path-prefix", value_delimiter = ',')]
    pub(in crate::config) exclude_path_prefix: Vec<String>,

    /// Repo-relative path substrings to exclude from git ingest, e.g.
    /// `--exclude-path docs/references/ --exclude-path vendor/` (repeatable).
    /// A file is skipped when its repo-relative path contains any value.
    #[arg(global = true, long = "exclude-path")]
    pub(in crate::config) ingest_exclude_paths: Vec<String>,

    /// Directory for saved markdown/HTML output files
    #[arg(global = true, long, default_value = DEFAULT_OUTPUT_DIR)]
    pub(in crate::config) output_dir: PathBuf,

    /// Explicit output file path (overrides --output-dir)
    #[arg(global = true, long)]
    pub(in crate::config) output: Option<PathBuf>,

    /// Write every fetched page of a crawl to a WARC 1.1 archive at this path
    /// (crawl only; HTTP and Chrome both archive).
    #[arg(global = true, long, value_name = "PATH")]
    pub(in crate::config) warc: Option<PathBuf>,

    /// JSON file mapping URL path prefixes to ordered Chrome web-automation
    /// steps (click/scroll/wait/evaluate/…) run before each matching page is
    /// captured during a Chrome crawl.
    #[arg(global = true, long = "automation-script", value_name = "PATH")]
    pub(in crate::config) automation_script: Option<PathBuf>,

    /// Page fetch mode: http, chrome, or auto-switch
    #[arg(global = true, long, value_enum, default_value_t = RenderMode::AutoSwitch)]
    pub(in crate::config) render_mode: RenderMode,

    /// Enable crawl cache reuse. Disabled by default; opt in with `--cache true`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::config) cache: bool,

    /// Keep cached crawl flow on the HTTP path and suppress Chrome runtime/bootstrap.
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::config) cache_http_only: bool,

    /// Enable conditional re-crawl (ETag / If-Modified-Since). Re-crawls send
    /// validators and 304-unchanged pages are reused from the previous run
    /// instead of being re-fetched. Requires `--cache true`.
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::config) etag_conditional: bool,

    /// Output format: markdown, html, rawHtml, json
    #[arg(global = true, long, value_enum, default_value_t = ScrapeFormat::Markdown)]
    pub(in crate::config) format: ScrapeFormat,

    /// Maximum number of results to return (default: 10)
    #[arg(global = true, long, default_value_t = 10)]
    pub(in crate::config) limit: usize,

    /// Query text (alternative to positional argument)
    #[arg(global = true, long)]
    pub(in crate::config) query: Option<String>,

    /// Comma-separated list of URLs (alternative to positional arguments)
    #[arg(global = true, long)]
    pub(in crate::config) urls: Option<String>,

    /// URL glob patterns to expand into seed URLs (comma-separated)
    #[arg(global = true, long = "url-glob", value_delimiter = ',')]
    pub(in crate::config) url_glob: Vec<String>,

    /// Fetch/save only; do not embed scraped or crawled content into Qdrant.
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::config) skip_embed: bool,

    /// Qdrant collection name (default: axon)
    #[arg(
        global = true,
        long,
        env = "AXON_COLLECTION",
        hide_env_values = true,
        default_value = "axon"
    )]
    pub(in crate::config) collection: String,

    /// Concurrent connections for batch operations (1-512)
    #[arg(global = true, long, default_value_t = 16)]
    pub(in crate::config) batch_concurrency: usize,

    /// Block until async job completes; false enqueues and returns (default: false)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::config) wait: bool,

    /// Skip confirmation prompts (non-interactive mode)
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::config) yes: bool,

    /// Output results as machine-readable JSON
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::config) json: bool,

    /// Color output: auto (TTY detect, default), always, never
    #[arg(global = true, long, value_enum, default_value_t = ColorChoice::Auto)]
    pub(in crate::config) color: ColorChoice,

    /// Live-update mode (currently honored by `axon status`)
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::config) watch: bool,

    /// Status mode: show only watchdog-reclaimed jobs.
    #[arg(
        global = true,
        long,
        action = ArgAction::SetTrue,
        conflicts_with_all = ["active", "recent"]
    )]
    pub(in crate::config) reclaimed: bool,

    /// Show only active jobs (running/pending) in list and status views.
    #[arg(global = true, long, action = ArgAction::SetTrue, conflicts_with = "recent")]
    pub(in crate::config) active: bool,

    /// Show active + completed jobs (hide failed/canceled) in list and status views.
    #[arg(global = true, long, action = ArgAction::SetTrue, conflicts_with = "active")]
    pub(in crate::config) recent: bool,

    /// Concurrency preset: high-stable, balanced, extreme, max
    #[arg(global = true, long, value_enum, default_value_t = PerformanceProfile::HighStable)]
    pub(in crate::config) performance_profile: PerformanceProfile,

    /// Only run sitemap discovery, not a full crawl
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::config) sitemap_only: bool,

    /// Text Embeddings Inference server URL (overrides TEI_URL)
    #[arg(global = true, long)]
    pub(in crate::config) tei_url: Option<String>,

    /// Qdrant server URL (overrides QDRANT_URL)
    #[arg(global = true, long)]
    pub(in crate::config) qdrant_url: Option<String>,

    /// Cron interval: re-run the command every N seconds
    #[arg(global = true, long)]
    pub(in crate::config) cron_every_seconds: Option<u64>,

    /// Stop cron after N runs (default: run forever)
    #[arg(global = true, long)]
    pub(in crate::config) cron_max_runs: Option<usize>,

    /// Deduplicate trailing-slash URL variants during crawl. Disable with `--normalize false`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::config) normalize: bool,

    /// Block asset downloads (images/CSS/fonts) during crawl
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::config) block_assets: bool,

    /// CSS selector to wait for before Chrome captures the page
    #[arg(global = true, long)]
    pub(in crate::config) chrome_wait_for_selector: Option<String>,

    /// CSS selector to scope content extraction (e.g. "article, main, .content")
    #[arg(global = true, long)]
    pub(in crate::config) root_selector: Option<String>,

    /// CSS selector to exclude elements from extraction (e.g. ".sidebar, .ads")
    #[arg(global = true, long)]
    pub(in crate::config) exclude_selector: Option<String>,

    /// Capture full-page PNG screenshots during Chrome crawl
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::config) chrome_screenshot: bool,

    /// Number of sources to synthesize over for the research command.
    /// Overrides --limit when set; falls back to --limit (default 10) when unset.
    /// Capped together with --offset at 100 (Tavily window).
    #[arg(global = true, long)]
    pub(in crate::config) research_depth: Option<usize>,

    /// Time range filter for search (day|week|month|year)
    #[arg(global = true, long, value_parser = ["day", "week", "month", "year"])]
    pub(in crate::config) search_time_range: Option<String>,

    /// Lower bound for temporal search filter. Formats: 7d, 30d, 1w, YYYY-MM-DD, RFC3339.
    /// Filters query/ask results to content indexed on or after this date.
    #[arg(global = true, long)]
    pub(in crate::config) since: Option<String>,

    /// Upper bound for temporal search filter. Same formats as --since.
    /// Filters query/ask results to content indexed on or before this date.
    #[arg(global = true, long)]
    pub(in crate::config) before: Option<String>,

    /// Capture full scrollable page (true) or viewport only (false, default: true)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::config) screenshot_full_page: bool,

    /// Viewport dimensions as WIDTHxHEIGHT (e.g. 1920x1080)
    #[arg(global = true, long, default_value = "1920x1080")]
    pub(in crate::config) viewport: String,

    /// Custom HTTP request header in 'Key: Value' format (repeatable)
    #[arg(global = true, long = "header", value_name = "HEADER")]
    pub(in crate::config) custom_headers: Vec<String>,

    /// Per-path crawl budget in 'PATH=N' format, e.g. '/blog=100' or '*=1000'
    /// (repeatable). Caps the number of pages crawled under each path prefix;
    /// '*' applies to all paths. Unset = no budget (current behavior).
    #[arg(global = true, long = "budget", value_name = "PATH=N")]
    pub(in crate::config) path_budgets: Vec<String>,

    /// Disable hybrid (dense + BM42 sparse + RRF) search; force dense-only retrieval.
    /// Overrides `AXON_HYBRID_SEARCH=true`. Useful for A/B comparing retrieval quality.
    #[arg(global = true, long = "no-hybrid-search", action = ArgAction::SetTrue, default_value_t = false)]
    pub(in crate::config) no_hybrid_search: bool,

    /// Suppress spinners and progress output (useful in scripts). JSON data is unaffected.
    #[arg(global = true, long, action = ArgAction::SetTrue, default_value_t = false)]
    pub(in crate::config) quiet: bool,
}
