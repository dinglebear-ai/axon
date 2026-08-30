use super::enums::{
    CommandKind, EvaluateResponsesMode, McpTransport, PerformanceProfile, RedditSort, RedditTime,
    RenderMode, ScrapeFormat,
};
use crate::llm::LlmBackendKind;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdaptiveConcurrencyConfig {
    pub enabled: bool,
    pub min: usize,
    pub max: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionBatchConfig {
    pub max_inputs: usize,
    pub max_request_bytes: usize,
    pub max_input_bytes: usize,
    pub max_query_bytes: usize,
    pub max_idempotency_key_bytes: usize,
    pub max_aggregate_input_bytes: usize,
    pub max_response_bytes: usize,
    pub max_pages: u64,
    pub max_manifest_items: u64,
    pub max_fetched_bytes_per_item: u64,
    pub max_decompressed_bytes_per_item: u64,
    pub max_prepared_bytes: u64,
    pub max_documents: u64,
    pub max_chunks: u64,
    pub max_vector_points: u64,
    pub max_elapsed_secs: u64,
    pub max_query_window: usize,
    pub global_max_in_flight_requests: usize,
    pub caller_max_in_flight_requests: usize,
    pub caller_rate_limit_per_minute: u64,
}

impl ProjectionBatchConfig {
    pub fn validate(&self) -> Result<(), String> {
        let fields = [
            ("server.projection-batch.max-inputs", self.max_inputs as u64),
            (
                "server.projection-batch.max-request-bytes",
                self.max_request_bytes as u64,
            ),
            (
                "server.projection-batch.max-input-bytes",
                self.max_input_bytes as u64,
            ),
            (
                "server.projection-batch.max-query-bytes",
                self.max_query_bytes as u64,
            ),
            (
                "server.projection-batch.max-idempotency-key-bytes",
                self.max_idempotency_key_bytes as u64,
            ),
            (
                "server.projection-batch.max-aggregate-input-bytes",
                self.max_aggregate_input_bytes as u64,
            ),
            (
                "server.projection-batch.max-response-bytes",
                self.max_response_bytes as u64,
            ),
            ("server.projection-batch.max-pages", self.max_pages),
            (
                "server.projection-batch.max-manifest-items",
                self.max_manifest_items,
            ),
            (
                "server.projection-batch.max-fetched-bytes-per-item",
                self.max_fetched_bytes_per_item,
            ),
            (
                "server.projection-batch.max-decompressed-bytes-per-item",
                self.max_decompressed_bytes_per_item,
            ),
            (
                "server.projection-batch.max-prepared-bytes",
                self.max_prepared_bytes,
            ),
            ("server.projection-batch.max-documents", self.max_documents),
            ("server.projection-batch.max-chunks", self.max_chunks),
            (
                "server.projection-batch.max-vector-points",
                self.max_vector_points,
            ),
            (
                "server.projection-batch.max-elapsed-secs",
                self.max_elapsed_secs,
            ),
            (
                "server.projection-batch.max-query-window",
                self.max_query_window as u64,
            ),
            (
                "server.projection-batch.global-max-in-flight-requests",
                self.global_max_in_flight_requests as u64,
            ),
            (
                "server.projection-batch.caller-max-in-flight-requests",
                self.caller_max_in_flight_requests as u64,
            ),
            (
                "server.projection-batch.caller-rate-limit-per-minute",
                self.caller_rate_limit_per_minute,
            ),
        ];
        if let Some((field, _)) = fields.into_iter().find(|(_, value)| *value == 0) {
            return Err(format!("{field} must be greater than zero"));
        }
        if self.max_input_bytes > self.max_aggregate_input_bytes {
            return Err(
                "server.projection-batch.max-input-bytes must not exceed max-aggregate-input-bytes"
                    .to_string(),
            );
        }
        if self.caller_max_in_flight_requests > self.global_max_in_flight_requests {
            return Err("server.projection-batch.caller-max-in-flight-requests must not exceed global-max-in-flight-requests".to_string());
        }
        Ok(())
    }
}

impl Default for ProjectionBatchConfig {
    fn default() -> Self {
        Self {
            max_inputs: 32,
            max_request_bytes: 1024 * 1024,
            max_input_bytes: 16 * 1024,
            max_query_bytes: 16 * 1024,
            max_idempotency_key_bytes: 256,
            max_aggregate_input_bytes: 256 * 1024,
            max_response_bytes: 8 * 1024 * 1024,
            max_pages: 1_000,
            max_manifest_items: 10_000,
            max_fetched_bytes_per_item: 16 * 1024 * 1024,
            max_decompressed_bytes_per_item: 32 * 1024 * 1024,
            max_prepared_bytes: 64 * 1024 * 1024,
            max_documents: 10_000,
            max_chunks: 100_000,
            max_vector_points: 100_000,
            max_elapsed_secs: 900,
            max_query_window: 1_000,
            global_max_in_flight_requests: 64,
            caller_max_in_flight_requests: 8,
            caller_rate_limit_per_minute: 120,
        }
    }
}

#[derive(Clone)]
pub struct Config {
    /// Server-owned Labby integration used to resolve revision-bound ask/chat context.
    pub labby_url: Option<String>,
    pub labby_service_token: Option<String>,
    pub labby_integration_id: Option<String>,
    pub labby_runtime_identity: Option<String>,
    pub labby_resolution_timeout_ms: u64,
    pub labby_resolution_max_bytes: usize,
    /// The subcommand being executed (scrape, crawl, ask, etc.).
    pub command: CommandKind,

    /// Primary URL argument; used by scrape, crawl, map, and similar single-URL commands.
    pub start_url: String,

    /// Positional arguments after the subcommand (URLs, query text, job sub-subcommand tokens).
    pub positional: Vec<String>,

    /// Canonical JSON projection items supplied through repeatable `--item`.
    pub projection_items: Vec<String>,

    /// Canonical focused request supplied through `--request-file`.
    pub projection_request_file: Option<PathBuf>,
    pub projection_output_dir: Option<PathBuf>,
    pub projection_output_template: Option<String>,

    /// Comma-separated URL list provided via `--urls` (alternative to positional arguments).
    pub urls_csv: Option<String>,

    /// Glob patterns to expand into seed URLs (e.g. `https://docs.rs/foo/**`).
    pub url_glob: Vec<String>,

    /// Query text for `query`, `ask`, and `evaluate` commands; also settable via `--query`.
    pub query: Option<String>,

    /// Maximum number of results returned by `query`/`search` commands. Flag: `--limit`.
    pub search_limit: usize,

    /// Maximum chunks fetched by `retrieve` before reconstructing the document.
    /// Flag: `retrieve --max-points` (`retrieve --limit` alias). Default: None
    /// (use the retrieve service ceiling).
    pub retrieve_max_points: Option<usize>,

    /// Non-interactive 1-based candidate rank for `train --best`.
    pub train_best_rank: Option<usize>,

    /// Optional free-form note stored with `train` preference events.
    pub train_notes: Option<String>,

    /// Maximum pages to crawl (crawl defaults to 2000 via parser; 0 = uncapped).
    /// Flag: `--max-pages`.
    pub max_pages: u32,

    /// Maximum crawl depth from the start URL. Flag: `--max-depth`.
    pub max_depth: usize,

    /// Whether to follow links from subdomains of the start URL. Flag: `--include-subdomains`.
    pub include_subdomains: bool,

    /// URL path prefixes to skip during crawl (e.g. `/blog/`, `/legacy/`). Flag: `--exclude-path-prefix`.
    pub exclude_path_prefix: Vec<String>,

    /// Repo-relative path substrings to skip during git ingest (e.g. `docs/references/`,
    /// `vendor/`). A file is excluded when its repo-relative path contains any entry.
    /// Flag: `--exclude-path` (repeatable). Empty = no extra exclusions.
    pub ingest_exclude_paths: Vec<String>,

    /// Directory for saved markdown/HTML output files. Flag: `--output-dir`.
    pub output_dir: PathBuf,

    /// Explicit single-file output path (overrides `output_dir` for single-URL commands). Flag: `--output`.
    pub output_path: Option<PathBuf>,

    /// When set, write every fetched page of a crawl to a WARC 1.1 archive at
    /// this path. Crawl path only (HTTP and Chrome both archive). Flag: `--warc`.
    pub warc_output: Option<PathBuf>,

    /// When set, path to a JSON file mapping URL path prefixes to ordered
    /// Chrome web-automation steps (click/scroll/wait/evaluate/…) run during a
    /// Chrome crawl before each matching page is captured. Flag: `--automation-script`.
    pub automation_script: Option<PathBuf>,

    /// Browser rendering strategy: `http`, `chrome`, or `auto-switch`. Flag: `--render-mode`.
    pub render_mode: RenderMode,

    /// URL of the Chrome DevTools Protocol (CDP) management endpoint. Env: `AXON_CHROME_REMOTE_URL`.
    pub chrome_remote_url: Option<String>,

    /// HTTP proxy URL for Chrome requests. Env: `AXON_CHROME_PROXY`.
    pub chrome_proxy: Option<String>,

    /// General-purpose `User-Agent` for all HTTP requests. Env: `AXON_USER_AGENT`.
    /// Chrome-specific paths fall back to this when `chrome_user_agent` is `None`.
    pub user_agent: Option<String>,

    /// Custom `User-Agent` header sent by Chrome. Env: `AXON_CHROME_USER_AGENT`.
    /// Falls back to `user_agent` when unset.
    pub chrome_user_agent: Option<String>,

    /// Timeout in milliseconds to wait for Chrome bootstrap. TOML: `chrome.bootstrap-timeout-ms`.
    pub chrome_bootstrap_timeout_ms: u64,

    /// Number of retries for Chrome bootstrap failures. TOML: `chrome.bootstrap-retries`.
    pub chrome_bootstrap_retries: usize,

    /// Push Spider/Chromey's remote local policy to capable Chrome engines. TOML: `chrome.remote-local-policy`.
    pub chrome_remote_local_policy: bool,

    /// Whether to honour `robots.txt` directives. Defaults `false`. TOML: `scrape.respect-robots`.
    pub respect_robots: bool,

    /// Pages with fewer than this many markdown characters are treated as "thin". TOML: `scrape.min-markdown-chars`.
    pub min_markdown_chars: usize,

    /// Drop thin pages — do not save or embed them. TOML: `scrape.drop-thin-markdown`.
    pub drop_thin_markdown: bool,

    /// Discover and backfill URLs from `sitemap.xml` after the main crawl. TOML: `scrape.discover-sitemaps`.
    pub discover_sitemaps: bool,

    /// Only backfill sitemap URLs with `<lastmod>` within the last N days (0 = no filter). TOML: `scrape.sitemap-since-days`.
    pub sitemap_since_days: u32,

    /// Fetch and scan first-party JavaScript bundles during endpoint discovery.
    pub endpoints_include_bundles: bool,

    /// Filter endpoint discovery results to first-party endpoints only.
    pub endpoints_first_party_only: bool,

    /// Deduplicate endpoint discovery results by normalized endpoint URL.
    pub endpoints_unique_only: bool,

    /// Maximum script bundle URLs considered for endpoint discovery.
    pub endpoints_max_scripts: usize,

    /// Maximum HTML + JavaScript bytes scanned by endpoint discovery.
    pub endpoints_max_scan_bytes: usize,

    /// Run safe unauthenticated endpoint verification probes after static discovery.
    pub endpoints_verify: bool,

    /// Capture browser network requests for endpoint discovery.
    pub endpoints_capture_network: bool,

    /// Probe discovered endpoints for JSON-RPC 2.0, MCP, and ACP protocol support.
    pub endpoints_probe_rpc: bool,

    /// Also synthesize + probe `mcp.<registrable-apex>` MCP candidates. No-op
    /// without `endpoints_probe_rpc`.
    pub endpoints_probe_rpc_subdomains: bool,

    /// Maximum number of sitemap documents to parse per map/backfill operation
    /// (0 = unlimited). TOML: `scrape.max-sitemaps`.
    pub max_sitemaps: usize,

    /// Probe `/llms.txt` at the site root and backfill its listed URLs after the main crawl,
    /// and merge them into `map` discovery. TOML: `scrape.discover-llms-txt`.
    pub discover_llms_txt: bool,

    /// Maximum number of URLs to take from a single `/llms.txt` after scope filtering
    /// (0 = unlimited). A flat llms.txt has no document-count bound, so this caps fan-out.
    /// TOML: `scrape.max-llms-txt-urls`.
    pub max_llms_txt_urls: usize,

    /// Enable Spider's built-in crawl-result caching. Flag: `--cache`.
    pub cache: bool,

    /// Keep cached crawl flow on the HTTP path and suppress Chrome runtime/bootstrap. Flag: `--cache-http-only`.
    pub cache_http_only: bool,

    /// Enable conditional re-crawl (ETag / If-Modified-Since). When set, the crawl
    /// engine seeds spider's per-`Website` ETag cache from a persisted sidecar and
    /// reconciles pages that 304 (and are therefore silently skipped by spider) back
    /// into the manifest as reused entries. Requires `cache` for the markdown.old
    /// archive used to relink reused pages. Flag: `--etag-conditional`. Bead
    /// axon_rust-hiyf.
    pub etag_conditional: bool,

    /// Per-path crawl budgets parsed from `--budget PATH=N`. Each entry caps the
    /// pages crawled under a path prefix; the key `*` applies to all paths. Empty
    /// = no budget (current behavior). Owned `String` keys so they can outlive the
    /// borrowed `HashMap<&str, u32>` passed to spider's `with_budget`. Bead
    /// axon_rust-37zv.
    pub path_budgets: Vec<(String, u32)>,

    /// Output format for scraped pages: `markdown`, `html`, `rawHtml`, or `json`. Flag: `--format`.
    pub format: ScrapeFormat,

    /// Qdrant collection name to read from and write to. Env: `AXON_COLLECTION`. Flag: `--collection`.
    pub collection: String,

    /// Automatically embed scraped content into Qdrant after fetching. Disabled by `--skip-embed`.
    pub embed: bool,

    /// Local filesystem roots allowed for source requests from server transports.
    /// Env: `AXON_SOURCE_LOCAL_ALLOWED_ROOTS` (comma-separated).
    pub source_local_allowed_roots: Vec<PathBuf>,

    /// Max bytes for one local file embedded through server surfaces.
    /// Env: `AXON_MCP_EMBED_MAX_LOCAL_BYTES`.
    pub mcp_embed_max_local_bytes: u64,

    /// Max recursive directory depth for local embed validation.
    /// Env: `AXON_MCP_EMBED_MAX_LOCAL_DEPTH`.
    pub mcp_embed_max_local_depth: usize,

    /// Max filesystem entries visited for local embed validation.
    /// Env: `AXON_MCP_EMBED_MAX_LOCAL_ENTRIES`.
    pub mcp_embed_max_local_entries: usize,

    /// Number of concurrent connections for batch operations (clamped 1–512). Flag: `--batch-concurrency`.
    pub batch_concurrency: usize,

    /// Shared transport-neutral admission and owning-stage ceilings.
    pub projection_batch: ProjectionBatchConfig,

    /// Block until async jobs complete instead of fire-and-forgetting. Flag: `--wait`.
    pub wait: bool,

    /// Path to the SQLite jobs database file. Env: `AXON_SQLITE_PATH`.
    pub sqlite_path: PathBuf,

    /// Skip confirmation prompts (non-interactive mode). Flag: `--yes`.
    pub yes: bool,

    /// Binary acquisition method passed from install.sh via `axon setup --method pull|build`.
    /// `None` when setup is run directly (not via install.sh).
    pub setup_method: Option<String>,

    /// Acquisition scope override for `axon <source>` / `axon source <input>`.
    /// Flag: `--scope <page|site|...>`. `None` uses the adapter's default scope.
    pub source_scope: Option<String>,

    /// Scheduler priority for source jobs. CLI `--priority` overrides `[jobs].default-priority`.
    pub source_priority: Option<String>,

    /// Operator gate for executable CLI/MCP tool sources.
    /// Env `AXON_ALLOW_TOOL_EXECUTION` overrides `[security].allow-tool-execution`.
    pub allow_tool_execution: bool,

    /// Request inline cleaned-page output for retained `axon scrape`.
    pub scrape_inline: bool,

    /// Stores selected for `axon reset` (`jobs`/`ledger`/`graph`/`memory`/
    /// `vectors`/`artifacts`). Empty = every store. Flag: `reset --stores a,b`.
    pub reset_stores: Vec<String>,

    /// Force `axon reset` to stay a dry-run plan even under `--yes`. Flag:
    /// `reset --dry-run`. Reset is dry-run by default regardless; this pins it.
    pub reset_dry_run: bool,

    /// Prune target for `axon prune plan`/`axon prune exec`: a source id, or
    /// `collection:<name>` to target a whole Qdrant collection.
    pub prune_target: Option<String>,

    /// Scope a `axon prune` selector to one generation of `prune_target`
    /// instead of the whole source. Flag: `prune plan|exec --generation`.
    pub prune_generation: Option<String>,

    /// Explicit destructive confirmation for `axon prune exec`. Flag:
    /// `prune exec --confirm`.
    pub prune_confirm: bool,

    /// Optional reusable plan id for destructive reset execution.
    pub reset_plan_id: Option<String>,

    /// Terminal color override. Flag: `--color=auto|always|never`.
    pub color_choice: super::enums::ColorChoice,

    /// Terminal motion override. Flag: `--motion=auto|always|never`.
    pub motion_choice: super::enums::MotionChoice,

    /// Operator diagnostic detail. `0` is quiet-by-default, `1` is `-v`, and
    /// `2+` is debug-oriented `-vv` output.
    pub verbosity: u8,

    /// Live-update mode for `axon status` and `axon monitor jobs`. Flag: `--watch`.
    pub watch_mode: bool,

    /// Concurrency/timeout preset. Profiles scale linearly with CPU count. Flag: `--performance-profile`.
    pub performance_profile: PerformanceProfile,

    /// Override concurrency limit for the primary crawl spider. TOML: `workers.crawl-concurrency-limit`.
    pub crawl_concurrency_limit: Option<usize>,

    /// Override concurrency limit for sitemap backfill fetches. TOML: `workers.backfill-concurrency-limit`.
    pub backfill_concurrency_limit: Option<usize>,

    /// Global durable capacity for the injected HTTP fetch provider.
    /// Env: `AXON_FETCH_CONCURRENCY`. TOML: `providers.fetch.concurrency`.
    pub fetch_provider_concurrency: usize,

    /// Global durable capacity for the injected render provider.
    /// Env: `AXON_RENDER_MAX_CONCURRENT_PAGES`. TOML: `providers.render.max-concurrent-pages`.
    pub render_provider_concurrency: usize,

    /// Opt-in Spider adaptive crawl semaphore settings. TOML: `workers.adaptive-concurrency`.
    pub adaptive_concurrency: AdaptiveConcurrencyConfig,

    /// Only run sitemap discovery, not a full crawl. Flag: `--sitemap-only`.
    pub sitemap_only: bool,

    /// Millisecond delay between spider requests (polite crawling). TOML: `scrape.delay-ms`.
    pub delay_ms: u64,

    /// Per-request timeout in milliseconds; `None` uses the profile default. TOML: `scrape.request-timeout-ms`.
    pub request_timeout_ms: Option<u64>,

    /// End-to-end timeout for one service-level scrape batch.
    /// Env: `AXON_SCRAPE_BATCH_TIMEOUT_SECS`. TOML: `scrape.batch-timeout-secs`.
    pub scrape_batch_timeout_secs: u64,

    /// Number of retries on transient fetch failures. TOML: `scrape.fetch-retries`.
    pub fetch_retries: usize,

    /// Backoff in milliseconds between retries. TOML: `scrape.retry-backoff-ms`.
    pub retry_backoff_ms: u64,

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

    /// GitLab personal/project/group access token for authenticated API requests. Env: `GITLAB_TOKEN`. **Secret.**
    pub gitlab_token: Option<String>,

    /// Gitea/Forgejo access token for authenticated API requests. Env: `GITEA_TOKEN`. **Secret.**
    pub gitea_token: Option<String>,

    /// Also index source code files when ingesting a GitHub repository. Flag: `--include-source`.
    pub github_include_source: bool,

    /// Maximum issues to fetch per GitHub repository (0 = unlimited). Flag: `--max-issues`. Env: `GITHUB_MAX_ISSUES`.
    pub github_max_issues: usize,

    /// Maximum pull requests to fetch per GitHub repository (0 = unlimited). Flag: `--max-prs`. Env: `GITHUB_MAX_PRS`.
    pub github_max_prs: usize,

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

    /// Gemini-specific model override for headless LLM synthesis.
    /// Env: `AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL` or legacy `AXON_HEADLESS_GEMINI_MODEL`.
    pub headless_gemini_model: String,

    /// Gemini-specific model override for direct chat.
    /// Env: `AXON_CHAT_HEADLESS_GEMINI_MODEL`.
    pub headless_gemini_chat_model: String,

    /// LLM completion backend. Env: `AXON_LLM_BACKEND`.
    pub llm_backend: LlmBackendKind,

    /// Gemini CLI command for headless LLM synthesis. Env: `AXON_HEADLESS_GEMINI_CMD`.
    pub headless_gemini_cmd: String,

    /// Source HOME for Gemini CLI auth isolation. Env: `AXON_HEADLESS_GEMINI_HOME`.
    pub headless_gemini_home: Option<PathBuf>,

    /// Codex CLI command for app-server LLM synthesis. Env: `AXON_CODEX_CMD`.
    pub codex_cmd: String,

    /// Source CODEX_HOME for Codex auth isolation. Env: `AXON_CODEX_HOME`.
    pub codex_home: Option<PathBuf>,

    /// Codex-specific model override for synthesis. Env: `AXON_SYNTHESIS_CODEX_MODEL` or `AXON_CODEX_MODEL`.
    pub codex_model: String,

    /// Codex process-pool size (and max concurrent turns). Env: `AXON_CODEX_COMPLETION_CONCURRENCY`.
    /// Default 4 — startup cost is amortised across turns so a larger default is safe.
    pub codex_completion_concurrency: usize,

    /// Load the user's real Codex config (MCP servers, skills, hooks) instead of
    /// the isolated, stripped throwaway `CODEX_HOME`. Env: `AXON_CODEX_LOAD_USER_CONFIG`.
    /// Default `false` — opt-in escape hatch that surrenders synthesis isolation.
    pub codex_load_user_config: bool,

    /// Enables the separately supervised Codex management runtime.
    pub codex_control_enabled: bool,
    /// Stable CODEX_HOME used only by the control runtime.
    pub codex_control_home: Option<PathBuf>,
    /// Timeout for each Codex control-plane request.
    pub codex_control_request_timeout_secs: u64,
    /// Maximum concurrent Codex control-plane reads.
    pub codex_control_read_concurrency: usize,
    /// Allows account login/logout mutations through Codex control.
    pub codex_control_account_writes: bool,
    /// Allows config mutations through Codex control.
    pub codex_control_config_writes: bool,
    /// Allows MCP server configuration/reload/OAuth mutations.
    pub codex_control_mcp_writes: bool,
    /// Allows plugin install/uninstall mutations.
    pub codex_control_plugin_writes: bool,
    /// Allows skill configuration and external-agent imports.
    pub codex_control_skill_writes: bool,

    /// Max concurrent LLM completion requests across the selected backend. Env: `AXON_LLM_COMPLETION_CONCURRENCY`.
    pub llm_completion_concurrency: usize,

    /// Timeout for each LLM completion request across the selected backend. Env: `AXON_LLM_COMPLETION_TIMEOUT_SECS`.
    pub llm_completion_timeout_secs: u64,

    /// How long (seconds) an idle pooled `codex app-server` child may sit
    /// unused before the pool discards it on next checkout; 0 disables TTL
    /// eviction. Env: `AXON_CODEX_POOL_IDLE_TTL_SECS`. TOML: `llm.codex-pool-idle-ttl-secs`.
    pub codex_pool_idle_ttl_secs: u64,

    /// OpenAI-compatible API base URL, e.g. llama.cpp `http://127.0.0.1:8080/v1`.
    /// Env: `AXON_OPENAI_BASE_URL`.
    pub openai_base_url: String,

    /// Optional API key for OpenAI-compatible endpoints. Env: `AXON_OPENAI_API_KEY`.
    pub openai_api_key: String,

    /// Model name for OpenAI-compatible synthesis completions. Env: `AXON_SYNTHESIS_OPENAI_MODEL`.
    pub openai_model: String,

    /// Model name for OpenAI-compatible direct chat. Env: `AXON_CHAT_OPENAI_MODEL`.
    pub openai_chat_model: String,

    /// Tavily search API key. Env: `TAVILY_API_KEY`. **Secret.**
    pub tavily_api_key: String,

    /// Base URL of a self-hosted SearXNG instance used as the `research` search
    /// backend (e.g. `https://searx.example.com`). When set, `research` queries
    /// SearXNG's JSON API instead of Tavily. Env: `AXON_SEARXNG_URL`.
    pub searxng_url: String,

    /// When true (default), `research` fetches each top source's full page and
    /// synthesizes over it; when false it synthesizes over search snippets only
    /// (much faster). Env: `AXON_RESEARCH_FULL_CONTENT` (`false`/`0`/`no`/`off`
    /// to disable).
    pub research_full_content: bool,

    /// Allowed cross-origin browser origins for the MCP HTTP surface.
    /// Env: `AXON_ALLOWED_ORIGINS` (comma-separated).
    pub mcp_allowed_origins: Vec<String>,

    /// Print verbose RAG diagnostics (retrieved chunks, scores) during `ask`/`evaluate`. Flag: `--diagnostics`.
    pub ask_diagnostics: bool,

    /// Emit per-candidate ask explain trace and skip LLM synthesis. Flag: `ask --explain`.
    pub ask_explain: bool,

    /// Stream answer tokens to stdout as they arrive. Flag: `ask --stream`.
    pub ask_stream: bool,

    /// Include recent turns from the selected local ask session. Flag: `ask --follow-up`.
    pub ask_follow_up: bool,

    /// Local ask session name for saved turns and follow-up context. Flag: `ask --session`.
    pub ask_session: Option<String>,

    /// Rendered local ask session history injected as a citable source for follow-up synthesis.
    pub ask_follow_up_context: Option<String>,

    /// Clear the selected local ask session before running. Flag: `ask --reset-session`.
    pub ask_reset_session: bool,

    /// Force a fresh ask session, overwriting any existing one. Flag: `ask --new-session`.
    pub ask_new_session: bool,

    /// List all local ask sessions and exit without running a query. Flag: `ask --list-sessions`.
    pub ask_list_sessions: bool,

    /// Output mode for live `evaluate` answer rendering (`inline`, `side-by-side`, `events`).
    pub evaluate_responses_mode: EvaluateResponsesMode,

    /// Maximum total characters of context passed to the LLM in a single `ask` request.
    /// Env: `AXON_ASK_MAX_CONTEXT_CHARS` (clamped 20_000–1_000_000). Default: 300_000.
    pub ask_max_context_chars: usize,

    /// Number of candidate chunks retrieved from Qdrant before reranking.
    /// Env: `AXON_ASK_CANDIDATE_LIMIT` (clamped 8–300). Default: 250.
    pub ask_candidate_limit: usize,

    /// Maximum chunks included in the LLM context after reranking.
    /// Env: `AXON_ASK_CHUNK_LIMIT` (clamped 3–64). Default: 24.
    pub ask_chunk_limit: usize,

    /// Number of top-scoring documents for which full-doc backfill is attempted.
    /// Env: `AXON_ASK_FULL_DOCS` (clamped 1–20). Default: 6.
    pub ask_full_docs: usize,

    /// True when `ask_full_docs` was set explicitly by the user (via
    /// `AXON_ASK_FULL_DOCS` env var or a CLI flag) rather than left at the
    /// hardcoded default. The adaptive resolver in
    /// `build_ask_context` honours user overrides and only applies its
    /// complexity-based default when this is `false`.
    /// (bd axon_rust-721)
    pub ask_full_docs_explicit: bool,

    /// Extra chunks added from each full-doc backfill pass.
    /// Env: `AXON_ASK_BACKFILL_CHUNKS` (clamped 0–20). Default: 5.
    pub ask_backfill_chunks: usize,

    /// Maximum concurrent Qdrant fetches during full-doc backfill.
    /// Env: `AXON_ASK_DOC_FETCH_CONCURRENCY` (clamped 1–16). Default: 4.
    pub ask_doc_fetch_concurrency: usize,

    /// Maximum chunks fetched per document during backfill.
    /// Env: `AXON_ASK_DOC_CHUNK_LIMIT` (clamped 8–2000). Default: 96.
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

    /// Minimum unique citations required for non-trivial ask responses.
    /// Env: `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` (clamped 1–5). Default: 2.
    pub ask_min_citations_nontrivial: usize,

    /// Explicit override for whether the configured synthesis backend has a
    /// large context window, driving the adaptive full-docs context floor in
    /// the `ask` path. `None` (default) falls back to the substring-heuristic
    /// in `high_context_synthesis_model` that infers capability from the model
    /// name. `Some(true)`/`Some(false)` force the decision regardless of the
    /// model name, so a new high-context model can be flagged without code
    /// changes (arch-M4). No behavior change unless the operator sets the knob.
    /// Env: `AXON_SYNTHESIS_HIGH_CONTEXT` (true/false/1/0). TOML: `[llm] synthesis-high-context`.
    pub synthesis_high_context: Option<bool>,

    /// Enable hybrid search (dense + BM42 sparse + RRF) for Named-mode collections.
    /// Env: `AXON_HYBRID_SEARCH` (true/false/1/0). Default: true.
    pub hybrid_search_enabled: bool,

    /// `evaluate` flag: replace the no-context baseline lane with a second RAG run that has
    /// hybrid retrieval disabled (dense-only). The judge then compares hybrid-RAG vs dense-RAG.
    /// CLI: `--retrieval-ab`. Default: false.
    pub evaluate_retrieval_ab: bool,

    /// Candidates fetched per prefetch arm (dense + sparse) before RRF fusion.
    /// Env: `AXON_HYBRID_CANDIDATES` (clamped 10–500). Default: 100.
    pub hybrid_search_candidates: usize,

    /// Candidates fetched per prefetch arm before RRF fusion, for the `ask` pipeline only.
    ///
    /// Ask reranks with `ask_min_relevance_score` (default 0.45) before selecting context,
    /// so it needs a wider prefetch window than `query` (which skips reranking).
    /// Env: `AXON_ASK_HYBRID_CANDIDATES` (clamped 10–500). Default: 150.
    pub ask_hybrid_candidates: usize,

    /// Enable the in-process document-chunk cache for `ask` (full-doc fetch path).
    /// Process-local cache: only useful in long-lived parents (`axon serve`, `axon mcp`).
    /// CLI one-shots see zero hit rate. Config-only via `[ask.cache] enabled` in
    /// `~/.axon/config.toml`. Default: false. (bd axon_rust-pmc)
    pub ask_cache_enabled: bool,

    /// Maximum bytes (summed `chunk_text` length) the doc-chunk cache may hold.
    /// Config-only via `[ask.cache] max-capacity-bytes`. Default: 256 MiB.
    pub ask_cache_max_capacity_bytes: u64,

    /// Time-to-live for cached doc-chunk entries, in seconds. Capped at 300s
    /// (security primitive: bounds staleness of deleted content).
    /// Config-only via `[ask.cache] ttl-secs`. Default: 300s.
    pub ask_cache_ttl_secs: u64,

    /// Enable the adaptive full-doc fetch skip gate for `ask`. When the top-K
    /// reranked candidates already cover enough URLs, bytes, and quality, the
    /// full-doc backfill stage is elided entirely. Default: false. Config-only via
    /// `[ask.adaptive] fulldoc-skip-enabled`. (bd axon_rust-30y)
    pub ask_fulldoc_skip_enabled: bool,

    /// Minimum unique URLs required in the reranked top-K for the skip gate
    /// to fire. Config-only via `[ask.adaptive] fulldoc-skip-min-urls`.
    /// Default: 3.
    pub ask_fulldoc_skip_min_urls: usize,

    /// Minimum total chunk_text bytes (summed across reranked top-K) required
    /// for the skip gate to fire. Config-only via
    /// `[ask.adaptive] fulldoc-skip-min-chars`. Default: 4000.
    pub ask_fulldoc_skip_min_chars: usize,

    /// Cosine-mode score floor offset added on top of `ask_min_relevance_score`.
    /// Every score in the reranked top-K must be `>= ask_min_relevance_score +
    /// ask_fulldoc_skip_score_delta` for the gate to fire on cosine paths.
    /// Ignored on RRF paths (rank-fusion output is unitless and uses a
    /// rank-based gate instead). Config-only via
    /// `[ask.adaptive] fulldoc-skip-score-delta`. Default: 0.15.
    pub ask_fulldoc_skip_score_delta: f64,

    /// Maximum TEI embed retry attempts after the initial request.
    /// Env: `TEI_MAX_RETRIES`. TOML: `providers.embedding.max-retries`. Clamped 0–20. Default: 5.
    pub tei_max_retries: usize,

    /// Per-attempt timeout in milliseconds for TEI embed requests.
    /// Env: `TEI_REQUEST_TIMEOUT_MS`. TOML: `providers.embedding.request-timeout-ms`. Clamped 1000–300_000. Default: 30_000.
    pub tei_request_timeout_ms: u64,

    /// Default client-side batch size for TEI embed requests (auto-splits on HTTP 413).
    /// Env: `TEI_MAX_CLIENT_BATCH_SIZE`. TOML: `providers.embedding.batch-size`. Clamped 1–256. Default: 96.
    pub tei_max_client_batch_size: usize,

    /// Max concurrent requests to native TEI `/embed`. This value sizes both
    /// the target local-source scheduler's logical embed-call capacity and the
    /// process-shared HTTP request semaphore reused by TEI providers with the
    /// same endpoint/admission profile. Each logical `embed()` call reserves one flat scheduler unit,
    /// while its internal client batches share this process-wide HTTP ceiling;
    /// `embed_tei_max_in_flight_inputs` independently bounds aggregate chunk
    /// inputs across those requests.
    /// Env: `AXON_TEI_MAX_CONCURRENT`. TOML: `providers.embedding.max-concurrent-requests`. Clamped 1–64. Default: 8.
    pub embed_tei_max_concurrent: usize,

    /// Weighted cap on input chunks simultaneously admitted to native TEI
    /// `/embed` HTTP requests. TEI providers with the same endpoint/admission
    /// profile reuse a process-shared weighted semaphore sized from this value,
    /// so concurrent logical `embed()` calls or separately constructed read/write
    /// providers cannot multiply the input budget. The scheduler reservation pool remains
    /// intentionally request-oriented and reserves one flat unit per logical
    /// `embed()` call; this field constrains the transport's aggregate chunk
    /// fanout underneath those scheduler slots.
    /// Env: `AXON_TEI_MAX_IN_FLIGHT_INPUTS`. TOML: `providers.embedding.max-in-flight-inputs`. Clamped 1–4096. Default: 320.
    pub embed_tei_max_in_flight_inputs: usize,

    /// Base backoff (before exponential growth + jitter) between retried TEI
    /// embed requests on 429/5xx. Was previously a hardcoded 1000ms literal in
    /// `axon-embedding`'s TEI client — see `[providers.embedding].retry-backoff-ms`
    /// in `docs/pipeline-unification/configuration/config-contract.md`.
    /// Env: `AXON_TEI_RETRY_BACKOFF_MS`. TOML: `providers.embedding.retry-backoff-ms`. Clamped 50–60000. Default: 500.
    pub embed_tei_retry_backoff_ms: u64,

    /// Consecutive reservation failures before the target local-source
    /// embedding reservation pool (`axon-services::context::target_runtime`'s
    /// `embedding_reservation_config`) starts rejecting non-interactive
    /// reservations. Was previously a hardcoded constant
    /// (`RESERVATION_COOLDOWN_AFTER_FAILURES = 1`) in `axon-services`.
    ///
    /// Deliberately NOT wired into `TeiEmbeddingProvider`'s own self-tracked
    /// health tracker (`axon-embedding`'s `HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES`),
    /// which stays fixed at 1: that tracker reports cooling after a single
    /// `embed()` call has already exhausted its full internal HTTP retry
    /// budget, a distinct invariant validated by
    /// `embed_retry_exhaustion_cools_the_provider_and_capabilities_report_it_live`
    /// (provider-contract F5-10..13/V01/V03) — raising it there would make a
    /// single genuinely-exhausted request stop reporting `Cooling`.
    /// Env: `AXON_TEI_COOLDOWN_AFTER_FAILURES`. TOML: `providers.embedding.cooldown-after-failures`. Clamped 1–20. Default: 3.
    pub embed_tei_cooldown_after_failures: usize,

    /// Cooldown window length once the embedding reservation pool trips
    /// `embed_tei_cooldown_after_failures`, in seconds. Same scope caveat as
    /// `embed_tei_cooldown_after_failures` — the TEI provider's own health
    /// tracker keeps its fixed 30s window.
    /// Env: `AXON_TEI_COOLDOWN_SECS`. TOML: `providers.embedding.cooldown-secs`. Clamped 1–3600. Default: 30.
    pub embed_tei_cooldown_secs: u64,

    /// Embedding reservation slots reserved exclusively for interactive
    /// (`ask`/`query`) embeddings; background/maintenance-priority embed jobs
    /// cannot consume this headroom out of the pool sized by
    /// `embed_tei_max_concurrent`. Feeds `ProviderReservationConfig.interactive_reserve`
    /// for the target local-source embedding reservation pool in
    /// `axon-services::context::target_runtime`. Was previously a hardcoded
    /// constant (`RESERVATION_INTERACTIVE_RESERVE = 1`) disconnected from
    /// config entirely.
    /// Env: `AXON_TEI_INTERACTIVE_RESERVED_REQUESTS`. TOML: `providers.embedding.interactive-reserved-requests`. Clamped 0–64. Default: 1.
    pub embed_tei_interactive_reserved_requests: usize,

    /// Documented soft ceiling on concurrent background-priority embed
    /// reservations. Parsed, defaulted, and exposed here, but **not yet
    /// independently enforced**: `ProviderReservationManager` (`axon-observe`)
    /// only implements a two-tier interactive/non-interactive gate today
    /// (`capacity` total pool minus `interactive_reserve`), so background and
    /// maintenance priorities currently share one combined non-interactive
    /// ceiling rather than each having its own. Building true per-priority-tier
    /// caps means extending `ProviderReservationConfig` (used by ~10 provider
    /// constructors across axon-embedding/axon-llm/axon-vectors/axon-adapters),
    /// which is out of scope for this config-wiring fix — tracked as a
    /// follow-up under the #298 pipeline-unification effort.
    /// Env: `AXON_TEI_BACKGROUND_MAX_CONCURRENT_REQUESTS`. TOML: `providers.embedding.background-max-concurrent-requests`. Clamped 1–64. Default: 3.
    pub embed_tei_background_max_concurrent_requests: usize,

    /// Documented soft ceiling on concurrent maintenance-priority embed
    /// reservations (migrate/reindex-class jobs). Same enforcement caveat as
    /// `embed_tei_background_max_concurrent_requests`.
    /// Env: `AXON_TEI_MAINTENANCE_MAX_CONCURRENT_REQUESTS`. TOML: `providers.embedding.maintenance-max-concurrent-requests`. Clamped 1–64. Default: 1.
    pub embed_tei_maintenance_max_concurrent_requests: usize,

    /// Whether the TEI embedding provider applies the query/document
    /// instruction prefix (when the provider capability advertises
    /// instruction support). `false` forces `InstructionSupport::None` at
    /// provider construction regardless of the model's real capability.
    /// Env: `AXON_TEI_QUERY_INSTRUCTION_ENABLED`. TOML: `providers.embedding.query-instruction-enabled`. Default: true.
    pub embed_tei_query_instruction_enabled: bool,

    /// Persist dense embedding vectors in the unified SQLite store for reuse
    /// across source generations and short-lived processes. Disabled by
    /// default because persistence adds cold-ingestion latency. Cache entries
    /// have a fixed seven-day creation TTL, after which they cannot be reused.
    /// Source deletion cannot immediately purge a shared content-addressed row;
    /// periodic fixed-budget maintenance physically reclaims expired rows and
    /// may require multiple passes after a large backlog.
    /// Env: `AXON_EMBED_CACHE_ENABLED`. TOML: `providers.embedding.cache-enabled`. Default: false.
    pub embed_cache_enabled: bool,

    /// Target maximum number of dense vectors retained by amortized LRU
    /// maintenance. The cache can temporarily exceed this target between
    /// maintenance sweeps.
    /// Env: `AXON_EMBED_CACHE_MAX_ENTRIES`. TOML: `providers.embedding.cache-max-entries`. Clamped 1–10_000_000. Default: 100_000.
    pub embed_cache_max_entries: usize,

    /// Max chunk inputs pooled into one native TEI embed wave.
    /// Env: `AXON_EMBED_POOL_MAX_INPUTS`. TOML: `providers.embedding.pool-max-inputs`. Clamped 64–65536. Default: 512.
    pub embed_pool_max_inputs: usize,

    /// Use the bounded preparation/embedding scheduler.
    /// Env: `AXON_EMBED_SCHEDULER_ENABLED`. TOML: `providers.embedding.scheduler-enabled`. Default: true.
    pub embed_scheduler_enabled: bool,

    /// Overlap the next embedding request with the current vector upsert.
    /// Env: `AXON_VECTOR_UPSERT_EMBED_OVERLAP`. TOML: `providers.embedding.vector-upsert-overlap-enabled`. Default: true.
    pub vector_upsert_embed_overlap: bool,

    /// Concurrent source-document preparation tasks before embedding.
    /// Env: `AXON_EMBED_PREP_CONCURRENCY`. TOML: `providers.embedding.prep-concurrency`. Clamped 1–64.
    pub embed_prep_concurrency: usize,

    /// Maximum characters in a Markdown prose chunk. Intact fenced code
    /// blocks may exceed this limit rather than being cut.
    /// Env: `AXON_MARKDOWN_CHUNK_MAX_CHARS`. TOML: `pipeline.chunking.markdown-max-chars`. Clamped 256–16384. Default: 2000.
    pub chunking_markdown_max_chars: usize,

    /// Minimum compatible Markdown chunk size considered during packing.
    /// Env: `AXON_MARKDOWN_CHUNK_MIN_CHARS`. TOML: `pipeline.chunking.markdown-min-chars`. Clamped 1–the resolved maximum. Default: 500.
    pub chunking_markdown_min_chars: usize,

    /// Character overlap between consecutive windows from the same Markdown
    /// prose span. It never crosses headings, frontmatter, or fenced code.
    /// Env: `AXON_CHUNK_OVERLAP_CHARS`. TOML: `pipeline.chunking.overlap-chars`. Clamped below the resolved maximum. Default: 200.
    pub chunking_overlap_chars: usize,

    /// Optional per-document chunk cap after exact dedupe; `None` disables the cap.
    /// Env: `AXON_EMBED_MAX_CHUNKS_PER_DOC`. TOML: `providers.embedding.max-chunks-per-doc`.
    pub embed_max_chunks_per_doc: Option<usize>,

    /// Optional per-source-document chunk cap after exact dedupe; `None` disables the cap.
    /// Env: `AXON_EMBED_MAX_SOURCE_CHUNKS_PER_DOC`. TOML: `providers.embedding.max-source-chunks-per-doc`.
    pub embed_max_source_chunks_per_doc: Option<usize>,

    /// Drop exact duplicate chunks within one logical document before embedding.
    /// Env: `AXON_EMBED_DEDUPE_EXACT_CHUNKS`. TOML: `providers.embedding.dedupe-exact-chunks`. Default: true.
    pub embed_dedupe_exact_chunks: bool,

    /// Model sent to OpenAI-compatible `/v1/embeddings` endpoints.
    /// Env: `AXON_OPENAI_EMBEDDING_MODEL` or `VLLM_SERVED_MODEL_NAME`. TOML: `providers.embedding.openai-model`.
    pub openai_embed_model: String,

    /// Client batch size for OpenAI-compatible `/v1/embeddings`.
    /// Env: `AXON_OPENAI_EMBED_MAX_CLIENT_BATCH_SIZE`. TOML: `providers.embedding.openai-max-client-batch-size`. Clamped 1–256. Default: 32.
    pub openai_embed_max_client_batch_size: usize,

    /// Max concurrent client requests to OpenAI-compatible `/v1/embeddings`.
    /// Env: `AXON_OPENAI_EMBED_MAX_CONCURRENT`. TOML: `providers.embedding.openai-max-concurrent`. Clamped 1–64. Default: 32.
    pub openai_embed_max_concurrent: usize,

    /// Weighted cap on input chunks in flight to OpenAI-compatible `/v1/embeddings`.
    /// Env: `AXON_OPENAI_EMBED_MAX_IN_FLIGHT_INPUTS`. TOML: `providers.embedding.openai-max-in-flight-inputs`. Clamped 1–4096. Default: 512.
    pub openai_embed_max_in_flight_inputs: usize,

    /// Max chunk inputs pooled into one OpenAI-compatible embed wave.
    /// Env: `AXON_OPENAI_EMBED_POOL_MAX_INPUTS`. TOML: `providers.embedding.openai-pool-max-inputs`. Clamped 64–65536. Default: 1024.
    pub openai_embed_pool_max_inputs: usize,

    /// Maximum number of unified-job-store jobs the single unified worker
    /// runs concurrently (bounded via a semaphore around its claim loop).
    /// Deliberately not auto-derived from provider-specific lane knobs: this
    /// is the durable worker's global source/extract/watch/prune envelope.
    /// Env: `AXON_UNIFIED_WORKER_CONCURRENCY`. TOML: `workers.unified-worker-concurrency`. Clamped 1–64. Default: 8.
    pub unified_worker_concurrency: usize,

    /// Maximum number of concurrent `Source` jobs (every source job kind —
    /// web, git, feeds, registries, Reddit, YouTube, CLI/MCP tools, local
    /// paths — plus `map`) the unified worker runs at once, independent of
    /// `unified_worker_concurrency`. This is a general per-source-kind rail,
    /// not a web/Chrome-specific one: several source kinds share constrained
    /// external resources (a single Chrome instance for web/render-backed
    /// acquisition, upstream API rate limits for git/registry/Reddit
    /// adapters, and so on), so gating all `Source` jobs together avoids one
    /// noisy kind starving the rest of the general
    /// `unified_worker_concurrency` semaphore (default 8).
    /// Env: `AXON_SOURCE_JOB_CONCURRENCY_LIMIT`. TOML: `pipeline.max-active-source-jobs`
    /// (the config-contract's canonical spelling for this knob). Clamped 1–64. Default: 4.
    pub source_job_concurrency_limit: usize,

    /// Per-document embed timeout in seconds (used by the embed pipeline).
    /// Env: `AXON_EMBED_DOC_TIMEOUT_SECS`. TOML: `workers.embed-doc-timeout-secs`. Clamped 30–3600. Default: 300.
    pub embed_doc_timeout_secs: u64,

    /// Queue summary interval in seconds.
    /// Env: `AXON_QUEUE_SUMMARY_SECS`. TOML: `workers.queue-summary-secs`. 0 disables logging. Clamped 0–3600. Default: 30.
    pub queue_summary_secs: u64,

    /// Buffered Qdrant points before flush.
    /// Env: `AXON_QDRANT_POINT_BUFFER`. TOML: `workers.qdrant-point-buffer`. Clamped 128–16384. Default: 1024.
    pub qdrant_point_buffer: usize,

    /// HNSW `ef` for named-mode (dense+sparse) collection searches.
    /// Env: `AXON_HNSW_EF_SEARCH`. TOML: `search.hnsw-ef`. Clamped 32–512. Default: 128.
    pub hnsw_ef_search: usize,

    /// Internal compatibility value used by an older diagnostic projection.
    /// It always mirrors `hnsw_ef_search`; no separate public setting exists.
    pub hnsw_ef_search_legacy: usize,

    /// Run the command on a recurring schedule every N seconds (`None` = one-shot). Flag: `--cron-every-seconds`.
    pub cron_every_seconds: Option<u64>,

    /// Stop cron after this many runs (`None` = run forever). Flag: `--cron-max-runs`.
    pub cron_max_runs: Option<usize>,

    /// Seconds a running job may remain idle before the watchdog marks it stale.
    /// Env: `AXON_JOB_STALE_TIMEOUT_SECS`. TOML: `workers.watchdog-stale-timeout-secs`.
    pub watchdog_stale_timeout_secs: i64,

    /// Seconds a stale-marked job must remain unchanged before the watchdog reclaims it.
    /// Env: `AXON_JOB_STALE_CONFIRM_SECS`. TOML: `workers.watchdog-confirm-secs`.
    pub watchdog_confirm_secs: i64,

    /// Seconds between periodic watchdog sweeps in the long-running worker process.
    /// Smaller values reclaim stale jobs sooner at the cost of extra SQL writes.
    /// Env: `AXON_WATCHDOG_SWEEP_SECS`. Default: 15.
    pub watchdog_sweep_secs: i64,

    /// Seconds a job kind's queue may hold pending jobs while zero jobs of that
    /// kind are running before the liveness watchdog declares worker starvation,
    /// logs loudly at ERROR, and kicks/respawns the lane(s). This is the safety
    /// net for a worker lane that has silently stopped claiming. `0` disables the
    /// starvation detector.
    /// Env: `AXON_WORKER_STARVATION_SECS`. TOML: `workers.worker-starvation-secs`. Default: 120.
    pub worker_starvation_secs: i64,

    /// Maximum number of times a job may be reclaimed out of a stale `running`
    /// state before the watchdog dead-letters it (marks it `failed`) instead of
    /// re-queueing it as `pending`. Bounds a job that crashes or hangs on every
    /// attempt so it cannot cycle running→pending→running forever, accumulating
    /// `attempt_count` without bound. `0` disables the cap (unlimited reclaims).
    /// Env: `AXON_MAX_JOB_ATTEMPTS`. TOML: `workers.max-job-attempts`. Default: 5.
    pub max_job_attempts: u32,

    /// Retention window in days for terminal unified job rows (and their
    /// cascaded children) before the retention sweep deletes them.
    /// Env: `AXON_JOBS_RETENTION_TERMINAL_DAYS`. TOML: `jobs.terminal-retention-days`. Default: 30.
    pub jobs_retention_terminal_days: i64,

    /// Retention window in days for detailed `job_events` rows belonging to
    /// non-failed jobs. Env: `AXON_JOBS_RETENTION_EVENT_DAYS`. TOML: `jobs.event-retention-days`. Default: 14.
    pub jobs_retention_event_days: i64,

    /// Retention window in days for `job_events` rows belonging to failed
    /// jobs — kept longer than ordinary events for postmortem evidence.
    /// Env: `AXON_JOBS_RETENTION_FAILED_EVENT_DAYS`. TOML: `jobs.failed-event-retention-days`. Default: 60.
    pub jobs_retention_failed_event_days: i64,

    /// Retention window in days for `provider_reservations` rows (provider
    /// capacity/health history). Env: `AXON_JOBS_RETENTION_PROVIDER_HEALTH_DAYS`.
    /// TOML: `jobs.provider-health-retention-days`. Default: 7.
    pub jobs_retention_provider_health_days: i64,

    /// Retention window in days for `job_artifacts` rows.
    /// Env: `AXON_JOBS_RETENTION_ARTIFACT_DAYS`. TOML: `jobs.artifact-retention-days`. Default: 30.
    pub jobs_retention_artifact_days: i64,

    /// Seconds between periodic differentiated retention sweeps in the
    /// long-running worker process. Env: `AXON_JOBS_RETENTION_SWEEP_SECS`.
    /// TOML: `jobs.retention-sweep-secs`. Default: 3600.
    pub jobs_retention_sweep_secs: i64,

    /// SLO in seconds an `interactive`-priority job may sit `queued` with no
    /// `interactive`-priority job running before the starvation watchdog
    /// emits a warning event. Distinct from `worker_starvation_secs`, which
    /// detects an entire wedged/dead lane rather than priority-class
    /// fairness. `0` disables the check.
    /// Env: `AXON_JOBS_INTERACTIVE_STARVATION_SLO_SECS`.
    /// TOML: `jobs.interactive-starvation-slo-secs`. Default: 30.
    pub jobs_interactive_starvation_slo_secs: i64,

    /// After a detached CLI enqueue (`axon <source>` without `--wait`),
    /// automatically start a background `axon jobs worker` process when no
    /// worker process currently holds the drain lock, so detached jobs are
    /// picked up without a manually started `axon serve`.
    /// Env: `AXON_JOBS_AUTO_WORKER`. TOML: `jobs.auto-worker`. Default: true.
    pub jobs_auto_worker: bool,

    /// Seconds an auto-started `axon jobs worker` process lingers after the
    /// queue goes quiet before exiting. `0` keeps the worker running until
    /// stopped. Env: `AXON_JOBS_WORKER_IDLE_EXIT_SECS`.
    /// TOML: `jobs.worker-idle-exit-secs`. Default: 300 (wide enough that
    /// repeated single enqueues reuse one worker instead of respawning).
    pub jobs_worker_idle_exit_secs: u64,

    /// Emit machine-readable JSON output on stdout instead of human-readable text. Flag: `--json`.
    pub json_output: bool,

    /// Status mode: include only watchdog-reclaimed jobs. Flag: `--reclaimed`.
    pub reclaimed_status_only: bool,

    /// List/status mode: include only active jobs (`running`/`pending`). Flag: `--active`.
    pub active_status_only: bool,

    /// List/status mode: include active + completed jobs, hide failed/canceled. Flag: `--recent`.
    pub recent_status_only: bool,

    /// Deduplicate trailing-slash URL variants (e.g. `/about` and `/about/` treated as one).
    /// Spider: `with_normalize(bool)`. Default false. Flag: `--normalize`.
    pub normalize: bool,

    // P2 — engine tuning (previously hardcoded in engine.rs)
    /// Seconds to wait for Chrome network idle before capturing the page.
    /// Used by `WaitForIdleNetwork`. Default: 15. TOML: `chrome.network-idle-timeout-secs`.
    pub chrome_network_idle_timeout_secs: u64,

    /// Thin-page ratio threshold for auto-switch from HTTP to Chrome mode (0.0–1.0).
    /// If more than this fraction of crawled pages are thin, retry with Chrome.
    /// Default: 0.60. TOML: `scrape.auto-switch-thin-ratio`.
    pub auto_switch_thin_ratio: f64,

    /// Minimum pages crawled before auto-switch eligibility is evaluated.
    /// Prevents triggering Chrome on tiny crawls. Default: 10. TOML: `scrape.auto-switch-min-pages`.
    pub auto_switch_min_pages: usize,

    /// Minimum broadcast channel buffer for crawl page receiver (entries, not bytes).
    /// Set by performance profile. Default (high-stable): 512.
    pub crawl_broadcast_buffer_min: usize,

    /// Maximum broadcast channel buffer for crawl page receiver (entries, not bytes).
    /// Set by performance profile. Default (high-stable): 2_048.
    pub crawl_broadcast_buffer_max: usize,

    /// Allow explicitly uncapped broad-domain crawls without a path budget or URL whitelist.
    /// Default: false. Env: `AXON_ALLOW_UNBOUNDED_BROAD_CRAWL`.
    pub allow_unbounded_broad_crawl: bool,

    // P3 — missing spider builder methods
    /// URL allow-list: only crawl URLs matching at least one of these regex patterns.
    /// Complement to the URL blacklist. Default: [] (no restriction). TOML: `scrape.url-whitelist`.
    pub url_whitelist: Vec<String>,

    /// Block asset downloads (images, CSS, fonts, JS) during crawl to reduce bandwidth.
    /// Spider: `with_block_assets(true)`. Default: false. Flag: `--block-assets`.
    pub block_assets: bool,

    /// Maximum response size per page in bytes; pages exceeding this are skipped.
    /// Spider: `with_max_page_bytes(u64)`. Default: 4 MiB; explicit 0 disables the cap. TOML: `scrape.max-page-bytes`.
    pub max_page_bytes: Option<u64>,

    /// Abort an in-process crawl when RSS reaches this percent of the host/cgroup memory limit.
    /// Default: 85.0; explicit 0 disables the guard. Env: `AXON_CRAWL_MEMORY_ABORT_PERCENT`.
    pub crawl_memory_abort_percent: Option<f64>,

    /// Use strict redirect policy — only follow same-origin redirects.
    /// Spider: `with_redirect_policy(RedirectPolicy::Strict)`. Default: false. TOML: `scrape.redirect-policy-strict`.
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

    /// Lower bound for `embedded_at` payload filter on query/ask. Accepts `7d`, `30d`, `1w`,
    /// `YYYY-MM-DD`, or RFC3339. Default: None (no lower bound). Flag: `--since`.
    pub since: Option<String>,

    /// Upper bound for `embedded_at` payload filter on query/ask. Same formats as `--since`.
    /// Default: None (no upper bound). Flag: `--before`.
    pub before: Option<String>,

    /// Transient per-job origin marker stamped onto every Qdrant chunk payload as
    /// `seed_url`. Set to the crawl start URL (crawl path) or the ingest target
    /// (ingest path) by the job runners before embedding, so each chunk records
    /// the origin that started its acquisition — distinct from the chunk's own
    /// page `url`. `None` for direct foreground source projections, where the
    /// embed pipeline falls back to the doc's own URL. Consumed by source refresh
    /// logic to re-enqueue origins. Not parsed from CLI/env — runtime-only state.
    pub seed_url: Option<String>,

    /// Optional exact domain/host filter for `axon sources --domain`.
    pub sources_domain: Option<String>,

    /// Export all matching domain URLs for `axon sources --domain --all`.
    pub sources_domain_all: bool,

    /// Optional exact domain/host check for `axon domains --domain`.
    pub domains_domain: Option<String>,

    // P5 — opt-in crawl safety/compat flags
    /// Bypass Content Security Policy in Chrome — helps on pages that block inline JS via CSP.
    /// Spider: `with_csp_bypass(true)`. Chrome only. Default: false. TOML: `chrome.bypass-csp`.
    pub bypass_csp: bool,

    /// Accept invalid/self-signed TLS certificates. Useful for internal or staging sites.
    /// Spider: `with_danger_accept_invalid_certs(true)`. Default: false. TOML: `chrome.accept-invalid-certs`.
    pub accept_invalid_certs: bool,

    /// Capture the full scrollable page (true) or just the viewport (false).
    /// Default: true. Flag: `--screenshot-full-page`.
    pub screenshot_full_page: bool,

    /// Viewport width in pixels for screenshot capture. Default: 1920. Flag: `--viewport`.
    pub viewport_width: u32,

    /// Viewport height in pixels for screenshot capture. Default: 1080. Flag: `--viewport`.
    pub viewport_height: u32,

    /// MCP transport mode. Defaults by entrypoint: `axon mcp` uses stdio,
    /// `axon serve mcp` uses HTTP. Flag: `--transport`.
    pub mcp_transport: McpTransport,

    /// Host interface for MCP HTTP transport. Env: `AXON_HTTP_HOST`. Default: `127.0.0.1`.
    pub mcp_http_host: String,

    /// Port for MCP HTTP transport. Env: `AXON_HTTP_PORT`. Default: `8001`.
    pub mcp_http_port: u16,

    /// Custom HTTP request headers in `"Key: Value"` format (repeatable). Flag: `--header`.
    pub custom_headers: Vec<String>,

    /// Suppress spinners and progress output while keeping JSON/data output intact. Flag: `--quiet`.
    pub quiet: bool,

    /// Override log level before tracing init. Env: `AXON_LOG_LEVEL`.
    pub log_level: Option<String>,

    /// Timeout in seconds for `--wait true` job polling.
    /// Env: `AXON_JOB_WAIT_TIMEOUT_SECS`. TOML: `workers.job-wait-timeout-secs`.
    /// Clamped 30–3600. Default: 300.
    pub job_wait_timeout_secs: u64,

    /// Run `axon doctor diagnose` extended remediation mode.
    pub doctor_diagnose: bool,

    // ── Webclaw port (axon_rust-zehr) ──────────────────────────────────────
    /// Enable per-site vertical extractors (GitHub, PyPI, Reddit, etc.).
    /// Env: `AXON_ENABLE_VERTICALS`. TOML: `verticals.enabled`. Default: `true`.
    pub enable_verticals: bool,

    /// Vertical extractor names to SKIP in auto-dispatch (still available via
    /// `--vertical <name>` or MCP `vertical_scrape`). Empty means auto-dispatch
    /// every registered extractor.
    /// Env: `AXON_AUTO_DISPATCH_SKIP` (comma-separated). TOML: `verticals.auto-dispatch-skip`.
    pub auto_dispatch_skip: Vec<String>,

    /// Per-vertical cache TTL in seconds. Keys are extractor names.
    /// Built-in defaults: github=86400, reddit=3600, hn=21600.
    /// Env override per-vertical: `AXON_VERTICAL_CACHE_TTL_<UPPER>=secs`.
    /// TOML: `[verticals.cache-ttl-secs]` table.
    pub vertical_cache_ttl_secs: std::collections::HashMap<String, u64>,

    /// Maximum bytes stored in the Qdrant `structured_blob` payload field per
    /// chunk. Larger structured-data payloads (e.g. multi-MB `__NEXT_DATA__`)
    /// are dropped rather than serialized.
    /// Env: `AXON_STRUCTURED_DATA_MAX_BYTES`. TOML: `payload.structured-data-max-bytes`.
    /// Default: 65536 (64 KiB).
    pub structured_data_max_bytes: usize,

    /// DOM retry ladder Strategy 1 threshold: re-run extraction with
    /// `only_main_content=false` when the scored extractor produces fewer than
    /// this many words.
    /// Env: `AXON_LADDER_STRATEGY1_THRESHOLD`. TOML: `scrape.ladder-strategy1-threshold`.
    /// Default: 30.
    pub ladder_word_threshold_strategy1: usize,

    /// DOM retry ladder Strategy 2 threshold: re-run extraction with
    /// `include_selectors=["body"]` when the scored extractor produces fewer
    /// than this many words AND no user `include_selectors` were supplied.
    /// Env: `AXON_LADDER_STRATEGY2_THRESHOLD`. TOML: `scrape.ladder-strategy2-threshold`.
    /// Default: 200.
    pub ladder_word_threshold_strategy2: usize,

    /// DOM retry ladder body-fallback multiplier: body-fallback wins only if
    /// it produces more than `multiplier * scored_word_count` words.
    /// Env: `AXON_LADDER_BODY_MULTIPLIER`. TOML: `scrape.ladder-body-multiplier`.
    /// Default: 2.0.
    pub ladder_body_multiplier: f64,

    /// Enable Akamai/CF cookie warmup retry on antibot challenge detection.
    /// Env: `AXON_CHALLENGE_WARMUP`. TOML: `antibot.cookie-warmup`. Default: `true`.
    pub antibot_cookie_warmup: bool,

    /// Maximum bytes scanned for antibot challenge patterns. Pages larger
    /// than this skip the substring scan (the byte-length gate is already
    /// applied per-vendor in the detection logic).
    /// Env: `AXON_ANTIBOT_MAX_BODY_SCAN_BYTES`. TOML: `antibot.max-body-scan-bytes`.
    /// Default: 150000.
    pub antibot_max_body_scan_bytes: usize,
}
#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
