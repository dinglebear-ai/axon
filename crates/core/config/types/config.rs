use super::enums::{
    CommandKind, EvaluateResponsesMode, McpTransport, PerformanceProfile, RedditSort, RedditTime,
    RenderMode, ScrapeFormat,
};
use std::path::PathBuf;

#[derive(Clone)]
pub struct Config {
    /// The subcommand being executed (scrape, crawl, ask, etc.).
    pub command: CommandKind,

    /// Primary URL argument; used by scrape, crawl, map, and similar single-URL commands.
    pub start_url: String,

    /// Positional arguments after the subcommand (URLs, query text, job sub-subcommand tokens).
    pub positional: Vec<String>,

    /// Comma-separated URL list provided via `--urls` (alternative to positional arguments).
    pub urls_csv: Option<String>,

    /// Glob patterns to expand into seed URLs (e.g. `https://docs.rs/foo/**`).
    pub url_glob: Vec<String>,

    /// Query text for `query`, `ask`, and `evaluate` commands; also settable via `--query`.
    pub query: Option<String>,

    /// Maximum number of results returned by `query`/`search` commands. Flag: `--limit`.
    pub search_limit: usize,

    /// Maximum pages to crawl (0 = uncapped). Flag: `--max-pages`.
    pub max_pages: u32,

    /// Maximum crawl depth from the start URL. Flag: `--max-depth`.
    pub max_depth: usize,

    /// Whether to follow links from subdomains of the start URL. Flag: `--include-subdomains`.
    pub include_subdomains: bool,

    /// URL path prefixes to skip during crawl (e.g. `/blog/`, `/legacy/`). Flag: `--exclude-path-prefix`.
    pub exclude_path_prefix: Vec<String>,

    /// Directory for saved markdown/HTML output files. Flag: `--output-dir`.
    pub output_dir: PathBuf,

    /// Explicit single-file output path (overrides `output_dir` for single-URL commands). Flag: `--output`.
    pub output_path: Option<PathBuf>,

    /// Browser rendering strategy: `http`, `chrome`, or `auto-switch`. Flag: `--render-mode`.
    pub render_mode: RenderMode,

    /// URL of the Chrome DevTools Protocol (CDP) management endpoint. Env: `AXON_CHROME_REMOTE_URL`.
    pub chrome_remote_url: Option<String>,

    /// HTTP proxy URL for Chrome requests. Env: `AXON_CHROME_PROXY`.
    pub chrome_proxy: Option<String>,

    /// Custom `User-Agent` header sent by Chrome. Env: `AXON_CHROME_USER_AGENT`.
    pub chrome_user_agent: Option<String>,

    /// Run Chrome in headless mode (no visible window). Flag: `--chrome-headless`.
    pub chrome_headless: bool,

    /// Enable Chrome's anti-bot evasion mode. Flag: `--chrome-anti-bot`.
    pub chrome_anti_bot: bool,

    /// Enable Chrome network interception (for blocking ads/trackers). Flag: `--chrome-intercept`.
    pub chrome_intercept: bool,

    /// Enable Chrome stealth mode (patches `navigator.webdriver`). Flag: `--chrome-stealth`.
    pub chrome_stealth: bool,

    /// Bootstrap Chrome connection before starting the crawl. Flag: `--chrome-bootstrap`.
    pub chrome_bootstrap: bool,

    /// Timeout in milliseconds to wait for Chrome bootstrap. Flag: `--chrome-bootstrap-timeout-ms`.
    pub chrome_bootstrap_timeout_ms: u64,

    /// Number of retries for Chrome bootstrap failures. Flag: `--chrome-bootstrap-retries`.
    pub chrome_bootstrap_retries: usize,

    /// Whether to honour `robots.txt` directives. Defaults `false`. Flag: `--respect-robots`.
    pub respect_robots: bool,

    /// Pages with fewer than this many markdown characters are treated as "thin". Flag: `--min-markdown-chars`.
    pub min_markdown_chars: usize,

    /// Drop thin pages — do not save or embed them. Flag: `--drop-thin-markdown`.
    pub drop_thin_markdown: bool,

    /// Discover and backfill URLs from `sitemap.xml` after the main crawl. Flag: `--discover-sitemaps`.
    pub discover_sitemaps: bool,

    /// Only backfill sitemap URLs with `<lastmod>` within the last N days (0 = no filter). Flag: `--sitemap-since-days`.
    pub sitemap_since_days: u32,

    /// Enable Spider's built-in crawl-result caching. Flag: `--cache`.
    pub cache: bool,

    /// Skip the cache for browser (Chrome) fetches only. Flag: `--cache-skip-browser`.
    pub cache_skip_browser: bool,

    /// Output format for scraped pages: `markdown`, `html`, `rawHtml`, or `json`. Flag: `--format`.
    pub format: ScrapeFormat,

    /// Qdrant collection name to read from and write to. Env: `AXON_COLLECTION`. Flag: `--collection`.
    pub collection: String,

    /// Automatically embed scraped content into Qdrant after fetching. Flag: `--embed`.
    pub embed: bool,

    /// Number of concurrent connections for batch operations (clamped 1–512). Flag: `--batch-concurrency`.
    pub batch_concurrency: usize,

    /// Block until async jobs complete instead of fire-and-forgetting. Flag: `--wait`.
    pub wait: bool,

    /// Skip confirmation prompts (non-interactive mode). Flag: `--yes`.
    pub yes: bool,

    /// Concurrency/timeout preset. Profiles scale linearly with CPU count. Flag: `--performance-profile`.
    pub performance_profile: PerformanceProfile,

    /// Override concurrency limit for the primary crawl spider. Flag: `--crawl-concurrency-limit`.
    pub crawl_concurrency_limit: Option<usize>,

    /// Override concurrency limit for sitemap backfill fetches. Flag: `--backfill-concurrency-limit`.
    pub backfill_concurrency_limit: Option<usize>,

    /// Only run sitemap discovery, not a full crawl. Flag: `--sitemap-only`.
    pub sitemap_only: bool,

    /// Millisecond delay between spider requests (polite crawling). Flag: `--delay-ms`.
    pub delay_ms: u64,

    /// Per-request timeout in milliseconds; `None` uses the profile default. Flag: `--request-timeout-ms`.
    pub request_timeout_ms: Option<u64>,

    /// Number of retries on transient fetch failures. Flag: `--fetch-retries`.
    pub fetch_retries: usize,

    /// Backoff in milliseconds between retries. Flag: `--retry-backoff-ms`.
    pub retry_backoff_ms: u64,

    /// Route all job types through a single shared AMQP queue. Flag: `--shared-queue`.
    pub shared_queue: bool,

    /// PostgreSQL connection URL. Env: `AXON_PG_URL`. Flag: `--pg-url`. **Secret.**
    pub pg_url: String,

    /// Redis connection URL. Env: `AXON_REDIS_URL`. Flag: `--redis-url`. **Secret.**
    pub redis_url: String,

    /// RabbitMQ AMQP connection URL. Env: `AXON_AMQP_URL`. Flag: `--amqp-url`. **Secret.**
    pub amqp_url: String,

    /// AMQP queue name for crawl jobs. Env: `AXON_CRAWL_QUEUE`. Flag: `--crawl-queue`.
    pub crawl_queue: String,

    /// AMQP queue name for refresh jobs. Env: `AXON_REFRESH_QUEUE`. Flag: `--refresh-queue`.
    pub refresh_queue: String,

    /// AMQP queue name for extract jobs. Env: `AXON_EXTRACT_QUEUE`. Flag: `--extract-queue`.
    pub extract_queue: String,

    /// AMQP queue name for embed jobs. Env: `AXON_EMBED_QUEUE`. Flag: `--embed-queue`.
    pub embed_queue: String,

    /// AMQP queue name for ingest jobs. Env: `AXON_INGEST_QUEUE`. Flag: `--ingest-queue`.
    pub ingest_queue: String,

    /// Index Claude Code session files when running the `sessions` command. Flag: `--claude`.
    pub sessions_claude: bool,

    /// Index Codex session files when running the `sessions` command. Flag: `--codex`.
    pub sessions_codex: bool,

    /// Index Gemini session files when running the `sessions` command. Flag: `--gemini`.
    pub sessions_gemini: bool,

    /// Filter sessions by project name (substring match). Flag: `--project`.
    pub sessions_project: Option<String>,

    /// GitHub personal access token for authenticated API requests. Env: `GITHUB_TOKEN`. **Secret.**
    pub github_token: Option<String>,

    /// Also index source code files when ingesting a GitHub repository. Flag: `--include-source`.
    pub github_include_source: bool,

    /// Reddit OAuth2 client ID. Env: `REDDIT_CLIENT_ID`. **Secret.**
    pub reddit_client_id: Option<String>,

    /// Reddit OAuth2 client secret. Env: `REDDIT_CLIENT_SECRET`. **Secret.**
    pub reddit_client_secret: Option<String>,

    /// Sort order for subreddit posts. Flag: `--reddit-sort`.
    pub reddit_sort: RedditSort,

    /// Time range for top posts. Flag: `--reddit-time`.
    pub reddit_time: RedditTime,

    /// Max posts to fetch per subreddit (0 = unlimited). Flag: `--reddit-max-posts`.
    pub reddit_max_posts: usize,

    /// Minimum score for posts/comments to be indexed. Flag: `--reddit-min-score`.
    pub reddit_min_score: i32,

    /// Max comment tree depth to traverse. Flag: `--reddit-depth`.
    pub reddit_depth: usize,

    /// Scrape external links in posts and include their content. Flag: `--reddit-scrape-links`.
    pub reddit_scrape_links: bool,

    /// Base URL of the TEI (Text Embeddings Inference) service. Env: `TEI_URL`. Flag: `--tei-url`.
    pub tei_url: String,

    /// Base URL of the Qdrant vector store. Env: `QDRANT_URL`. Flag: `--qdrant-url`.
    pub qdrant_url: String,

    /// OpenAI-compatible API base URL (e.g. `http://ollama:11434/v1`). Env: `OPENAI_BASE_URL`.
    pub openai_base_url: String,

    /// API key for the OpenAI-compatible LLM endpoint. Env: `OPENAI_API_KEY`. **Secret.**
    pub openai_api_key: String,

    /// Model name to use for LLM completions (e.g. `llama3`). Env: `OPENAI_MODEL`.
    pub openai_model: String,

    /// ACP adapter command used by `pulse_chat` execution mode.
    /// Env: `AXON_ACP_ADAPTER_CMD`.
    pub acp_adapter_cmd: Option<String>,

    /// Optional ACP adapter args encoded as a pipe-delimited string
    /// (e.g. `--stdio|--model|gemini-3-flash-preview`).
    /// Env: `AXON_ACP_ADAPTER_ARGS`.
    pub acp_adapter_args: Option<String>,

    /// Tavily search API key. Env: `TAVILY_API_KEY`. **Secret.**
    pub tavily_api_key: String,

    /// Neo4j HTTP base URL for graph extraction and retrieval.
    /// Env: `AXON_NEO4J_URL`.
    pub neo4j_url: String,

    /// Neo4j username for the transactional Cypher API.
    /// Env: `AXON_NEO4J_USER`.
    pub neo4j_user: String,

    /// Neo4j password for the transactional Cypher API. Env: `AXON_NEO4J_PASSWORD`. **Secret.**
    pub neo4j_password: String,

    /// AMQP queue name for graph extraction jobs. Env: `AXON_GRAPH_QUEUE`.
    pub graph_queue: String,

    /// Parallelism for graph extraction workers. Env: `AXON_GRAPH_CONCURRENCY`.
    pub graph_concurrency: usize,

    /// Base URL for the graph extraction LLM endpoint. Env: `AXON_GRAPH_LLM_URL`.
    pub graph_llm_url: String,

    /// Model name used for graph extraction. Env: `AXON_GRAPH_LLM_MODEL`.
    pub graph_llm_model: String,

    /// Minimum similarity score for graph candidate edges. Env: `AXON_GRAPH_SIMILARITY_THRESHOLD`.
    pub graph_similarity_threshold: f64,

    /// Maximum similar candidates considered during graph extraction. Env: `AXON_GRAPH_SIMILARITY_LIMIT`.
    pub graph_similarity_limit: usize,

    /// Maximum characters of graph context injected into `ask`. Env: `AXON_GRAPH_CONTEXT_MAX_CHARS`.
    pub graph_context_max_chars: usize,

    /// Optional override path for the graph taxonomy file. Env: `AXON_GRAPH_TAXONOMY_PATH`.
    pub graph_taxonomy_path: String,

    /// Allowed cross-origin browser origins for web and MCP HTTP surfaces.
    /// Env: `AXON_WEB_ALLOWED_ORIGINS` (comma-separated).
    pub web_allowed_origins: Vec<String>,

    /// Shell WebSocket-specific allowed origins.
    /// Env: `AXON_SHELL_ALLOWED_ORIGINS` (comma-separated).
    pub shell_allowed_origins: Vec<String>,

    /// Print verbose RAG diagnostics (retrieved chunks, scores) during `ask`/`evaluate`. Flag: `--diagnostics`.
    pub ask_diagnostics: bool,

    /// Enable graph-enhanced retrieval during `ask` when Neo4j is configured. Flag: `--graph`.
    pub ask_graph: bool,

    /// Output mode for live `evaluate` answer rendering (`inline`, `side-by-side`, `events`).
    pub evaluate_responses_mode: EvaluateResponsesMode,

    /// Maximum total characters of context passed to the LLM in a single `ask` request.
    /// Env: `AXON_ASK_MAX_CONTEXT_CHARS` (clamped 20_000–400_000). Default: 120_000.
    pub ask_max_context_chars: usize,

    /// Number of candidate chunks retrieved from Qdrant before reranking.
    /// Env: `AXON_ASK_CANDIDATE_LIMIT` (clamped 8–200). Default: 64.
    pub ask_candidate_limit: usize,

    /// Maximum chunks included in the LLM context after reranking.
    /// Env: `AXON_ASK_CHUNK_LIMIT` (clamped 3–40). Default: 10.
    pub ask_chunk_limit: usize,

    /// Number of top-scoring documents for which full-doc backfill is attempted.
    /// Env: `AXON_ASK_FULL_DOCS` (clamped 1–20). Default: 4.
    pub ask_full_docs: usize,

    /// Extra chunks added from each full-doc backfill pass.
    /// Env: `AXON_ASK_BACKFILL_CHUNKS` (clamped 0–20). Default: 3.
    pub ask_backfill_chunks: usize,

    /// Maximum concurrent Qdrant fetches during full-doc backfill.
    /// Env: `AXON_ASK_DOC_FETCH_CONCURRENCY` (clamped 1–16). Default: 4.
    pub ask_doc_fetch_concurrency: usize,

    /// Maximum chunks fetched per document during backfill.
    /// Env: `AXON_ASK_DOC_CHUNK_LIMIT` (clamped 8–2000). Default: 192.
    pub ask_doc_chunk_limit: usize,

    /// Minimum Qdrant similarity score for a chunk to be included in RAG context.
    /// Env: `AXON_ASK_MIN_RELEVANCE_SCORE` (clamped -1.0–2.0). Default: 0.45.
    pub ask_min_relevance_score: f64,

    /// Authoritative domains to boost during ask reranking (exact host or suffix match).
    /// Env: `AXON_ASK_AUTHORITATIVE_DOMAINS` (comma-separated). Default: empty.
    pub ask_authoritative_domains: Vec<String>,

    /// Extra rerank score boost applied when candidate URL matches an authoritative domain.
    /// Env: `AXON_ASK_AUTHORITATIVE_BOOST` (clamped 0.0–0.5). Default: 0.0.
    pub ask_authoritative_boost: f64,

    /// Optional strict allowlist for ask retrieval candidate domains.
    /// When non-empty, candidates outside this list are excluded.
    /// Env: `AXON_ASK_AUTHORITATIVE_ALLOWLIST` (comma-separated). Default: empty.
    pub ask_authoritative_allowlist: Vec<String>,

    /// Minimum unique citations required for non-trivial ask responses.
    /// Env: `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` (clamped 1–5). Default: 2.
    pub ask_min_citations_nontrivial: usize,

    /// Enable hybrid search (dense + BM42 sparse + RRF) for Named-mode collections.
    /// Env: `AXON_HYBRID_SEARCH` (true/false/1/0). Default: true.
    pub hybrid_search_enabled: bool,

    /// Candidates fetched per prefetch arm (dense + sparse) before RRF fusion.
    /// Env: `AXON_HYBRID_CANDIDATES` (clamped 10–500). Default: 100.
    pub hybrid_search_candidates: usize,

    /// Run the command on a recurring schedule every N seconds (`None` = one-shot). Flag: `--cron-every-seconds`.
    pub cron_every_seconds: Option<u64>,

    /// Stop cron after this many runs (`None` = run forever). Flag: `--cron-max-runs`.
    pub cron_max_runs: Option<usize>,

    /// Seconds a running job may remain idle before the watchdog marks it stale.
    /// Env: `AXON_JOB_STALE_TIMEOUT_SECS`. Flag: `--watchdog-stale-timeout-secs`.
    pub watchdog_stale_timeout_secs: i64,

    /// Seconds a stale-marked job must remain unchanged before the watchdog reclaims it.
    /// Env: `AXON_JOB_STALE_CONFIRM_SECS`. Flag: `--watchdog-confirm-secs`.
    pub watchdog_confirm_secs: i64,

    /// Emit machine-readable JSON output on stdout instead of human-readable text. Flag: `--json`.
    pub json_output: bool,

    /// Status mode: include only watchdog-reclaimed jobs. Flag: `--reclaimed`.
    pub reclaimed_status_only: bool,

    /// Deduplicate trailing-slash URL variants (e.g. `/about` and `/about/` treated as one).
    /// Spider: `with_normalize(bool)`. Default false. Flag: `--normalize`.
    pub normalize: bool,

    // P2 — engine tuning (previously hardcoded in engine.rs)
    /// Seconds to wait for Chrome network idle before capturing the page.
    /// Used by `WaitForIdleNetwork`. Default: 15. Flag: `--chrome-network-idle-timeout`.
    pub chrome_network_idle_timeout_secs: u64,

    /// Thin-page ratio threshold for auto-switch from HTTP to Chrome mode (0.0–1.0).
    /// If more than this fraction of crawled pages are thin, retry with Chrome.
    /// Default: 0.60. Flag: `--auto-switch-thin-ratio`.
    pub auto_switch_thin_ratio: f64,

    /// Minimum pages crawled before auto-switch eligibility is evaluated.
    /// Prevents triggering Chrome on tiny crawls. Default: 10. Flag: `--auto-switch-min-pages`.
    pub auto_switch_min_pages: usize,

    /// Minimum broadcast channel buffer for crawl page receiver (entries, not bytes).
    /// Set by performance profile. Default (high-stable): 4096.
    pub crawl_broadcast_buffer_min: usize,

    /// Maximum broadcast channel buffer for crawl page receiver (entries, not bytes).
    /// Set by performance profile. Default (high-stable): 16_384.
    pub crawl_broadcast_buffer_max: usize,

    // P3 — missing spider builder methods
    /// URL allow-list: only crawl URLs matching at least one of these regex patterns.
    /// Complement to the URL blacklist. Default: [] (no restriction). Flag: `--url-whitelist` (repeatable).
    pub url_whitelist: Vec<String>,

    /// Block asset downloads (images, CSS, fonts, JS) during crawl to reduce bandwidth.
    /// Spider: `with_block_assets(true)`. Default: false. Flag: `--block-assets`.
    pub block_assets: bool,

    /// Maximum response size per page in bytes; pages exceeding this are skipped.
    /// Spider: `with_max_page_bytes(u64)`. Default: None (unlimited). Flag: `--max-page-bytes`.
    pub max_page_bytes: Option<u64>,

    /// Use strict redirect policy — only follow same-origin redirects.
    /// Spider: `with_redirect_policy(RedirectPolicy::Strict)`. Default: false. Flag: `--redirect-policy-strict`.
    pub redirect_policy_strict: bool,

    /// CSS selector to wait for before capturing a Chrome page.
    /// Spider: `with_wait_for_selector`. Default: None. Flag: `--chrome-wait-for-selector`.
    pub chrome_wait_for_selector: Option<String>,

    /// CSS selector to scope content extraction (e.g. `"article, main, .content"`).
    /// Spider: `root_selector`. Default: None. Flag: `--root-selector`.
    pub root_selector: Option<String>,

    /// CSS selector to exclude elements from extraction (e.g. `".sidebar, .ads"`).
    /// Spider: `exclude_selector`. Default: None. Flag: `--exclude-selector`.
    pub exclude_selector: Option<String>,

    /// Capture full-page PNG screenshots during Chrome crawl.
    /// Spider: `with_screenshot`. Saved to `output_dir`. Default: false. Flag: `--chrome-screenshot`.
    pub chrome_screenshot: bool,

    // P4 — spider_agent improvements
    /// Research crawl depth limit for the `research` command.
    /// Passed to `ResearchOptions::with_depth` if available. Default: None. Flag: `--research-depth`.
    pub research_depth: Option<usize>,

    /// Time range filter for the `search` command (values: day, week, month, year).
    /// Passed to `SearchOptions::with_time_range`. Default: None. Flag: `--search-time-range`.
    pub search_time_range: Option<String>,

    // P5 — opt-in crawl safety/compat flags
    /// Bypass Content Security Policy in Chrome — helps on pages that block inline JS via CSP.
    /// Spider: `with_csp_bypass(true)`. Chrome only. Default: false. Flag: `--bypass-csp`.
    pub bypass_csp: bool,

    /// Accept invalid/self-signed TLS certificates. Useful for internal or staging sites.
    /// Spider: `with_danger_accept_invalid_certs(true)`. Default: false. Flag: `--accept-invalid-certs`.
    pub accept_invalid_certs: bool,

    /// Capture the full scrollable page (true) or just the viewport (false).
    /// Default: true. Flag: `--screenshot-full-page`.
    pub screenshot_full_page: bool,

    /// Viewport width in pixels for screenshot capture. Default: 1920. Flag: `--viewport`.
    pub viewport_width: u32,

    /// Viewport height in pixels for screenshot capture. Default: 1080. Flag: `--viewport`.
    pub viewport_height: u32,

    /// Port for the `serve` web UI server. Flag: `--port`, env: `AXON_SERVE_PORT`. Default: 49000.
    pub serve_port: u16,

    /// MCP transport mode. Env: `AXON_MCP_TRANSPORT`. Flag: `axon mcp --transport`.
    pub mcp_transport: McpTransport,

    /// Host interface for MCP HTTP transport. Env: `AXON_MCP_HTTP_HOST`. Default: `0.0.0.0`.
    pub mcp_http_host: String,

    /// Port for MCP HTTP transport. Env: `AXON_MCP_HTTP_PORT`. Default: `8001`.
    pub mcp_http_port: u16,

    /// Custom HTTP request headers in `"Key: Value"` format (repeatable). Flag: `--header`.
    pub custom_headers: Vec<String>,
}
