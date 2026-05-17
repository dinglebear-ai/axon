//! Builds the populated `Config` literal from `GlobalArgs`, the TOML config,
//! and the per-command accumulators returned by `command_dispatch::dispatch`.
//!
//! Split out of the monolithic `into_config()` (bead axon_rust-2j9.6).
//! Field semantics, env-var keys, clamps, and defaults are byte-for-byte the
//! same as the previous flat literal.

use super::super::super::cli::GlobalArgs;
use super::super::super::types::{ClientMode, CommandKind, Config};
use super::super::docker::normalize_local_service_url;
use super::super::helpers::{
    env_port, parse_csv_env, parse_origin_allowlist, resolve_mcp_transport, validate_custom_headers,
};
use super::super::toml_config::TomlConfig;
use super::super::tuning;
use super::command_dispatch::DispatchOutput;
use crate::core::logging::log_warn;
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
    warn_compat_shim_env_vars();

    // Resolve fallible inputs first so `?` short-circuits before we mutate `cfg`.
    let tei_url = resolve_tei_url(inputs.global, inputs.toml)?;
    let qdrant_url = resolve_qdrant_url(inputs.global, inputs.toml)?;
    let custom_headers = validate_custom_headers(inputs.global.custom_headers.clone())?;
    let mcp_http_port = env_port("AXON_MCP_HTTP_PORT", 8001)?;

    let mut cfg = Config::default();
    populate_identity_and_crawl(&mut cfg, &inputs);
    populate_chrome_and_filtering(&mut cfg, &inputs);
    populate_perf_and_credentials(&mut cfg, &inputs);
    populate_services_and_ask_basics(&mut cfg, &inputs, tei_url, qdrant_url)?;
    populate_ask_tuning(&mut cfg, inputs.toml);
    populate_misc(&mut cfg, &inputs, custom_headers, mcp_http_port)?;
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
    cfg.retrieve_max_points = inputs.dispatched.retrieve_max_points;
    cfg.train_best_rank = inputs.dispatched.train_best_rank;
    cfg.train_notes = inputs.dispatched.train_notes.clone();
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
        .or_else(|| {
            inputs.toml.services.chrome_remote_url.clone().inspect(|_| {
                warn_legacy_service_url("chrome-remote-url", "AXON_CHROME_REMOTE_URL");
            })
        })
        .map(normalize_local_service_url);
    cfg.chrome_proxy = g
        .chrome_proxy
        .clone()
        .or_else(|| env::var("AXON_CHROME_PROXY").ok());
    cfg.chrome_user_agent = g
        .chrome_user_agent
        .clone()
        .or_else(|| env::var("AXON_CHROME_USER_AGENT").ok())
        .or_else(|| inputs.toml.chrome.user_agent.clone());
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
) -> Result<(), String> {
    cfg.tei_url = tei_url;
    cfg.qdrant_url = qdrant_url;
    cfg.headless_gemini_model = non_empty_env("AXON_HEADLESS_GEMINI_MODEL").unwrap_or_default();
    cfg.headless_gemini_cmd =
        non_empty_env("AXON_HEADLESS_GEMINI_CMD").unwrap_or_else(|| "gemini".to_string());
    cfg.headless_gemini_home = non_empty_env("AXON_HEADLESS_GEMINI_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| env::var("HOME").ok().map(std::path::PathBuf::from));
    cfg.llm_completion_concurrency =
        parse_positive_usize_env("AXON_LLM_COMPLETION_CONCURRENCY", 4)?;
    cfg.llm_completion_timeout_secs =
        parse_positive_u64_env("AXON_LLM_COMPLETION_TIMEOUT_SECS", 300)?;
    cfg.tavily_api_key = env::var("TAVILY_API_KEY").ok().unwrap_or_default();
    cfg.mcp_allowed_origins = env::var("AXON_MCP_ALLOWED_ORIGINS")
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
    cfg.ask_graph = false;
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
    cfg.watchdog_stale_timeout_secs = g.watchdog_stale_timeout_secs.max(30);
    cfg.watchdog_confirm_secs = g.watchdog_confirm_secs.max(10);
    cfg.watchdog_sweep_secs = g.watchdog_sweep_secs.clamp(1, 600);
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
    cfg.sources_by_schema_version = g.sources_by_schema_version;
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
    cfg.local_mode = g.local;
    cfg.server_url = if cfg.local_mode {
        None
    } else {
        resolve_server_url(g)?
    };
    cfg.client_mode = if cfg.server_url.is_some() {
        ClientMode::Server
    } else {
        ClientMode::Local
    };
    Ok(())
}

fn resolve_server_url(g: &GlobalArgs) -> Result<Option<reqwest::Url>, String> {
    let candidate = g
        .server_url
        .as_ref()
        .map(|value| ("--server-url / AXON_SERVER_URL", value.trim().to_string()))
        .filter(|(_, value)| !value.is_empty());

    candidate
        .map(|(source, raw)| {
            reqwest::Url::parse(&raw).map(Some).map_err(|e| {
                format!("invalid --server-url / AXON_SERVER_URL '{raw}' ({source}): {e}")
            })
        })
        .unwrap_or(Ok(None))
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

/// Emit once-per-process deprecation warnings for env vars that are no longer
/// honored — both `CompatibilityShim` (still accepted but slated for removal)
/// and `Delete` (already removed; presence is silently ignored without this
/// warning). Fires at every Config build but is guarded by a OnceLock so users
/// see warnings on first invocation, not on every repeated subcommand.
fn warn_compat_shim_env_vars() {
    use crate::core::config::parse::env_registry::{EnvClassification, LegacyBehavior, all_specs};
    use std::sync::OnceLock;
    static WARNED: OnceLock<()> = OnceLock::new();
    WARNED.get_or_init(|| {
        for spec in all_specs() {
            let is_compat = spec.classification == EnvClassification::CompatibilityShim;
            let is_deleted = spec.classification == EnvClassification::Delete;
            if !is_compat && !is_deleted {
                continue;
            }
            if env::var(spec.key).is_err() {
                continue;
            }
            let reason = if is_deleted {
                "removed in 3.0.0; this variable is ignored — run `axon setup repair --migrate-env` to scrub it from ~/.axon/.env. See docs/env-migration-matrix.md for the replacement."
            } else {
                match spec.legacy_behavior {
                    LegacyBehavior::WarnEnvOverride => {
                        "still accepted but will be removed; set the TOML equivalent instead"
                    }
                    LegacyBehavior::WarnAndIgnore => {
                        "ignored; this variable has no effect in the current runtime"
                    }
                    _ => "deprecated; consult docs/env-migration-matrix.md for the replacement",
                }
            };
            log_warn(&format!("env var {} is deprecated: {}", spec.key, reason));
        }
    });
}

fn warn_legacy_service_url(toml_key: &str, env_key: &str) {
    log_warn(&format!(
        "[services].{toml_key} is deprecated and will be ignored in a future release; move it to {env_key} in .env"
    ));
}
