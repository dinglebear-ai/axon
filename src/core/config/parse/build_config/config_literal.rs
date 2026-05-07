//! Builds the populated `Config` literal from `GlobalArgs`, the TOML config,
//! and the per-command accumulators returned by `command_dispatch::dispatch`.
//!
//! Split out of the monolithic `into_config()` (bead axon_rust-2j9.6).
//! Field semantics, env-var keys, clamps, and defaults are byte-for-byte the
//! same as the previous flat literal.

use super::super::super::cli::GlobalArgs;
use super::super::super::types::Config;
use super::super::docker::normalize_local_service_url;
use super::super::helpers::{
    env_bool, env_bool_opt, env_port, parse_csv_env, parse_origin_allowlist,
    resolve_ask_adapter_args, resolve_ask_adapter_cmd, resolve_mcp_transport,
    validate_custom_headers,
};
use super::super::performance;
use super::super::toml_config::TomlConfig;
use super::command_dispatch::DispatchOutput;
use std::env;

/// Inputs required by the assemblers below. Shared so each helper takes a
/// single tuple rather than five separate parameters.
pub(super) struct LiteralInputs<'a> {
    pub global: &'a GlobalArgs,
    pub toml: &'a TomlConfig,
    pub dispatched: &'a DispatchOutput,
    pub collection: String,
    pub lite_mode: bool,
    pub sqlite_path: std::path::PathBuf,
    pub crawl_concurrency_limit: Option<usize>,
    pub backfill_concurrency_limit: Option<usize>,
    pub exclude_path_prefix: Vec<String>,
    pub viewport_width: u32,
    pub viewport_height: u32,
}

/// Top-level builder. Mirrors the original literal precisely; field population
/// is delegated to `populate_*` helpers (each <120 lines per monolith policy).
pub(super) fn build(inputs: LiteralInputs<'_>) -> Result<Config, String> {
    // Resolve fallible inputs first so `?` short-circuits before we mutate `cfg`.
    let tei_url = resolve_tei_url(inputs.global, inputs.toml)?;
    let qdrant_url = resolve_qdrant_url(inputs.global, inputs.toml)?;
    let custom_headers = validate_custom_headers(inputs.global.custom_headers.clone())?;
    let mcp_http_port = env_port("AXON_MCP_HTTP_PORT", 8001)?;

    let mut cfg = Config::default();
    populate_identity_and_crawl(&mut cfg, &inputs);
    populate_chrome_and_filtering(&mut cfg, &inputs);
    populate_perf_and_credentials(&mut cfg, &inputs);
    populate_services_and_ask_basics(&mut cfg, &inputs, tei_url, qdrant_url);
    populate_ask_tuning(&mut cfg, inputs.toml);
    populate_search_and_workers(&mut cfg, inputs.toml);
    populate_misc(&mut cfg, &inputs, custom_headers, mcp_http_port);
    Ok(cfg)
}

fn populate_identity_and_crawl(cfg: &mut Config, inputs: &LiteralInputs<'_>) {
    let g = inputs.global;
    cfg.command = inputs.dispatched.command;
    cfg.start_url = g.start_url.clone();
    cfg.positional = inputs.dispatched.positional.clone();
    cfg.urls_csv = g.urls.clone();
    cfg.url_glob = g.url_glob.clone();
    cfg.query = g.query.clone();
    cfg.search_limit = g.limit;
    cfg.max_pages = g.max_pages;
    cfg.max_depth = g.max_depth;
    cfg.include_subdomains = g.include_subdomains;
    cfg.exclude_path_prefix = inputs.exclude_path_prefix.clone();
    cfg.output_dir = g.output_dir.clone();
    cfg.output_path = g.output.clone();
    cfg.render_mode = g.render_mode;
    cfg.respect_robots = g.respect_robots;
    cfg.min_markdown_chars = g.min_markdown_chars;
    cfg.drop_thin_markdown = g.drop_thin_markdown;
    cfg.discover_sitemaps = g.discover_sitemaps;
    cfg.sitemap_since_days = g.sitemap_since_days;
    cfg.map_fallback = inputs.dispatched.map_fallback;
    cfg.max_sitemaps = g.max_sitemaps;
    cfg.cache = g.cache;
    cfg.cache_skip_browser = g.cache_skip_browser;
    cfg.format = g.format;
}

fn populate_chrome_and_filtering(cfg: &mut Config, inputs: &LiteralInputs<'_>) {
    let g = inputs.global;
    cfg.chrome_remote_url = g
        .chrome_remote_url
        .clone()
        .or_else(|| env::var("AXON_CHROME_REMOTE_URL").ok())
        .or_else(|| inputs.toml.services.chrome_remote_url.clone())
        .map(normalize_local_service_url);
    cfg.chrome_proxy = g
        .chrome_proxy
        .clone()
        .or_else(|| env::var("AXON_CHROME_PROXY").ok());
    cfg.chrome_user_agent = g
        .chrome_user_agent
        .clone()
        .or_else(|| env::var("AXON_CHROME_USER_AGENT").ok());
    cfg.chrome_headless = g.chrome_headless;
    cfg.chrome_anti_bot = g.chrome_anti_bot;
    cfg.chrome_intercept = g.chrome_intercept;
    cfg.chrome_stealth = g.chrome_stealth;
    cfg.chrome_bootstrap = g.chrome_bootstrap;
    cfg.chrome_bootstrap_timeout_ms = g.chrome_bootstrap_timeout_ms.max(250);
    cfg.chrome_bootstrap_retries = g.chrome_bootstrap_retries.min(10);
}

fn populate_perf_and_credentials(cfg: &mut Config, inputs: &LiteralInputs<'_>) {
    let g = inputs.global;
    cfg.collection = inputs.collection.clone();
    cfg.embed = g.embed;
    cfg.batch_concurrency = g.batch_concurrency.clamp(1, 512);
    cfg.wait = g.wait;
    cfg.lite_mode = inputs.lite_mode;
    cfg.sqlite_path = inputs.sqlite_path.clone();
    cfg.yes = g.yes;
    cfg.performance_profile = g.performance_profile;
    cfg.crawl_concurrency_limit = inputs.crawl_concurrency_limit;
    cfg.backfill_concurrency_limit = inputs.backfill_concurrency_limit;
    cfg.sitemap_only = g.sitemap_only;
    cfg.delay_ms = g.delay_ms;
    cfg.request_timeout_ms = g.request_timeout_ms;
    cfg.fetch_retries = g.fetch_retries.unwrap_or(0);
    cfg.retry_backoff_ms = g.retry_backoff_ms.unwrap_or(0);
    let d = inputs.dispatched;
    cfg.sessions_claude = d.sessions_claude;
    cfg.sessions_codex = d.sessions_codex;
    cfg.sessions_gemini = d.sessions_gemini;
    cfg.sessions_project = d.sessions_project.clone();
    cfg.github_token = env::var("GITHUB_TOKEN").ok();
    cfg.github_include_source = d.github_include_source;
    cfg.github_max_issues = d.github_max_issues;
    cfg.github_max_prs = d.github_max_prs;
    cfg.reddit_client_id = env::var("REDDIT_CLIENT_ID").ok();
    cfg.reddit_client_secret = env::var("REDDIT_CLIENT_SECRET").ok();
    cfg.reddit_sort = d.reddit_sort;
    cfg.reddit_time = d.reddit_time;
    cfg.reddit_max_posts = d.reddit_max_posts;
    cfg.reddit_min_score = d.reddit_min_score;
    cfg.reddit_depth = d.reddit_depth;
    cfg.reddit_scrape_links = d.reddit_scrape_links;
}

fn populate_services_and_ask_basics(
    cfg: &mut Config,
    inputs: &LiteralInputs<'_>,
    tei_url: String,
    qdrant_url: String,
) {
    let g = inputs.global;
    cfg.tei_url = tei_url;
    cfg.qdrant_url = qdrant_url;
    cfg.openai_base_url = g
        .openai_base_url
        .clone()
        .or_else(|| env::var("OPENAI_BASE_URL").ok())
        .unwrap_or_default();
    cfg.openai_api_key = g
        .openai_api_key
        .clone()
        .or_else(|| env::var("OPENAI_API_KEY").ok())
        .unwrap_or_default();
    cfg.openai_model = g
        .openai_model
        .clone()
        .or_else(|| env::var("OPENAI_MODEL").ok())
        .unwrap_or_default();
    cfg.acp_adapter_cmd = resolve_ask_adapter_cmd();
    cfg.acp_adapter_args = resolve_ask_adapter_args();
    cfg.acp_prewarm = env_bool("AXON_ACP_PREWARM", true);
    cfg.acp_ws_url = env::var("AXON_ACP_WS_URL")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v: &String| !v.is_empty());
    cfg.acp_ws_token = env::var("AXON_ACP_WS_TOKEN")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v: &String| !v.is_empty());
    cfg.tavily_api_key = env::var("TAVILY_API_KEY").ok().unwrap_or_default();
    cfg.mcp_allowed_origins = env::var("AXON_MCP_ALLOWED_ORIGINS")
        .ok()
        .map(|raw| parse_origin_allowlist(&raw))
        .unwrap_or_default();
    cfg.ask_diagnostics = inputs.dispatched.ask_diagnostics;
    cfg.ask_graph = g.graph;
    cfg.evaluate_responses_mode = inputs.dispatched.evaluate_responses_mode;
    cfg.evaluate_retrieval_ab = inputs.dispatched.evaluate_retrieval_ab;
}

fn populate_ask_tuning(cfg: &mut Config, toml: &TomlConfig) {
    cfg.ask_max_context_chars = ask_max_context_chars();
    cfg.ask_candidate_limit = ask_candidate_limit(toml);
    cfg.ask_chunk_limit = ask_chunk_limit(toml);
    cfg.ask_full_docs = performance::env_usize_clamped("AXON_ASK_FULL_DOCS", 4, 1, 20);
    cfg.ask_backfill_chunks = performance::env_usize_clamped("AXON_ASK_BACKFILL_CHUNKS", 3, 0, 20);
    cfg.ask_doc_fetch_concurrency =
        performance::env_usize_clamped("AXON_ASK_DOC_FETCH_CONCURRENCY", 4, 1, 16);
    cfg.ask_doc_chunk_limit =
        performance::env_usize_clamped("AXON_ASK_DOC_CHUNK_LIMIT", 192, 8, 2000);
    cfg.ask_min_relevance_score = ask_min_relevance_score(toml);
    cfg.ask_authoritative_domains = env::var("AXON_ASK_AUTHORITATIVE_DOMAINS")
        .ok()
        .map(|raw| parse_csv_env(&raw, |s| s.to_ascii_lowercase()))
        .unwrap_or_default();
    cfg.ask_authoritative_boost =
        performance::env_f64_clamped("AXON_ASK_AUTHORITATIVE_BOOST", 0.0, 0.0, 0.5);
    cfg.ask_min_citations_nontrivial =
        performance::env_usize_clamped("AXON_ASK_MIN_CITATIONS_NONTRIVIAL", 2, 1, 5);
}

fn populate_search_and_workers(cfg: &mut Config, toml: &TomlConfig) {
    cfg.hybrid_search_candidates = hybrid_search_candidates(toml);
    cfg.ask_hybrid_candidates = ask_hybrid_candidates(toml);
    cfg.tei_max_retries = tei_max_retries(toml);
    cfg.tei_request_timeout_ms = tei_request_timeout_ms(toml);
    cfg.tei_max_client_batch_size = tei_max_client_batch_size(toml);
    cfg.ingest_lanes = ingest_lanes(toml);
    cfg.embed_doc_timeout_secs = embed_doc_timeout_secs(toml);
    cfg.max_pending_crawl_jobs = max_pending(toml, "crawl");
    cfg.max_pending_embed_jobs = max_pending(toml, "embed");
    cfg.max_pending_extract_jobs = max_pending(toml, "extract");
    cfg.max_pending_ingest_jobs = max_pending(toml, "ingest");
    cfg.hnsw_ef_search = performance::env_usize_opt("AXON_HNSW_EF_SEARCH", 32, 512)
        .or_else(|| toml.search.hnsw_ef.map(|v| v.clamp(32, 512)))
        .unwrap_or(128);
    cfg.hnsw_ef_search_legacy = performance::env_usize_opt("AXON_HNSW_EF_SEARCH_LEGACY", 16, 256)
        .or_else(|| toml.search.hnsw_ef_legacy.map(|v| v.clamp(16, 256)))
        .unwrap_or(64);
}

fn populate_misc(
    cfg: &mut Config,
    inputs: &LiteralInputs<'_>,
    custom_headers: Vec<String>,
    mcp_http_port: u16,
) {
    let g = inputs.global;
    cfg.hybrid_search_enabled = hybrid_search_enabled(g, inputs.toml);
    cfg.cron_every_seconds = g.cron_every_seconds.filter(|v| *v > 0);
    cfg.cron_max_runs = g.cron_max_runs.filter(|v| *v > 0);
    cfg.watchdog_stale_timeout_secs = g.watchdog_stale_timeout_secs.max(30);
    cfg.watchdog_confirm_secs = g.watchdog_confirm_secs.max(10);
    cfg.json_output = g.json;
    cfg.reclaimed_status_only = g.reclaimed;
    cfg.active_status_only = g.active;
    cfg.recent_status_only = g.recent;
    cfg.normalize = g.normalize;
    cfg.chrome_network_idle_timeout_secs = g.chrome_network_idle_timeout;
    cfg.auto_switch_thin_ratio = g.auto_switch_thin_ratio;
    cfg.auto_switch_min_pages = g.auto_switch_min_pages;
    cfg.crawl_broadcast_buffer_min = 4096; // placeholder — overwritten by post_init from profile
    cfg.crawl_broadcast_buffer_max = 16_384; // placeholder — overwritten by post_init from profile
    cfg.url_whitelist = g.url_whitelist.clone();
    cfg.block_assets = g.block_assets;
    cfg.max_page_bytes = if g.max_page_bytes == 0 {
        None
    } else {
        Some(g.max_page_bytes)
    };
    cfg.redirect_policy_strict = g.redirect_policy_strict;
    cfg.chrome_wait_for_selector = g.chrome_wait_for_selector.clone();
    cfg.root_selector = g.root_selector.clone();
    cfg.exclude_selector = g.exclude_selector.clone();
    cfg.chrome_screenshot = g.chrome_screenshot;
    cfg.research_depth = g.research_depth;
    cfg.search_time_range = g.search_time_range.clone();
    cfg.since = g.since.clone();
    cfg.before = g.before.clone();
    cfg.bypass_csp = g.bypass_csp;
    cfg.accept_invalid_certs = g.accept_invalid_certs;
    cfg.screenshot_full_page = g.screenshot_full_page;
    cfg.viewport_width = inputs.viewport_width;
    cfg.viewport_height = inputs.viewport_height;
    let (mcp_transport, mcp_transport_default) = (
        inputs.dispatched.mcp_transport,
        inputs.dispatched.mcp_transport_default,
    );
    cfg.mcp_transport = resolve_mcp_transport(mcp_transport, mcp_transport_default);
    cfg.mcp_http_host = env::var("AXON_MCP_HTTP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    cfg.mcp_http_port = mcp_http_port;
    cfg.custom_headers = custom_headers;
    cfg.quiet = g.quiet;
    cfg.log_level = g.log_level.clone();
}

fn resolve_tei_url(global: &GlobalArgs, toml: &TomlConfig) -> Result<String, String> {
    Ok(normalize_local_service_url(
        global
            .tei_url
            .clone()
            .or_else(|| env::var("TEI_URL").ok())
            .or_else(|| toml.services.tei_url.clone())
            .ok_or_else(|| {
                "TEI_URL environment variable is required (or pass --tei-url). \
                 Copy .env.example to .env and fill in credentials."
                    .to_string()
            })?,
    ))
}

fn resolve_qdrant_url(global: &GlobalArgs, toml: &TomlConfig) -> Result<String, String> {
    Ok(normalize_local_service_url(
        global
            .qdrant_url
            .clone()
            .or_else(|| env::var("QDRANT_URL").ok())
            .or_else(|| toml.services.qdrant_url.clone())
            .ok_or_else(|| {
                "QDRANT_URL environment variable is required (or pass --qdrant-url). \
                 Copy .env.example to .env and fill in credentials."
                    .to_string()
            })?,
    ))
}

fn ask_max_context_chars() -> usize {
    performance::env_usize_clamped("AXON_ASK_MAX_CONTEXT_CHARS", 120_000, 20_000, 400_000)
}

fn ask_candidate_limit(toml: &TomlConfig) -> usize {
    performance::env_usize_opt("AXON_ASK_CANDIDATE_LIMIT", 8, 300)
        .or_else(|| toml.ask.candidate_limit.map(|v| v.clamp(8, 300)))
        .unwrap_or(150)
}

fn ask_chunk_limit(toml: &TomlConfig) -> usize {
    performance::env_usize_opt("AXON_ASK_CHUNK_LIMIT", 3, 40)
        .or_else(|| toml.ask.chunk_limit.map(|v| v.clamp(3, 40)))
        .unwrap_or(10)
}

fn ask_min_relevance_score(toml: &TomlConfig) -> f64 {
    performance::env_f64_opt("AXON_ASK_MIN_RELEVANCE_SCORE", -1.0, 2.0)
        .or_else(|| toml.ask.min_relevance_score.map(|v| v.clamp(-1.0, 2.0)))
        .unwrap_or(0.45)
}

fn hybrid_search_enabled(global: &GlobalArgs, toml: &TomlConfig) -> bool {
    env_bool_opt("AXON_HYBRID_SEARCH")
        .or(toml.search.hybrid_enabled)
        .unwrap_or(true)
        && !global.no_hybrid_search
}

fn hybrid_search_candidates(toml: &TomlConfig) -> usize {
    performance::env_usize_opt("AXON_HYBRID_CANDIDATES", 10, 500)
        .or_else(|| toml.search.hybrid_candidates.map(|v| v.clamp(10, 500)))
        .unwrap_or(100)
}

fn ask_hybrid_candidates(toml: &TomlConfig) -> usize {
    performance::env_usize_opt("AXON_ASK_HYBRID_CANDIDATES", 10, 500)
        .or_else(|| toml.search.ask_hybrid_candidates.map(|v| v.clamp(10, 500)))
        .unwrap_or(150)
}

fn tei_max_retries(toml: &TomlConfig) -> usize {
    performance::env_usize_opt("TEI_MAX_RETRIES", 0, 20)
        .or_else(|| toml.tei.max_retries.map(|v| v.clamp(0, 20)))
        .unwrap_or(5)
}

fn tei_request_timeout_ms(toml: &TomlConfig) -> u64 {
    performance::env_u64_opt("TEI_REQUEST_TIMEOUT_MS", 1000, 300_000)
        .or_else(|| toml.tei.request_timeout_ms.map(|v| v.clamp(1000, 300_000)))
        .unwrap_or(30_000)
}

fn tei_max_client_batch_size(toml: &TomlConfig) -> usize {
    performance::env_usize_opt("TEI_MAX_CLIENT_BATCH_SIZE", 1, 128)
        .or_else(|| toml.tei.max_client_batch_size.map(|v| v.clamp(1, 128)))
        .unwrap_or(64)
}

fn ingest_lanes(toml: &TomlConfig) -> usize {
    performance::env_usize_opt("AXON_INGEST_LANES", 1, 64)
        .or_else(|| toml.workers.ingest_lanes.map(|v| v.clamp(1, 64)))
        .unwrap_or(2)
}

fn embed_doc_timeout_secs(toml: &TomlConfig) -> u64 {
    performance::env_u64_opt("AXON_EMBED_DOC_TIMEOUT_SECS", 30, 3600)
        .or_else(|| {
            toml.workers
                .embed_doc_timeout_secs
                .map(|v| v.clamp(30, 3600))
        })
        .unwrap_or(300)
}

/// Per-queue pending caps. `kind` selects between crawl/embed/extract/ingest.
/// Defaults: crawl=100, others=50. Env vars `AXON_MAX_PENDING_*_JOBS`. Range 0..=10_000.
fn max_pending(toml: &TomlConfig, kind: &str) -> usize {
    let (env_key, toml_value, default): (&str, Option<usize>, usize) = match kind {
        "crawl" => (
            "AXON_MAX_PENDING_CRAWL_JOBS",
            toml.workers.max_pending_crawl_jobs,
            100,
        ),
        "embed" => (
            "AXON_MAX_PENDING_EMBED_JOBS",
            toml.workers.max_pending_embed_jobs,
            50,
        ),
        "extract" => (
            "AXON_MAX_PENDING_EXTRACT_JOBS",
            toml.workers.max_pending_extract_jobs,
            50,
        ),
        "ingest" => (
            "AXON_MAX_PENDING_INGEST_JOBS",
            toml.workers.max_pending_ingest_jobs,
            50,
        ),
        _ => unreachable!("unknown pending-jobs kind: {kind}"),
    };
    performance::env_usize_opt(env_key, 0, 10_000)
        .or(toml_value.map(|v| v.clamp(0, 10_000)))
        .unwrap_or(default)
}
