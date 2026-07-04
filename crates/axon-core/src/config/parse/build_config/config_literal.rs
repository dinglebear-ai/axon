//! Builds the populated `Config` literal from `GlobalArgs`, the TOML config,
//! and the per-command accumulators returned by `command_dispatch::dispatch`.
//!
//! Split out of the monolithic `into_config()` (bead axon_rust-2j9.6).
//! Field semantics, env-var keys, clamps, and defaults are byte-for-byte the
//! same as the previous flat literal.

use super::super::super::cli::GlobalArgs;
use super::super::super::types::{
    CommandKind, Config, DEFAULT_CRAWL_BROADCAST_BUFFER_MAX, DEFAULT_CRAWL_BROADCAST_BUFFER_MIN,
    DEFAULT_CRAWL_MEMORY_ABORT_PERCENT, DEFAULT_MAX_PAGE_BYTES,
};
use super::super::docker::normalize_local_service_url;
use super::super::helpers::{
    env_bool, env_port, parse_csv_env, parse_origin_allowlist, parse_path_budgets,
    resolve_mcp_transport, validate_custom_headers,
};
use super::super::toml_config::TomlConfig;
use super::super::tuning;
use super::command_dispatch::DispatchOutput;
use crate::logging::log_warn;
use std::env;

/// Inputs required by the assemblers below. Shared so each helper takes a
/// single tuple rather than five separate parameters.
pub(super) struct LiteralInputs<'a> {
    pub global: &'a GlobalArgs,
    pub toml: &'a TomlConfig,
    pub dispatched: &'a DispatchOutput,
    pub collection: String,
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
    warn_services_section_if_present(inputs.toml);

    // Resolve fallible inputs first so `?` short-circuits before we mutate `cfg`.
    let tei_url = resolve_tei_url(inputs.global, inputs.toml)?;
    let qdrant_url = resolve_qdrant_url(inputs.global, inputs.toml)?;
    let custom_headers = validate_custom_headers(inputs.global.custom_headers.clone())?;
    let mcp_http_port = env_port("AXON_HTTP_PORT", 8001)?;

    let mut cfg = Config::default();
    populate_identity_and_crawl(&mut cfg, &inputs);
    populate_chrome_and_filtering(&mut cfg, &inputs);
    populate_perf_and_credentials(&mut cfg, &inputs)?;
    populate_services_and_ask_basics(&mut cfg, &inputs, tei_url, qdrant_url)?;
    populate_ask_tuning(&mut cfg, inputs.toml);
    populate_misc(&mut cfg, &inputs, custom_headers, mcp_http_port)?;
    Ok(cfg)
}

fn populate_identity_and_crawl(cfg: &mut Config, inputs: &LiteralInputs<'_>) {
    let g = inputs.global;
    let scrape = &inputs.toml.scrape;
    cfg.command = inputs.dispatched.command;
    cfg.start_url = inputs
        .dispatched
        .positional
        .first()
        .cloned()
        .unwrap_or_default();
    cfg.positional = inputs.dispatched.positional.clone();
    cfg.urls_csv = g.urls.clone();
    cfg.url_glob = g.url_glob.clone();
    cfg.query = g.query.clone();
    cfg.search_limit = g.limit;
    cfg.freshness = inputs.dispatched.freshness.clone();
    cfg.fresh_action = inputs.dispatched.fresh_action.clone();
    cfg.retrieve_max_points = inputs.dispatched.retrieve_max_points;
    cfg.train_best_rank = inputs.dispatched.train_best_rank;
    cfg.train_notes = inputs.dispatched.train_notes.clone();
    cfg.purge_prefix = inputs.dispatched.purge_prefix;
    cfg.purge_dry_run = inputs.dispatched.purge_dry_run;
    cfg.source_scope = inputs.dispatched.source_scope.clone();
    cfg.reset_stores = inputs.dispatched.reset_stores.clone();
    cfg.reset_dry_run = inputs.dispatched.reset_dry_run;
    cfg.doctor_diagnose = inputs.dispatched.doctor_diagnose;
    // `extract` defaults to the exact single-page path when omitted. The crawl
    // page-cap default + ceiling are NOT resolved here — that policy lives in the
    // services layer (`axon_services::crawl::resolve_crawl_max_pages`) so the CLI,
    // MCP, and HTTP transports all behave identically. `0` means "unspecified" and
    // the services layer fills in the default; this transport stays a thin shim.
    cfg.max_pages = match (inputs.dispatched.command, g.max_pages) {
        (CommandKind::Extract, None) => 1,
        (_, max_pages) => max_pages.unwrap_or(0),
    };
    cfg.max_depth = g.max_depth;
    cfg.include_subdomains = g.include_subdomains;
    cfg.exclude_path_prefix = inputs.exclude_path_prefix.clone();
    cfg.ingest_exclude_paths = g.ingest_exclude_paths.clone();
    cfg.output_dir = g.output_dir.clone();
    cfg.output_path = g.output.clone();
    cfg.render_mode = g.render_mode;
    cfg.respect_robots = scrape.respect_robots.unwrap_or(false);
    cfg.min_markdown_chars = scrape.min_markdown_chars.unwrap_or(200);
    cfg.drop_thin_markdown = scrape.drop_thin_markdown.unwrap_or(true);
    cfg.discover_sitemaps = scrape.discover_sitemaps.unwrap_or(true);
    cfg.sitemap_since_days = scrape.sitemap_since_days.unwrap_or(0);
    cfg.map_fallback = inputs.dispatched.map_fallback;
    cfg.endpoints_include_bundles = inputs.dispatched.endpoints_include_bundles;
    cfg.endpoints_first_party_only = inputs.dispatched.endpoints_first_party_only;
    cfg.endpoints_unique_only = inputs.dispatched.endpoints_unique_only;
    cfg.endpoints_max_scripts = inputs.dispatched.endpoints_max_scripts;
    cfg.endpoints_max_scan_bytes = inputs.dispatched.endpoints_max_scan_bytes;
    cfg.endpoints_verify = inputs.dispatched.endpoints_verify;
    cfg.endpoints_capture_network = inputs.dispatched.endpoints_capture_network;
    cfg.endpoints_probe_rpc = inputs.dispatched.endpoints_probe_rpc;
    cfg.endpoints_probe_rpc_subdomains = inputs.dispatched.endpoints_probe_rpc_subdomains;
    cfg.max_sitemaps = scrape.max_sitemaps.unwrap_or(512);
    cfg.discover_llms_txt = scrape.discover_llms_txt.unwrap_or(true);
    cfg.max_llms_txt_urls = scrape.max_llms_txt_urls.unwrap_or(512);
    cfg.cache = g.cache;
    cfg.cache_http_only = g.cache_http_only;
    cfg.etag_conditional = g.etag_conditional;
    cfg.path_budgets = parse_path_budgets(&g.path_budgets);
    cfg.format = g.format;
}

fn populate_chrome_and_filtering(cfg: &mut Config, inputs: &LiteralInputs<'_>) {
    cfg.chrome_remote_url = env::var("AXON_CHROME_REMOTE_URL")
        .ok()
        .or_else(|| {
            inputs.toml.services.chrome_remote_url.clone().inspect(|_| {
                warn_legacy_service_url("chrome-remote-url", "AXON_CHROME_REMOTE_URL");
            })
        })
        .map(normalize_local_service_url);
    cfg.chrome_proxy = env::var("AXON_CHROME_PROXY").ok();
    cfg.chrome_user_agent = env::var("AXON_CHROME_USER_AGENT")
        .ok()
        .or_else(|| inputs.toml.chrome.user_agent.clone());
    cfg.chrome_bootstrap_timeout_ms = inputs
        .toml
        .chrome
        .bootstrap_timeout_ms
        .unwrap_or(3_000)
        .max(250);
    cfg.chrome_bootstrap_retries = inputs.toml.chrome.bootstrap_retries.unwrap_or(2).min(10);
    cfg.chrome_remote_local_policy = inputs.toml.chrome.remote_local_policy.unwrap_or(false);
}

fn populate_perf_and_credentials(
    cfg: &mut Config,
    inputs: &LiteralInputs<'_>,
) -> Result<(), String> {
    let g = inputs.global;
    cfg.collection = inputs.collection.clone();
    cfg.embed = !g.skip_embed;
    cfg.mcp_embed_allowed_roots = env::var("AXON_MCP_EMBED_ALLOWED_ROOTS")
        .ok()
        .map(|raw| {
            raw.split(',')
                .filter_map(|part| {
                    let trimmed = part.trim();
                    (!trimmed.is_empty()).then(|| std::path::PathBuf::from(trimmed))
                })
                .collect()
        })
        .unwrap_or_default();
    cfg.mcp_embed_max_local_bytes = tuning::mcp_embed_max_local_bytes();
    cfg.mcp_embed_max_local_depth = tuning::mcp_embed_max_local_depth();
    cfg.mcp_embed_max_local_entries = tuning::mcp_embed_max_local_entries();
    cfg.batch_concurrency = g.batch_concurrency.clamp(1, 512);
    cfg.wait = g.wait;
    cfg.sqlite_path = inputs.sqlite_path.clone();
    cfg.yes = g.yes;
    cfg.color_choice = g.color;
    cfg.watch_mode = g.watch;
    cfg.performance_profile = g.performance_profile;
    cfg.crawl_concurrency_limit = inputs.crawl_concurrency_limit;
    cfg.backfill_concurrency_limit = inputs.backfill_concurrency_limit;
    cfg.adaptive_concurrency.enabled = inputs
        .toml
        .workers
        .adaptive_concurrency
        .enabled
        .unwrap_or(false);
    cfg.adaptive_concurrency.min = inputs
        .toml
        .workers
        .adaptive_concurrency
        .min
        .unwrap_or(1)
        .max(1);
    cfg.adaptive_concurrency.max = inputs.toml.workers.adaptive_concurrency.max;
    cfg.sitemap_only = g.sitemap_only;
    cfg.delay_ms = inputs.toml.scrape.delay_ms.unwrap_or(0);
    cfg.request_timeout_ms = inputs.toml.scrape.request_timeout_ms;
    cfg.scrape_batch_timeout_secs = env::var("AXON_SCRAPE_BATCH_TIMEOUT_SECS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .or(inputs.toml.scrape.batch_timeout_secs)
        .filter(|value| *value > 0)
        .map(|value| value.clamp(1, 3600))
        .unwrap_or(120);
    cfg.fetch_retries = inputs.toml.scrape.fetch_retries.unwrap_or(0);
    cfg.retry_backoff_ms = inputs.toml.scrape.retry_backoff_ms.unwrap_or(0);
    let d = inputs.dispatched;
    cfg.sessions_claude = d.sessions_claude;
    cfg.sessions_codex = d.sessions_codex;
    cfg.sessions_gemini = d.sessions_gemini;
    cfg.sessions_project = d.sessions_project.clone();
    cfg.sessions_watch = d.sessions_watch.clone();
    cfg.sessions_action = d.sessions_action;
    cfg.setup_session_watch_action = d.setup_session_watch_action;
    cfg.github_token = env::var("GITHUB_TOKEN").ok();
    cfg.gitlab_token = env::var("GITLAB_TOKEN").ok();
    cfg.gitea_token = env::var("GITEA_TOKEN").ok();
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
    Ok(())
}

fn populate_services_and_ask_basics(
    cfg: &mut Config,
    inputs: &LiteralInputs<'_>,
    tei_url: String,
    qdrant_url: String,
) -> Result<(), String> {
    cfg.tei_url = tei_url;
    cfg.qdrant_url = qdrant_url;
    cfg.llm_backend =
        crate::llm::LlmBackendKind::parse(&non_empty_env("AXON_LLM_BACKEND").unwrap_or_default())?;
    cfg.headless_gemini_model = non_empty_env("AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL")
        .or_else(|| non_empty_env("AXON_HEADLESS_GEMINI_MODEL"))
        .or_else(|| non_empty_toml(inputs.toml.llm.synthesis_gemini_model.as_deref()))
        .unwrap_or_default();
    cfg.headless_gemini_chat_model = non_empty_env("AXON_CHAT_HEADLESS_GEMINI_MODEL")
        .or_else(|| non_empty_toml(inputs.toml.llm.chat_gemini_model.as_deref()))
        .unwrap_or_default();
    cfg.headless_gemini_cmd =
        non_empty_env("AXON_HEADLESS_GEMINI_CMD").unwrap_or_else(|| "gemini".to_string());
    cfg.headless_gemini_home = non_empty_env("AXON_HEADLESS_GEMINI_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| env::var("HOME").ok().map(std::path::PathBuf::from));
    cfg.codex_cmd = non_empty_env("AXON_CODEX_CMD").unwrap_or_else(|| "codex".to_string());
    cfg.codex_home = non_empty_env("AXON_CODEX_HOME").map(std::path::PathBuf::from);
    cfg.codex_model = non_empty_env("AXON_SYNTHESIS_CODEX_MODEL")
        .or_else(|| non_empty_env("AXON_CODEX_MODEL"))
        .unwrap_or_default();
    cfg.codex_completion_concurrency =
        parse_positive_usize_env("AXON_CODEX_COMPLETION_CONCURRENCY", 4)?;
    cfg.codex_load_user_config = env_bool("AXON_CODEX_LOAD_USER_CONFIG", false);
    cfg.llm_completion_concurrency =
        parse_positive_usize_env("AXON_LLM_COMPLETION_CONCURRENCY", 4)?;
    cfg.llm_completion_timeout_secs =
        parse_positive_u64_env("AXON_LLM_COMPLETION_TIMEOUT_SECS", 300)?;
    cfg.openai_base_url = non_empty_env("AXON_OPENAI_BASE_URL").unwrap_or_default();
    cfg.openai_api_key = non_empty_env("AXON_OPENAI_API_KEY").unwrap_or_default();
    cfg.openai_model = non_empty_env("AXON_SYNTHESIS_OPENAI_MODEL")
        .or_else(|| non_empty_env("AXON_OPENAI_MODEL"))
        .or_else(|| non_empty_toml(inputs.toml.llm.synthesis_openai_model.as_deref()))
        .unwrap_or_default();
    cfg.openai_chat_model = non_empty_env("AXON_CHAT_OPENAI_MODEL")
        .or_else(|| non_empty_toml(inputs.toml.llm.chat_openai_model.as_deref()))
        .unwrap_or_default();
    cfg.tavily_api_key = env::var("TAVILY_API_KEY").ok().unwrap_or_default();
    cfg.searxng_url = non_empty_env("AXON_SEARXNG_URL")
        .map(|u| u.trim_end_matches('/').to_string())
        .unwrap_or_default();
    cfg.research_full_content = env_bool("AXON_RESEARCH_FULL_CONTENT", true);
    cfg.mcp_allowed_origins = env::var("AXON_ALLOWED_ORIGINS")
        .ok()
        .map(|raw| parse_origin_allowlist(&raw))
        .unwrap_or_default();
    cfg.ask_diagnostics = inputs.dispatched.ask_diagnostics;
    cfg.ask_explain = inputs.dispatched.ask_explain;
    cfg.ask_stream = inputs.dispatched.ask_stream;
    cfg.ask_follow_up = inputs.dispatched.ask_follow_up;
    cfg.ask_session = inputs.dispatched.ask_session.clone();
    cfg.ask_follow_up_context = None;
    cfg.ask_reset_session = inputs.dispatched.ask_reset_session;
    cfg.ask_new_session = inputs.dispatched.ask_new_session;
    cfg.ask_list_sessions = inputs.dispatched.ask_list_sessions;
    if cfg.ask_list_sessions && matches!(cfg.command, CommandKind::Ask) {
        let has_query_flag = inputs
            .global
            .query
            .as_deref()
            .map(str::trim)
            .is_some_and(|q| !q.is_empty());
        if !inputs.dispatched.positional.is_empty() || has_query_flag {
            return Err(
                "--list-sessions cannot be combined with a query argument; run it on its own"
                    .into(),
            );
        }
    }
    cfg.evaluate_responses_mode = inputs.dispatched.evaluate_responses_mode;
    cfg.evaluate_retrieval_ab = inputs.dispatched.evaluate_retrieval_ab;
    Ok(())
}

fn non_empty_env(var_name: &str) -> Option<String> {
    env::var(var_name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn non_empty_toml(value: Option<&str>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_positive_usize_env(var_name: &str, default: usize) -> Result<usize, String> {
    match env::var(var_name) {
        Ok(raw) if raw.trim().is_empty() => Ok(default),
        Ok(raw) => raw
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| format!("{var_name} must be a positive integer, got {raw:?}")),
        Err(_) => Ok(default),
    }
}

fn parse_positive_u64_env(var_name: &str, default: u64) -> Result<u64, String> {
    match env::var(var_name) {
        Ok(raw) if raw.trim().is_empty() => Ok(default),
        Ok(raw) => raw
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| format!("{var_name} must be a positive integer, got {raw:?}")),
        Err(_) => Ok(default),
    }
}

fn populate_ask_tuning(cfg: &mut Config, toml: &TomlConfig) {
    tuning::apply_env_toml_tuning(cfg, toml);
    cfg.ask_authoritative_domains = env::var("AXON_ASK_AUTHORITATIVE_DOMAINS")
        .ok()
        .map(|raw| parse_csv_env(&raw, |s| s.to_ascii_lowercase()))
        .or_else(|| {
            toml.ask.authoritative_domains.as_ref().map(|domains| {
                domains
                    .iter()
                    .map(|domain| domain.trim().to_ascii_lowercase())
                    .filter(|domain| !domain.is_empty())
                    .collect()
            })
        })
        .unwrap_or_default();
}

fn populate_misc(
    cfg: &mut Config,
    inputs: &LiteralInputs<'_>,
    custom_headers: Vec<String>,
    mcp_http_port: u16,
) -> Result<(), String> {
    let g = inputs.global;
    cfg.hybrid_search_enabled = cfg.hybrid_search_enabled && !g.no_hybrid_search;
    cfg.cron_every_seconds = g.cron_every_seconds.filter(|v| *v > 0);
    cfg.cron_max_runs = g.cron_max_runs.filter(|v| *v > 0);
    cfg.watchdog_stale_timeout_secs = parse_i64_env("AXON_JOB_STALE_TIMEOUT_SECS")
        .or(inputs.toml.workers.watchdog_stale_timeout_secs)
        .unwrap_or(300)
        .max(30);
    cfg.watchdog_confirm_secs = parse_i64_env("AXON_JOB_STALE_CONFIRM_SECS")
        .or(inputs.toml.workers.watchdog_confirm_secs)
        .unwrap_or(60)
        .max(10);
    cfg.watchdog_sweep_secs = parse_i64_env("AXON_WATCHDOG_SWEEP_SECS")
        .or(inputs.toml.workers.watchdog_sweep_secs)
        .unwrap_or(15)
        .clamp(1, 600);
    cfg.worker_starvation_secs = parse_i64_env("AXON_WORKER_STARVATION_SECS")
        .or(inputs.toml.workers.worker_starvation_secs)
        .unwrap_or(120)
        .clamp(0, 3600);
    cfg.crawl_job_timeout_secs = parse_i64_env("AXON_CRAWL_JOB_TIMEOUT_SECS")
        .or(inputs.toml.workers.crawl_job_timeout_secs)
        .unwrap_or(7200)
        .clamp(0, 86_400);
    cfg.max_job_attempts = parse_i64_env("AXON_MAX_JOB_ATTEMPTS")
        .or(inputs.toml.workers.max_job_attempts)
        .map(|value| value.clamp(0, 1_000) as u32)
        .unwrap_or(cfg.max_job_attempts);
    cfg.json_output = g.json;
    cfg.reclaimed_status_only = g.reclaimed;
    cfg.active_status_only = g.active;
    cfg.recent_status_only = g.recent;
    cfg.normalize = g.normalize;
    cfg.chrome_network_idle_timeout_secs =
        inputs.toml.chrome.network_idle_timeout_secs.unwrap_or(15);
    cfg.auto_switch_thin_ratio = inputs.toml.scrape.auto_switch_thin_ratio.unwrap_or(0.60);
    cfg.auto_switch_min_pages = inputs.toml.scrape.auto_switch_min_pages.unwrap_or(10);
    cfg.crawl_broadcast_buffer_min = DEFAULT_CRAWL_BROADCAST_BUFFER_MIN; // overwritten by post_init from profile
    cfg.crawl_broadcast_buffer_max = DEFAULT_CRAWL_BROADCAST_BUFFER_MAX; // overwritten by post_init from profile
    cfg.allow_unbounded_broad_crawl = parse_bool_env_opt("AXON_ALLOW_UNBOUNDED_BROAD_CRAWL")
        .or(inputs.toml.scrape.allow_unbounded_broad_crawl)
        .unwrap_or(false);
    cfg.url_whitelist = inputs.toml.scrape.url_whitelist.clone().unwrap_or_default();
    cfg.block_assets = g.block_assets;
    let max_page_bytes = inputs
        .toml
        .scrape
        .max_page_bytes
        .unwrap_or(DEFAULT_MAX_PAGE_BYTES);
    cfg.max_page_bytes = if max_page_bytes == 0 {
        None
    } else {
        Some(max_page_bytes)
    };
    cfg.crawl_memory_abort_percent = resolve_crawl_memory_abort_percent(inputs);
    cfg.redirect_policy_strict = inputs.toml.scrape.redirect_policy_strict.unwrap_or(false);
    cfg.chrome_wait_for_selector = g.chrome_wait_for_selector.clone();
    cfg.root_selector = g.root_selector.clone();
    cfg.exclude_selector = g.exclude_selector.clone();
    cfg.chrome_screenshot = g.chrome_screenshot;
    cfg.research_depth = g.research_depth;
    cfg.search_time_range = g.search_time_range.clone();
    cfg.since = g.since.clone();
    cfg.before = g.before.clone();
    cfg.sources_by_schema_version = g.sources_by_schema_version;
    cfg.sources_domain = inputs.dispatched.sources_domain.clone();
    cfg.sources_domain_all = inputs.dispatched.sources_domain_all;
    cfg.domains_domain = inputs.dispatched.domains_domain.clone();
    cfg.bypass_csp = inputs.toml.chrome.bypass_csp.unwrap_or(false);
    cfg.accept_invalid_certs = inputs.toml.chrome.accept_invalid_certs.unwrap_or(false);
    cfg.screenshot_full_page = g.screenshot_full_page;
    cfg.viewport_width = inputs.viewport_width;
    cfg.viewport_height = inputs.viewport_height;
    let (mcp_transport, mcp_transport_default) = (
        inputs.dispatched.mcp_transport,
        inputs.dispatched.mcp_transport_default,
    );
    cfg.mcp_transport = resolve_mcp_transport(mcp_transport, mcp_transport_default);
    cfg.mcp_http_host = env::var("AXON_HTTP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    cfg.mcp_http_port = mcp_http_port;
    cfg.custom_headers = custom_headers;
    cfg.warc_output = g.warc.clone();
    cfg.automation_script = g.automation_script.clone();
    cfg.quiet = g.quiet;
    cfg.log_level = env::var("AXON_LOG_LEVEL").ok();
    Ok(())
}

fn parse_i64_env(var_name: &str) -> Option<i64> {
    env::var(var_name)
        .ok()
        .and_then(|raw| raw.trim().parse::<i64>().ok())
}

fn parse_bool_env_opt(var_name: &str) -> Option<bool> {
    env::var(var_name)
        .ok()
        .and_then(|raw| match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => {
                log_warn(&format!(
                    "invalid {var_name}={raw:?}; expected true/false, 1/0, yes/no, or on/off"
                ));
                None
            }
        })
}

fn resolve_crawl_memory_abort_percent(inputs: &LiteralInputs<'_>) -> Option<f64> {
    let percent = match env::var("AXON_CRAWL_MEMORY_ABORT_PERCENT") {
        Ok(raw) => match raw.trim().parse::<f64>() {
            Ok(parsed) => parsed,
            Err(err) => {
                log_warn(&format!(
                    "invalid AXON_CRAWL_MEMORY_ABORT_PERCENT={raw:?}: {err}; using default {DEFAULT_CRAWL_MEMORY_ABORT_PERCENT:.1}%"
                ));
                DEFAULT_CRAWL_MEMORY_ABORT_PERCENT
            }
        },
        Err(_) => inputs
            .toml
            .scrape
            .crawl_memory_abort_percent
            .unwrap_or(DEFAULT_CRAWL_MEMORY_ABORT_PERCENT),
    };
    (percent.is_finite() && percent > 0.0).then_some(percent.clamp(1.0, 100.0))
}

fn resolve_tei_url(global: &GlobalArgs, toml: &TomlConfig) -> Result<String, String> {
    Ok(normalize_local_service_url(
        global
            .tei_url
            .clone()
            .or_else(|| env::var("TEI_URL").ok())
            .or_else(|| {
                toml.services.tei_url.clone().inspect(|_| {
                    warn_legacy_service_url("tei-url", "TEI_URL");
                })
            })
            .ok_or_else(|| {
                "TEI_URL environment variable is required (or pass --tei-url). \
                 Move legacy [services].tei-url to TEI_URL in .env."
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
            .or_else(|| {
                toml.services.qdrant_url.clone().inspect(|_| {
                    warn_legacy_service_url("qdrant-url", "QDRANT_URL");
                })
            })
            .ok_or_else(|| {
                "QDRANT_URL environment variable is required (or pass --qdrant-url). \
                 Move legacy [services].qdrant-url to QDRANT_URL in .env."
                    .to_string()
            })?,
    ))
}

/// Emit a one-time process warning for each `[services]` URL field present in
/// config.toml. Guarded by a `OnceLock` so repeated Config builds (tests, sub-
/// commands) only emit each message once per process.
fn warn_services_section_if_present(toml: &TomlConfig) {
    use std::sync::OnceLock;
    static WARNED: OnceLock<()> = OnceLock::new();
    // Skip if any field is absent — only warn when the stale [services] block exists.
    let any_set = toml.services.qdrant_url.is_some()
        || toml.services.tei_url.is_some()
        || toml.services.chrome_remote_url.is_some();
    if !any_set {
        return;
    }
    WARNED.get_or_init(|| {
        if toml.services.qdrant_url.is_some() {
            log_warn(
                "[services] qdrant-url in config.toml is ignored; set QDRANT_URL in ~/.axon/.env instead",
            );
        }
        if toml.services.tei_url.is_some() {
            log_warn(
                "[services] tei-url in config.toml is ignored; set TEI_URL in ~/.axon/.env instead",
            );
        }
        if toml.services.chrome_remote_url.is_some() {
            log_warn(
                "[services] chrome-remote-url in config.toml is ignored; set AXON_CHROME_REMOTE_URL in ~/.axon/.env instead",
            );
        }
    });
}

fn warn_legacy_service_url(toml_key: &str, env_key: &str) {
    log_warn(&format!(
        "[services].{toml_key} is deprecated and will be ignored in a future release; move it to {env_key} in .env"
    ));
}
