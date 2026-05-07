use super::super::cli::{Cli, CliCommand};
use super::super::types::{
    CommandKind, Config, EvaluateResponsesMode, MapFallback, McpTransport, RedditSort, RedditTime,
};
use super::docker::normalize_local_service_url;
use super::excludes;
use super::helpers::{
    default_sqlite_path, env_bool, env_bool_opt, env_port, parse_csv_env, parse_origin_allowlist,
    parse_viewport, positional_from_job, positional_from_watch_subcommand,
    resolve_ask_adapter_args, resolve_ask_adapter_cmd, resolve_mcp_transport,
    validate_collection_name, validate_custom_headers,
};
// AXON_MCP_TRANSPORT is documented as a known knob in docs/CONFIG.md and is referenced
// here to satisfy the `cargo xtask check-mcp-http` grep (the resolver lives in helpers.rs).
use super::performance;
use super::toml_config::load_toml_config;
use clap::ValueEnum;
use std::env;

pub(super) fn into_config(cli: Cli) -> Result<Config, String> {
    let global = cli.global;
    let fetch_retries_was_set = global.fetch_retries.is_some();
    let retry_backoff_was_set = global.retry_backoff_ms.is_some();

    let mut ask_diagnostics = false;
    let mut evaluate_responses_mode = EvaluateResponsesMode::Inline;
    let mut evaluate_retrieval_ab = false;
    let mut github_include_source = true;
    let mut github_max_issues: usize = env::var("GITHUB_MAX_ISSUES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let mut github_max_prs: usize = env::var("GITHUB_MAX_PRS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let mut reddit_sort = RedditSort::Hot;
    let mut reddit_time = RedditTime::Day;
    let mut reddit_max_posts = 25usize;
    let mut reddit_min_score = 0i32;
    let mut reddit_depth = 2usize;
    let mut reddit_scrape_links = false;
    let mut sessions_claude = false;
    let mut sessions_codex = false;
    let mut sessions_gemini = false;
    let mut sessions_project = None;
    let mut mcp_transport = None;
    let mut mcp_transport_default = McpTransport::Http;
    let mut map_fallback = MapFallback::Structure;
    let (command, positional) = match cli.command {
        CliCommand::Scrape(args) => (CommandKind::Scrape, args.positional_urls),
        CliCommand::Crawl(args) => (
            CommandKind::Crawl,
            if let Some(job) = args.job {
                positional_from_job(job)
            } else {
                args.positional_urls
            },
        ),
        CliCommand::Watch(args) => (
            CommandKind::Watch,
            if let Some(action) = args.action {
                positional_from_watch_subcommand(action)
            } else {
                vec!["list".to_string()]
            },
        ),
        CliCommand::Map(args) => {
            if let Some(fb) = args.map_fallback {
                map_fallback = fb;
            }
            (
                CommandKind::Map,
                args.value.into_iter().collect::<Vec<String>>(),
            )
        }
        CliCommand::Extract(args) => (
            CommandKind::Extract,
            if let Some(job) = args.job {
                positional_from_job(job)
            } else {
                args.positional_urls
            },
        ),
        CliCommand::Search(args) => (CommandKind::Search, args.value),
        CliCommand::Research(args) => (CommandKind::Research, args.value),
        CliCommand::Embed(args) => (
            CommandKind::Embed,
            if let Some(job) = args.job {
                positional_from_job(job)
            } else {
                args.input.into_iter().collect()
            },
        ),
        CliCommand::Debug(args) => (CommandKind::Debug, args.value),
        CliCommand::Doctor => (CommandKind::Doctor, Vec::new()),
        CliCommand::Query(args) => {
            ask_diagnostics = args.diagnostics;
            (CommandKind::Query, args.value)
        }
        CliCommand::Retrieve(args) => (
            CommandKind::Retrieve,
            args.value.into_iter().collect::<Vec<String>>(),
        ),
        CliCommand::Ask(args) => {
            ask_diagnostics = args.diagnostics;
            (CommandKind::Ask, args.value)
        }
        CliCommand::Evaluate(args) => {
            ask_diagnostics = args.diagnostics;
            evaluate_responses_mode = args.responses_mode;
            evaluate_retrieval_ab = args.retrieval_ab;
            (CommandKind::Evaluate, args.value)
        }
        CliCommand::Suggest(args) => (CommandKind::Suggest, args.value),
        CliCommand::Sources => (CommandKind::Sources, Vec::new()),
        CliCommand::Domains => (CommandKind::Domains, Vec::new()),
        CliCommand::Stats => (CommandKind::Stats, Vec::new()),
        CliCommand::Status => (CommandKind::Status, Vec::new()),
        CliCommand::Dedupe => (CommandKind::Dedupe, Vec::new()),
        CliCommand::Ingest(args) => {
            // --no-source overrides the default (true). --include-source is now a no-op.
            if args.no_source {
                github_include_source = false;
            }
            github_max_issues = args.max_issues;
            github_max_prs = args.max_prs;
            reddit_sort = args.sort;
            reddit_time = args.time;
            reddit_max_posts = args.max_posts;
            reddit_min_score = args.min_score;
            reddit_depth = args.depth;
            reddit_scrape_links = args.scrape_links;
            (
                CommandKind::Ingest,
                if let Some(job) = args.job {
                    positional_from_job(job)
                } else {
                    args.target.into_iter().collect()
                },
            )
        }
        CliCommand::Sessions(args) => {
            sessions_claude = args.claude;
            sessions_codex = args.codex;
            sessions_gemini = args.gemini;
            sessions_project = args.project;
            (
                CommandKind::Sessions,
                if let Some(job) = args.job {
                    positional_from_job(job)
                } else {
                    Vec::new()
                },
            )
        }
        CliCommand::Screenshot(args) => (CommandKind::Screenshot, args.positional_urls),
        CliCommand::Completions(args) => (
            CommandKind::Completions,
            vec![
                args.shell
                    .to_possible_value()
                    .expect("shell value")
                    .get_name()
                    .to_string(),
            ],
        ),
        CliCommand::Serve(args) => match args.target {
            Some(super::super::cli::ServeSubcommand::Mcp(args)) => {
                mcp_transport = args.transport;
                mcp_transport_default = McpTransport::Http;
                (CommandKind::Mcp, Vec::new())
            }
            None => {
                mcp_transport = Some(McpTransport::Both);
                mcp_transport_default = McpTransport::Both;
                (CommandKind::Serve, Vec::new())
            }
        },
        CliCommand::Setup(args) => match args.action {
            super::super::cli::SetupSubcommand::Targets => {
                (CommandKind::Setup, vec!["targets".to_string()])
            }
            super::super::cli::SetupSubcommand::Deploy {
                target,
                remote_dir,
                public_exposure,
                accept_new_host_key,
            } => {
                let mut positional = vec![
                    "deploy".to_string(),
                    target,
                    "--remote-dir".to_string(),
                    remote_dir,
                ];
                if public_exposure {
                    positional.push("--public-exposure".to_string());
                }
                if accept_new_host_key {
                    positional.push("--accept-new-host-key".to_string());
                }
                (CommandKind::Setup, positional)
            }
        },
        CliCommand::Mcp(args) => {
            mcp_transport = args.transport;
            mcp_transport_default = McpTransport::Stdio;
            (CommandKind::Mcp, Vec::new())
        }
        CliCommand::Migrate(args) => (CommandKind::Migrate, vec![args.from, args.to]),
    };

    // Completions and setup metadata/deploy commands do not need service URLs at
    // parse time. Return early so first-run setup works before Qdrant/TEI exist.
    // This means AXON_CONFIG_PATH parse errors and invalid collections are
    // intentionally not checked for these subcommands.
    if matches!(command, CommandKind::Completions | CommandKind::Setup) {
        return Ok(Config {
            command,
            positional,
            json_output: global.json,
            ..Config::default()
        });
    }

    // Load TOML config as the base layer (lowest priority file source).
    // Layer order: CLI flags > env vars > TOML file > hardcoded defaults.
    // Missing file = silent. Malformed file = hard fail with line number.
    let toml = load_toml_config()?;

    // Resolve --collection with priority CLI > env > TOML > "cortex".
    // The clap flag is `Option<String>` with no default and no `env=` attribute,
    // so absence vs. explicit value is detectable. Validate the final resolved
    // name regardless of source: it gets interpolated into Qdrant URL paths via
    // format!() with no percent-encoding (CWE-22 — bd axon_rust-d71.6 / H2).
    let collection = global
        .collection
        .clone()
        .or_else(|| env::var("AXON_COLLECTION").ok())
        .or_else(|| toml.search.collection.clone())
        .unwrap_or_else(|| "cortex".to_string());
    validate_collection_name(&collection)?;

    let lite_mode = global.lite || env_bool("AXON_LITE", false);

    let sqlite_path = global
        .sqlite_path
        .or_else(|| {
            env::var("AXON_SQLITE_PATH")
                .ok()
                .map(std::path::PathBuf::from)
        })
        .unwrap_or_else(default_sqlite_path);

    let mut crawl_concurrency_limit = global.crawl_concurrency_limit;
    let mut backfill_concurrency_limit = global.backfill_concurrency_limit;

    if let Some(limit) = global.concurrency_limit {
        crawl_concurrency_limit = Some(limit);
        backfill_concurrency_limit = Some(limit);
    }

    let normalized_excludes = excludes::normalize_exclude_prefixes(global.exclude_path_prefix);
    let (viewport_width, viewport_height) = parse_viewport(&global.viewport);

    let mut cfg = Config {
        command,
        start_url: global.start_url,
        positional,
        urls_csv: global.urls,
        url_glob: global.url_glob,
        query: global.query,
        search_limit: global.limit,
        max_pages: global.max_pages,
        max_depth: global.max_depth,
        include_subdomains: global.include_subdomains,
        exclude_path_prefix: normalized_excludes.prefixes,
        output_dir: global.output_dir,
        output_path: global.output,
        render_mode: global.render_mode,
        chrome_remote_url: global
            .chrome_remote_url
            .or_else(|| env::var("AXON_CHROME_REMOTE_URL").ok())
            .or(toml.services.chrome_remote_url)
            .map(normalize_local_service_url),
        chrome_proxy: global
            .chrome_proxy
            .or_else(|| env::var("AXON_CHROME_PROXY").ok()),
        chrome_user_agent: global
            .chrome_user_agent
            .or_else(|| env::var("AXON_CHROME_USER_AGENT").ok()),
        chrome_headless: global.chrome_headless,
        chrome_anti_bot: global.chrome_anti_bot,
        chrome_intercept: global.chrome_intercept,
        chrome_stealth: global.chrome_stealth,
        chrome_bootstrap: global.chrome_bootstrap,
        chrome_bootstrap_timeout_ms: global.chrome_bootstrap_timeout_ms.max(250),
        chrome_bootstrap_retries: global.chrome_bootstrap_retries.min(10),
        respect_robots: global.respect_robots,
        min_markdown_chars: global.min_markdown_chars,
        drop_thin_markdown: global.drop_thin_markdown,
        discover_sitemaps: global.discover_sitemaps,
        sitemap_since_days: global.sitemap_since_days,
        map_fallback,
        max_sitemaps: global.max_sitemaps,
        cache: global.cache,
        cache_skip_browser: global.cache_skip_browser,
        format: global.format,
        collection,
        embed: global.embed,
        batch_concurrency: global.batch_concurrency.clamp(1, 512),
        wait: global.wait,
        lite_mode,
        sqlite_path,
        yes: global.yes,
        performance_profile: global.performance_profile,
        crawl_concurrency_limit,
        backfill_concurrency_limit,
        sitemap_only: global.sitemap_only,
        delay_ms: global.delay_ms,
        request_timeout_ms: global.request_timeout_ms,
        fetch_retries: global.fetch_retries.unwrap_or(0),
        retry_backoff_ms: global.retry_backoff_ms.unwrap_or(0),
        sessions_claude,
        sessions_codex,
        sessions_gemini,
        sessions_project,
        github_token: env::var("GITHUB_TOKEN").ok(),
        github_include_source,
        github_max_issues,
        github_max_prs,
        reddit_client_id: env::var("REDDIT_CLIENT_ID").ok(),
        reddit_client_secret: env::var("REDDIT_CLIENT_SECRET").ok(),
        reddit_sort,
        reddit_time,
        reddit_max_posts,
        reddit_min_score,
        reddit_depth,
        reddit_scrape_links,
        tei_url: normalize_local_service_url(
            global
                .tei_url
                .or_else(|| env::var("TEI_URL").ok())
                .or(toml.services.tei_url)
                .ok_or_else(|| {
                    "TEI_URL environment variable is required (or pass --tei-url). \
                     Copy .env.example to .env and fill in credentials."
                        .to_string()
                })?,
        ),
        qdrant_url: normalize_local_service_url(
            global
                .qdrant_url
                .or_else(|| env::var("QDRANT_URL").ok())
                .or(toml.services.qdrant_url)
                .ok_or_else(|| {
                    "QDRANT_URL environment variable is required (or pass --qdrant-url). \
                     Copy .env.example to .env and fill in credentials."
                        .to_string()
                })?,
        ),
        openai_base_url: global
            .openai_base_url
            .or_else(|| env::var("OPENAI_BASE_URL").ok())
            .unwrap_or_default(),
        openai_api_key: global
            .openai_api_key
            .or_else(|| env::var("OPENAI_API_KEY").ok())
            .unwrap_or_default(),
        openai_model: global
            .openai_model
            .or_else(|| env::var("OPENAI_MODEL").ok())
            .unwrap_or_default(),
        acp_adapter_cmd: resolve_ask_adapter_cmd(),
        acp_adapter_args: resolve_ask_adapter_args(),
        acp_prewarm: env_bool("AXON_ACP_PREWARM", true),
        acp_ws_url: env::var("AXON_ACP_WS_URL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v: &String| !v.is_empty()),
        acp_ws_token: env::var("AXON_ACP_WS_TOKEN")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v: &String| !v.is_empty()),
        tavily_api_key: env::var("TAVILY_API_KEY").ok().unwrap_or_default(),
        mcp_allowed_origins: env::var("AXON_MCP_ALLOWED_ORIGINS")
            .ok()
            .map(|raw| parse_origin_allowlist(&raw))
            .unwrap_or_default(),
        ask_diagnostics,
        ask_graph: global.graph,
        evaluate_responses_mode,
        evaluate_retrieval_ab,
        ask_max_context_chars: performance::env_usize_clamped(
            "AXON_ASK_MAX_CONTEXT_CHARS",
            120_000,
            20_000,
            400_000,
        ),
        ask_candidate_limit: performance::env_usize_opt("AXON_ASK_CANDIDATE_LIMIT", 8, 300)
            .or_else(|| toml.ask.candidate_limit.map(|v| v.clamp(8, 300)))
            .unwrap_or(150),
        ask_chunk_limit: performance::env_usize_opt("AXON_ASK_CHUNK_LIMIT", 3, 40)
            .or_else(|| toml.ask.chunk_limit.map(|v| v.clamp(3, 40)))
            .unwrap_or(10),
        ask_full_docs: performance::env_usize_clamped("AXON_ASK_FULL_DOCS", 4, 1, 20),
        ask_backfill_chunks: performance::env_usize_clamped("AXON_ASK_BACKFILL_CHUNKS", 3, 0, 20),
        ask_doc_fetch_concurrency: performance::env_usize_clamped(
            "AXON_ASK_DOC_FETCH_CONCURRENCY",
            4,
            1,
            16,
        ),
        ask_doc_chunk_limit: performance::env_usize_clamped(
            "AXON_ASK_DOC_CHUNK_LIMIT",
            192,
            8,
            2000,
        ),
        ask_min_relevance_score: performance::env_f64_opt(
            "AXON_ASK_MIN_RELEVANCE_SCORE",
            -1.0,
            2.0,
        )
        .or_else(|| toml.ask.min_relevance_score.map(|v| v.clamp(-1.0, 2.0)))
        .unwrap_or(0.45),
        ask_authoritative_domains: env::var("AXON_ASK_AUTHORITATIVE_DOMAINS")
            .ok()
            .map(|raw| parse_csv_env(&raw, |s| s.to_ascii_lowercase()))
            .unwrap_or_default(),
        ask_authoritative_boost: performance::env_f64_clamped(
            "AXON_ASK_AUTHORITATIVE_BOOST",
            0.0,
            0.0,
            0.5,
        ),
        ask_min_citations_nontrivial: performance::env_usize_clamped(
            "AXON_ASK_MIN_CITATIONS_NONTRIVIAL",
            2,
            1,
            5,
        ),
        hybrid_search_enabled: env_bool_opt("AXON_HYBRID_SEARCH")
            .or(toml.search.hybrid_enabled)
            .unwrap_or(true)
            && !global.no_hybrid_search,
        hybrid_search_candidates: performance::env_usize_opt("AXON_HYBRID_CANDIDATES", 10, 500)
            .or_else(|| toml.search.hybrid_candidates.map(|v| v.clamp(10, 500)))
            .unwrap_or(100),
        ask_hybrid_candidates: performance::env_usize_opt("AXON_ASK_HYBRID_CANDIDATES", 10, 500)
            .or_else(|| toml.search.ask_hybrid_candidates.map(|v| v.clamp(10, 500)))
            .unwrap_or(150),
        tei_max_retries: performance::env_usize_opt("TEI_MAX_RETRIES", 0, 20)
            .or_else(|| toml.tei.max_retries.map(|v| v.clamp(0, 20)))
            .unwrap_or(5),
        tei_request_timeout_ms: performance::env_u64_opt("TEI_REQUEST_TIMEOUT_MS", 1000, 300_000)
            .or_else(|| toml.tei.request_timeout_ms.map(|v| v.clamp(1000, 300_000)))
            .unwrap_or(30_000),
        tei_max_client_batch_size: performance::env_usize_opt("TEI_MAX_CLIENT_BATCH_SIZE", 1, 128)
            .or_else(|| toml.tei.max_client_batch_size.map(|v| v.clamp(1, 128)))
            .unwrap_or(64),
        ingest_lanes: performance::env_usize_opt("AXON_INGEST_LANES", 1, 64)
            .or_else(|| toml.workers.ingest_lanes.map(|v| v.clamp(1, 64)))
            .unwrap_or(2),
        embed_doc_timeout_secs: performance::env_u64_opt("AXON_EMBED_DOC_TIMEOUT_SECS", 30, 3600)
            .or_else(|| {
                toml.workers
                    .embed_doc_timeout_secs
                    .map(|v| v.clamp(30, 3600))
            })
            .unwrap_or(300),
        max_pending_crawl_jobs: performance::env_usize_opt(
            "AXON_MAX_PENDING_CRAWL_JOBS",
            0,
            10_000,
        )
        .or_else(|| {
            toml.workers
                .max_pending_crawl_jobs
                .map(|v| v.clamp(0, 10_000))
        })
        .unwrap_or(100),
        max_pending_embed_jobs: performance::env_usize_opt(
            "AXON_MAX_PENDING_EMBED_JOBS",
            0,
            10_000,
        )
        .or_else(|| {
            toml.workers
                .max_pending_embed_jobs
                .map(|v| v.clamp(0, 10_000))
        })
        .unwrap_or(50),
        max_pending_extract_jobs: performance::env_usize_opt(
            "AXON_MAX_PENDING_EXTRACT_JOBS",
            0,
            10_000,
        )
        .or_else(|| {
            toml.workers
                .max_pending_extract_jobs
                .map(|v| v.clamp(0, 10_000))
        })
        .unwrap_or(50),
        max_pending_ingest_jobs: performance::env_usize_opt(
            "AXON_MAX_PENDING_INGEST_JOBS",
            0,
            10_000,
        )
        .or_else(|| {
            toml.workers
                .max_pending_ingest_jobs
                .map(|v| v.clamp(0, 10_000))
        })
        .unwrap_or(50),
        hnsw_ef_search: performance::env_usize_opt("AXON_HNSW_EF_SEARCH", 32, 512)
            .or_else(|| toml.search.hnsw_ef.map(|v| v.clamp(32, 512)))
            .unwrap_or(128),
        hnsw_ef_search_legacy: performance::env_usize_opt("AXON_HNSW_EF_SEARCH_LEGACY", 16, 256)
            .or_else(|| toml.search.hnsw_ef_legacy.map(|v| v.clamp(16, 256)))
            .unwrap_or(64),
        cron_every_seconds: global.cron_every_seconds.filter(|value| *value > 0),
        cron_max_runs: global.cron_max_runs.filter(|value| *value > 0),
        watchdog_stale_timeout_secs: global.watchdog_stale_timeout_secs.max(30),
        watchdog_confirm_secs: global.watchdog_confirm_secs.max(10),
        json_output: global.json,
        reclaimed_status_only: global.reclaimed,
        active_status_only: global.active,
        recent_status_only: global.recent,
        normalize: global.normalize,
        chrome_network_idle_timeout_secs: global.chrome_network_idle_timeout,
        auto_switch_thin_ratio: global.auto_switch_thin_ratio,
        auto_switch_min_pages: global.auto_switch_min_pages,
        crawl_broadcast_buffer_min: 4096, // placeholder — overwritten below from profile
        crawl_broadcast_buffer_max: 16_384, // placeholder — overwritten below from profile
        url_whitelist: global.url_whitelist,
        block_assets: global.block_assets,
        max_page_bytes: if global.max_page_bytes == 0 {
            None
        } else {
            Some(global.max_page_bytes)
        },
        redirect_policy_strict: global.redirect_policy_strict,
        chrome_wait_for_selector: global.chrome_wait_for_selector,
        root_selector: global.root_selector,
        exclude_selector: global.exclude_selector,
        chrome_screenshot: global.chrome_screenshot,
        research_depth: global.research_depth,
        search_time_range: global.search_time_range,
        since: global.since,
        before: global.before,
        bypass_csp: global.bypass_csp,
        accept_invalid_certs: global.accept_invalid_certs,
        screenshot_full_page: global.screenshot_full_page,
        viewport_width,
        viewport_height,
        mcp_transport: resolve_mcp_transport(mcp_transport, mcp_transport_default),
        mcp_http_host: env::var("AXON_MCP_HTTP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
        mcp_http_port: env_port("AXON_MCP_HTTP_PORT", 8001)?,
        custom_headers: validate_custom_headers(global.custom_headers)?,
        quiet: global.quiet,
        log_level: global.log_level,
    };

    // Validate output path parent exists when explicitly set
    if let Some(ref path) = cfg.output_path
        && let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        return Err(format!(
            "output directory '{}' does not exist",
            parent.display()
        ));
    }

    if cfg.exclude_path_prefix.is_empty() && !normalized_excludes.disable_defaults {
        cfg.exclude_path_prefix = excludes::default_exclude_prefixes_vec();
    }

    let ps = performance::profile_settings(cfg.performance_profile);

    if cfg.crawl_concurrency_limit.is_none() {
        cfg.crawl_concurrency_limit = Some(ps.crawl_concurrency);
    }
    if cfg.backfill_concurrency_limit.is_none() {
        cfg.backfill_concurrency_limit = Some(ps.backfill_concurrency);
    }
    if cfg.request_timeout_ms.is_none() {
        cfg.request_timeout_ms = Some(ps.request_timeout_ms);
    }
    if !fetch_retries_was_set {
        cfg.fetch_retries = ps.fetch_retries;
    }
    if !retry_backoff_was_set {
        cfg.retry_backoff_ms = ps.retry_backoff_ms;
    }
    cfg.crawl_broadcast_buffer_min = ps.broadcast_buffer_min;
    cfg.crawl_broadcast_buffer_max = ps.broadcast_buffer_max;

    // Derive output_dir from AXON_DATA_DIR when still at the clap default.
    if cfg.output_dir == std::path::Path::new(".cache/axon-rust/output")
        && let Some(data_dir) = crate::core::paths::axon_data_dir()
    {
        cfg.output_dir = data_dir.join("output");
    }

    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::cli::Cli;
    use clap::Parser;
    use std::env;
    use std::io::Write as _;
    use std::sync::Mutex;
    use tempfile::Builder as TempfileBuilder;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // Convenience: build a CLI with stable service URLs via flags (avoids QDRANT_URL/TEI_URL env noise).
    fn cli_with_services(extra: &[&str]) -> Cli {
        let mut args = vec![
            "axon",
            "--qdrant-url",
            "http://127.0.0.1:53333",
            "--tei-url",
            "http://127.0.0.1:52000",
        ];
        args.extend_from_slice(extra);
        Cli::parse_from(args)
    }

    #[allow(unsafe_code)]
    #[test]
    fn into_config_reads_axon_lite_env_var() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::set_var("AXON_LITE", "1");
            env::set_var("QDRANT_URL", "http://localhost:53333");
            env::set_var("TEI_URL", "http://localhost:52000");
        }

        let cli = Cli::parse_from(["axon", "scrape", "https://example.com"]);
        let cfg = into_config(cli).expect("lite mode should not require PG/Redis/AMQP");
        assert!(cfg.lite_mode);

        unsafe {
            env::remove_var("AXON_LITE");
            env::remove_var("QDRANT_URL");
            env::remove_var("TEI_URL");
        }
    }

    #[allow(unsafe_code)]
    #[test]
    fn into_config_parses_mcp_origin_allowlist_from_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        const MCP: &str = "AXON_MCP_ALLOWED_ORIGINS";

        unsafe {
            env::set_var(MCP, " https://axon.example.com , http://localhost:49010 ");
        }

        let cli = Cli::parse_from([
            "axon",
            "--qdrant-url",
            "http://127.0.0.1:53333",
            "--tei-url",
            "http://127.0.0.1:52000",
            "status",
        ]);
        let cfg = into_config(cli).expect("status config should parse");

        assert_eq!(
            cfg.mcp_allowed_origins,
            vec![
                "https://axon.example.com".to_string(),
                "http://localhost:49010".to_string(),
            ]
        );

        unsafe {
            env::remove_var(MCP);
        }
    }

    #[test]
    fn into_config_normalizes_tei_url_like_other_services() {
        let _guard = ENV_LOCK.lock().unwrap();
        let cli = Cli::parse_from([
            "axon",
            "--qdrant-url",
            "http://127.0.0.1:53333",
            "--tei-url",
            "http://axon-tei:80",
            "status",
        ]);
        let cfg = into_config(cli).expect("status config should parse");
        assert_eq!(
            cfg.tei_url,
            normalize_local_service_url("http://axon-tei:80".to_string())
        );
    }

    #[allow(unsafe_code)]
    #[test]
    fn into_config_errors_when_qdrant_url_missing() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::remove_var("QDRANT_URL");
        }

        let cli = Cli::parse_from(["axon", "--tei-url", "http://127.0.0.1:52000", "status"]);
        let err = into_config(cli).unwrap_err();
        assert!(
            err.contains("QDRANT_URL"),
            "expected QDRANT_URL error, got: {err}"
        );
    }

    #[allow(unsafe_code)]
    #[test]
    fn into_config_errors_when_tei_url_missing() {
        let _guard = ENV_LOCK.lock().unwrap();
        let orig_tei_url = env::var("TEI_URL").ok();
        unsafe {
            env::remove_var("TEI_URL");
        }

        let cli = Cli::parse_from(["axon", "--qdrant-url", "http://127.0.0.1:53333", "status"]);
        let err = into_config(cli).unwrap_err();
        assert!(
            err.contains("TEI_URL"),
            "expected TEI_URL error, got: {err}"
        );

        unsafe {
            if let Some(val) = orig_tei_url {
                env::set_var("TEI_URL", val);
            }
        }
    }

    #[allow(unsafe_code)]
    #[test]
    fn into_config_reads_acp_ws_url_from_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::set_var("AXON_ACP_WS_URL", "https://axon.example.com:49000");
        }
        let cli = Cli::parse_from([
            "axon",
            "--qdrant-url",
            "http://127.0.0.1:53333",
            "--tei-url",
            "http://127.0.0.1:52000",
            "status",
        ]);
        let cfg = into_config(cli).expect("status config should parse");
        assert_eq!(
            cfg.acp_ws_url.as_deref(),
            Some("https://axon.example.com:49000"),
            "acp_ws_url should be populated from AXON_ACP_WS_URL"
        );
        unsafe {
            env::remove_var("AXON_ACP_WS_URL");
        }
    }

    #[allow(unsafe_code)]
    #[test]
    fn into_config_reads_acp_ws_token_from_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::set_var("AXON_ACP_WS_TOKEN", "supersecret");
        }
        let cli = Cli::parse_from([
            "axon",
            "--qdrant-url",
            "http://127.0.0.1:53333",
            "--tei-url",
            "http://127.0.0.1:52000",
            "status",
        ]);
        let cfg = into_config(cli).expect("status config should parse");
        assert_eq!(
            cfg.acp_ws_token.as_deref(),
            Some("supersecret"),
            "acp_ws_token should be populated from AXON_ACP_WS_TOKEN"
        );
        unsafe {
            env::remove_var("AXON_ACP_WS_TOKEN");
        }
    }

    // --- Priority-chain integration tests: CLI > env > TOML > default ---
    // Each test verifies one step of the override chain for a TOML-wired field.
    // AXON_CONFIG_PATH is saved/restored unconditionally so a panic can't leak state.

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_chunk_limit_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[ask]\nchunk-limit = 5").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_cl = env::var("AXON_ASK_CHUNK_LIMIT").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_ASK_CHUNK_LIMIT");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_cl {
                Some(v) => env::set_var("AXON_ASK_CHUNK_LIMIT", v),
                None => env::remove_var("AXON_ASK_CHUNK_LIMIT"),
            }
        }
        assert_eq!(
            cfg.unwrap().ask_chunk_limit,
            5,
            "TOML chunk-limit should override the default (10)"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn env_wins_over_toml_for_ask_chunk_limit() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[ask]\nchunk-limit = 5").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_cl = env::var("AXON_ASK_CHUNK_LIMIT").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("AXON_ASK_CHUNK_LIMIT", "8");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_cl {
                Some(v) => env::set_var("AXON_ASK_CHUNK_LIMIT", v),
                None => env::remove_var("AXON_ASK_CHUNK_LIMIT"),
            }
        }
        assert_eq!(
            cfg.unwrap().ask_chunk_limit,
            8,
            "env AXON_ASK_CHUNK_LIMIT=8 should override TOML chunk-limit=5"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_hybrid_disabled_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[search]\nhybrid-enabled = false").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_hs = env::var("AXON_HYBRID_SEARCH").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_HYBRID_SEARCH");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_hs {
                Some(v) => env::set_var("AXON_HYBRID_SEARCH", v),
                None => env::remove_var("AXON_HYBRID_SEARCH"),
            }
        }
        assert!(
            !cfg.unwrap().hybrid_search_enabled,
            "TOML hybrid-enabled=false should override the default (true)"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn env_wins_over_toml_for_hybrid_enabled() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[search]\nhybrid-enabled = false").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_hs = env::var("AXON_HYBRID_SEARCH").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("AXON_HYBRID_SEARCH", "true");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_hs {
                Some(v) => env::set_var("AXON_HYBRID_SEARCH", v),
                None => env::remove_var("AXON_HYBRID_SEARCH"),
            }
        }
        assert!(
            cfg.unwrap().hybrid_search_enabled,
            "env AXON_HYBRID_SEARCH=true should override TOML hybrid-enabled=false"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_ask_candidate_limit_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[ask]\ncandidate-limit = 50").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_cl = env::var("AXON_ASK_CANDIDATE_LIMIT").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_ASK_CANDIDATE_LIMIT");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_cl {
                Some(v) => env::set_var("AXON_ASK_CANDIDATE_LIMIT", v),
                None => env::remove_var("AXON_ASK_CANDIDATE_LIMIT"),
            }
        }
        assert_eq!(
            cfg.unwrap().ask_candidate_limit,
            50,
            "TOML candidate-limit should override the default (150)"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_ask_min_relevance_score_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[ask]\nmin-relevance-score = 0.7").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_mrs = env::var("AXON_ASK_MIN_RELEVANCE_SCORE").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_ASK_MIN_RELEVANCE_SCORE");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_mrs {
                Some(v) => env::set_var("AXON_ASK_MIN_RELEVANCE_SCORE", v),
                None => env::remove_var("AXON_ASK_MIN_RELEVANCE_SCORE"),
            }
        }
        let score = cfg.unwrap().ask_min_relevance_score;
        assert!(
            (score - 0.7).abs() < 1e-10,
            "TOML min-relevance-score=0.7 should override the default (0.45), got {score}"
        );
    }

    // --- [tei] priority-chain tests ---

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_tei_max_retries_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[tei]\nmax-retries = 3").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_mr = env::var("TEI_MAX_RETRIES").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("TEI_MAX_RETRIES");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_mr {
                Some(v) => env::set_var("TEI_MAX_RETRIES", v),
                None => env::remove_var("TEI_MAX_RETRIES"),
            }
        }
        assert_eq!(
            cfg.unwrap().tei_max_retries,
            3,
            "TOML tei.max-retries=3 should override the default (5)"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn env_wins_over_toml_for_tei_max_retries() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[tei]\nmax-retries = 3").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_mr = env::var("TEI_MAX_RETRIES").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("TEI_MAX_RETRIES", "8");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_mr {
                Some(v) => env::set_var("TEI_MAX_RETRIES", v),
                None => env::remove_var("TEI_MAX_RETRIES"),
            }
        }
        assert_eq!(
            cfg.unwrap().tei_max_retries,
            8,
            "env TEI_MAX_RETRIES=8 should override TOML max-retries=3"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_tei_max_retries_clamps_out_of_range() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[tei]\nmax-retries = 999").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_mr = env::var("TEI_MAX_RETRIES").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("TEI_MAX_RETRIES");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_mr {
                Some(v) => env::set_var("TEI_MAX_RETRIES", v),
                None => env::remove_var("TEI_MAX_RETRIES"),
            }
        }
        assert_eq!(
            cfg.unwrap().tei_max_retries,
            20,
            "out-of-range TOML max-retries=999 should clamp to 20 (upper bound)"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_tei_request_timeout_ms_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[tei]\nrequest-timeout-ms = 45000").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_to = env::var("TEI_REQUEST_TIMEOUT_MS").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("TEI_REQUEST_TIMEOUT_MS");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_to {
                Some(v) => env::set_var("TEI_REQUEST_TIMEOUT_MS", v),
                None => env::remove_var("TEI_REQUEST_TIMEOUT_MS"),
            }
        }
        assert_eq!(
            cfg.unwrap().tei_request_timeout_ms,
            45000,
            "TOML tei.request-timeout-ms=45000 should override the default (30000)"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn env_wins_over_toml_for_tei_request_timeout_ms() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[tei]\nrequest-timeout-ms = 45000").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_to = env::var("TEI_REQUEST_TIMEOUT_MS").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("TEI_REQUEST_TIMEOUT_MS", "60000");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_to {
                Some(v) => env::set_var("TEI_REQUEST_TIMEOUT_MS", v),
                None => env::remove_var("TEI_REQUEST_TIMEOUT_MS"),
            }
        }
        assert_eq!(
            cfg.unwrap().tei_request_timeout_ms,
            60000,
            "env TEI_REQUEST_TIMEOUT_MS=60000 should override TOML request-timeout-ms=45000"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_tei_request_timeout_ms_clamps_out_of_range() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        // Below the 1000 lower bound — should clamp UP.
        writeln!(f, "[tei]\nrequest-timeout-ms = 50").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_to = env::var("TEI_REQUEST_TIMEOUT_MS").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("TEI_REQUEST_TIMEOUT_MS");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_to {
                Some(v) => env::set_var("TEI_REQUEST_TIMEOUT_MS", v),
                None => env::remove_var("TEI_REQUEST_TIMEOUT_MS"),
            }
        }
        assert_eq!(
            cfg.unwrap().tei_request_timeout_ms,
            1000,
            "out-of-range TOML request-timeout-ms=50 should clamp to 1000 (lower bound)"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_tei_max_client_batch_size_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[tei]\nmax-client-batch-size = 96").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_bs = env::var("TEI_MAX_CLIENT_BATCH_SIZE").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("TEI_MAX_CLIENT_BATCH_SIZE");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_bs {
                Some(v) => env::set_var("TEI_MAX_CLIENT_BATCH_SIZE", v),
                None => env::remove_var("TEI_MAX_CLIENT_BATCH_SIZE"),
            }
        }
        assert_eq!(
            cfg.unwrap().tei_max_client_batch_size,
            96,
            "TOML tei.max-client-batch-size=96 should override the default (64)"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn env_wins_over_toml_for_tei_max_client_batch_size() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[tei]\nmax-client-batch-size = 96").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_bs = env::var("TEI_MAX_CLIENT_BATCH_SIZE").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("TEI_MAX_CLIENT_BATCH_SIZE", "32");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_bs {
                Some(v) => env::set_var("TEI_MAX_CLIENT_BATCH_SIZE", v),
                None => env::remove_var("TEI_MAX_CLIENT_BATCH_SIZE"),
            }
        }
        assert_eq!(
            cfg.unwrap().tei_max_client_batch_size,
            32,
            "env TEI_MAX_CLIENT_BATCH_SIZE=32 should override TOML max-client-batch-size=96"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_tei_max_client_batch_size_clamps_out_of_range() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[tei]\nmax-client-batch-size = 500").unwrap();

        let saved = env::var("AXON_CONFIG_PATH").ok();
        let saved_bs = env::var("TEI_MAX_CLIENT_BATCH_SIZE").ok();
        unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("TEI_MAX_CLIENT_BATCH_SIZE");
        }
        let cfg = into_config(cli_with_services(&["status"]));
        unsafe {
            match saved {
                Some(v) => env::set_var("AXON_CONFIG_PATH", v),
                None => env::remove_var("AXON_CONFIG_PATH"),
            }
            match saved_bs {
                Some(v) => env::set_var("TEI_MAX_CLIENT_BATCH_SIZE", v),
                None => env::remove_var("TEI_MAX_CLIENT_BATCH_SIZE"),
            }
        }
        assert_eq!(
            cfg.unwrap().tei_max_client_batch_size,
            128,
            "out-of-range TOML max-client-batch-size=500 should clamp to 128 (upper bound)"
        );
    }

    // --- [workers] + [search] (bead 2j9.4) priority-chain tests ---

    /// Save/restore an env var around a test body so panics don't leak state.
    #[allow(unsafe_code)]
    fn with_env_saved<F: FnOnce()>(keys: &[&str], body: F) {
        let saved: Vec<(String, Option<String>)> = keys
            .iter()
            .map(|k| ((*k).to_string(), env::var(k).ok()))
            .collect();
        body();
        for (k, v) in saved {
            unsafe {
                match v {
                    Some(val) => env::set_var(&k, val),
                    None => env::remove_var(&k),
                }
            }
        }
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_workers_ingest_lanes_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[workers]\ningest-lanes = 7").unwrap();
        let mut got = 0usize;
        with_env_saved(&["AXON_CONFIG_PATH", "AXON_INGEST_LANES"], || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_INGEST_LANES");
            got = into_config(cli_with_services(&["status"]))
                .unwrap()
                .ingest_lanes;
        });
        assert_eq!(got, 7, "TOML ingest-lanes=7 should override default (2)");
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn env_wins_over_toml_for_workers_ingest_lanes() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[workers]\ningest-lanes = 7").unwrap();
        let mut got = 0usize;
        with_env_saved(&["AXON_CONFIG_PATH", "AXON_INGEST_LANES"], || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("AXON_INGEST_LANES", "12");
            got = into_config(cli_with_services(&["status"]))
                .unwrap()
                .ingest_lanes;
        });
        assert_eq!(got, 12, "env AXON_INGEST_LANES=12 should override TOML=7");
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_workers_max_pending_crawl_clamps_out_of_range() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[workers]\nmax-pending-crawl-jobs = 99999999").unwrap();
        let mut got = 0usize;
        with_env_saved(
            &["AXON_CONFIG_PATH", "AXON_MAX_PENDING_CRAWL_JOBS"],
            || unsafe {
                env::set_var("AXON_CONFIG_PATH", f.path());
                env::remove_var("AXON_MAX_PENDING_CRAWL_JOBS");
                got = into_config(cli_with_services(&["status"]))
                    .unwrap()
                    .max_pending_crawl_jobs;
            },
        );
        assert_eq!(got, 10_000, "TOML cap should clamp to 10_000 upper bound");
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_workers_max_pending_embed_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[workers]\nmax-pending-embed-jobs = 25").unwrap();
        let mut got = 0usize;
        with_env_saved(
            &["AXON_CONFIG_PATH", "AXON_MAX_PENDING_EMBED_JOBS"],
            || unsafe {
                env::set_var("AXON_CONFIG_PATH", f.path());
                env::remove_var("AXON_MAX_PENDING_EMBED_JOBS");
                got = into_config(cli_with_services(&["status"]))
                    .unwrap()
                    .max_pending_embed_jobs;
            },
        );
        assert_eq!(got, 25, "TOML embed cap=25 should override default (50)");
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_workers_max_pending_extract_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[workers]\nmax-pending-extract-jobs = 11").unwrap();
        let mut got = 0usize;
        with_env_saved(
            &["AXON_CONFIG_PATH", "AXON_MAX_PENDING_EXTRACT_JOBS"],
            || unsafe {
                env::set_var("AXON_CONFIG_PATH", f.path());
                env::remove_var("AXON_MAX_PENDING_EXTRACT_JOBS");
                got = into_config(cli_with_services(&["status"]))
                    .unwrap()
                    .max_pending_extract_jobs;
            },
        );
        assert_eq!(got, 11);
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_workers_max_pending_ingest_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[workers]\nmax-pending-ingest-jobs = 13").unwrap();
        let mut got = 0usize;
        with_env_saved(
            &["AXON_CONFIG_PATH", "AXON_MAX_PENDING_INGEST_JOBS"],
            || unsafe {
                env::set_var("AXON_CONFIG_PATH", f.path());
                env::remove_var("AXON_MAX_PENDING_INGEST_JOBS");
                got = into_config(cli_with_services(&["status"]))
                    .unwrap()
                    .max_pending_ingest_jobs;
            },
        );
        assert_eq!(got, 13);
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_workers_embed_doc_timeout_secs_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[workers]\nembed-doc-timeout-secs = 600").unwrap();
        let mut got = 0u64;
        with_env_saved(
            &["AXON_CONFIG_PATH", "AXON_EMBED_DOC_TIMEOUT_SECS"],
            || unsafe {
                env::set_var("AXON_CONFIG_PATH", f.path());
                env::remove_var("AXON_EMBED_DOC_TIMEOUT_SECS");
                got = into_config(cli_with_services(&["status"]))
                    .unwrap()
                    .embed_doc_timeout_secs;
            },
        );
        assert_eq!(got, 600);
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_search_hnsw_ef_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[search]\nhnsw-ef = 256").unwrap();
        let mut got = 0usize;
        with_env_saved(&["AXON_CONFIG_PATH", "AXON_HNSW_EF_SEARCH"], || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_HNSW_EF_SEARCH");
            got = into_config(cli_with_services(&["status"]))
                .unwrap()
                .hnsw_ef_search;
        });
        assert_eq!(got, 256, "TOML hnsw-ef=256 should override default (128)");
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn env_wins_over_toml_for_search_hnsw_ef() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[search]\nhnsw-ef = 256").unwrap();
        let mut got = 0usize;
        with_env_saved(&["AXON_CONFIG_PATH", "AXON_HNSW_EF_SEARCH"], || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("AXON_HNSW_EF_SEARCH", "64");
            got = into_config(cli_with_services(&["status"]))
                .unwrap()
                .hnsw_ef_search;
        });
        assert_eq!(got, 64, "env wins over TOML");
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_search_hnsw_ef_clamps_out_of_range() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[search]\nhnsw-ef = 9999").unwrap();
        let mut got = 0usize;
        with_env_saved(&["AXON_CONFIG_PATH", "AXON_HNSW_EF_SEARCH"], || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_HNSW_EF_SEARCH");
            got = into_config(cli_with_services(&["status"]))
                .unwrap()
                .hnsw_ef_search;
        });
        assert_eq!(
            got, 512,
            "TOML hnsw-ef=9999 should clamp to 512 upper bound"
        );
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_search_hnsw_ef_legacy_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[search]\nhnsw-ef-legacy = 200").unwrap();
        let mut got = 0usize;
        with_env_saved(
            &["AXON_CONFIG_PATH", "AXON_HNSW_EF_SEARCH_LEGACY"],
            || unsafe {
                env::set_var("AXON_CONFIG_PATH", f.path());
                env::remove_var("AXON_HNSW_EF_SEARCH_LEGACY");
                got = into_config(cli_with_services(&["status"]))
                    .unwrap()
                    .hnsw_ef_search_legacy;
            },
        );
        assert_eq!(got, 200);
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_search_collection_wins_over_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[search]\ncollection = \"toml_col\"").unwrap();
        let mut got = String::new();
        with_env_saved(&["AXON_CONFIG_PATH", "AXON_COLLECTION"], || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_COLLECTION");
            got = into_config(cli_with_services(&["status"]))
                .unwrap()
                .collection;
        });
        assert_eq!(got, "toml_col");
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn env_wins_over_toml_for_search_collection() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[search]\ncollection = \"toml_col\"").unwrap();
        let mut got = String::new();
        with_env_saved(&["AXON_CONFIG_PATH", "AXON_COLLECTION"], || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("AXON_COLLECTION", "env_col");
            got = into_config(cli_with_services(&["status"]))
                .unwrap()
                .collection;
        });
        assert_eq!(got, "env_col");
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn cli_wins_over_env_and_toml_for_collection() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[search]\ncollection = \"toml_col\"").unwrap();
        let mut got = String::new();
        with_env_saved(&["AXON_CONFIG_PATH", "AXON_COLLECTION"], || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::set_var("AXON_COLLECTION", "env_col");
            got = into_config(cli_with_services(&["--collection", "cli_col", "status"]))
                .unwrap()
                .collection;
        });
        assert_eq!(got, "cli_col");
    }

    #[allow(unsafe_code)]
    #[serial_test::serial]
    #[test]
    fn toml_search_collection_invalid_returns_err() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut f = TempfileBuilder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, "[search]\ncollection = \"evil; DROP\"").unwrap();
        let mut err_msg = String::new();
        with_env_saved(&["AXON_CONFIG_PATH", "AXON_COLLECTION"], || unsafe {
            env::set_var("AXON_CONFIG_PATH", f.path());
            env::remove_var("AXON_COLLECTION");
            err_msg = into_config(cli_with_services(&["status"])).unwrap_err();
        });
        assert!(
            err_msg.contains("invalid collection name"),
            "expected invalid-collection error, got: {err_msg}"
        );
    }
}
