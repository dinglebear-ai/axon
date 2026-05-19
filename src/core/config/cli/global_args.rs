use crate::core::config::types::{PerformanceProfile, RenderMode, ScrapeFormat};
use clap::{ArgAction, Args};
use std::path::PathBuf;

pub(in crate::core::config) const DEFAULT_OUTPUT_DIR: &str = ".cache/axon-rust/output";

#[derive(Debug, Args)]
pub(in crate::core::config) struct GlobalArgs {
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

    /// Enable crawl cache reuse. Disable with `--cache false`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = true)]
    pub(in crate::core::config) cache: bool,

    /// Keep cached crawl flow on the HTTP path and suppress Chrome runtime/bootstrap.
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::core::config) cache_http_only: bool,

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

    /// Fetch/save only; do not embed scraped or crawled content into Qdrant.
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::core::config) skip_embed: bool,

    /// Qdrant collection name (default: axon)
    #[arg(
        global = true,
        long,
        env = "AXON_COLLECTION",
        hide_env_values = true,
        default_value = "axon"
    )]
    pub(in crate::core::config) collection: String,

    /// Concurrent connections for batch operations (1-512)
    #[arg(global = true, long, default_value_t = 16)]
    pub(in crate::core::config) batch_concurrency: usize,

    /// Block until async job completes; false enqueues and returns (default: false)
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::core::config) wait: bool,

    /// Skip confirmation prompts (non-interactive mode)
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::core::config) yes: bool,

    /// Output results as machine-readable JSON
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::core::config) json: bool,

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

    /// Only run sitemap discovery, not a full crawl
    #[arg(global = true, long, action = ArgAction::SetTrue)]
    pub(in crate::core::config) sitemap_only: bool,

    /// Text Embeddings Inference server URL (overrides TEI_URL)
    #[arg(global = true, long)]
    pub(in crate::core::config) tei_url: Option<String>,

    /// Qdrant server URL (overrides QDRANT_URL)
    #[arg(global = true, long)]
    pub(in crate::core::config) qdrant_url: Option<String>,

    /// Cron interval: re-run the command every N seconds
    #[arg(global = true, long)]
    pub(in crate::core::config) cron_every_seconds: Option<u64>,

    /// Stop cron after N runs (default: run forever)
    #[arg(global = true, long)]
    pub(in crate::core::config) cron_max_runs: Option<usize>,

    /// Deduplicate trailing-slash URL variants during crawl. Disable with `--normalize false`.
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::core::config) normalize: bool,

    /// Block asset downloads (images/CSS/fonts) during crawl
    #[arg(global = true, long, action = ArgAction::Set, default_value_t = false)]
    pub(in crate::core::config) block_assets: bool,

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

    /// Number of sources to synthesize over for the research command.
    /// Overrides --limit when set; falls back to --limit (default 10) when unset.
    /// Capped together with --offset at 100 (Tavily window).
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
}
