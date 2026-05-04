use super::super::cli::{Cli, CliCommand};
use super::super::types::{
    CommandKind, Config, EvaluateResponsesMode, MapFallback, McpTransport, RedditSort, RedditTime,
};
use super::docker::normalize_local_service_url;
use super::excludes;
use super::helpers::{
    default_sqlite_path, env_bool, env_port, parse_origin_allowlist, parse_viewport,
    positional_from_job, positional_from_watch_subcommand, resolve_ask_adapter_args,
    resolve_ask_adapter_cmd, resolve_mcp_transport, validate_collection_name,
    validate_custom_headers,
};
use super::performance;
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
            super::super::cli::ServeSubcommand::Mcp(args) => {
                mcp_transport = args.transport;
                mcp_transport_default = McpTransport::Http;
                (CommandKind::Mcp, Vec::new())
            }
        },
        CliCommand::Mcp(args) => {
            mcp_transport = args.transport;
            mcp_transport_default = McpTransport::Stdio;
            (CommandKind::Mcp, Vec::new())
        }
        CliCommand::Migrate(args) => (CommandKind::Migrate, vec![args.from, args.to]),
    };

    if matches!(command, CommandKind::Completions) {
        return Ok(Config {
            command,
            positional,
            ..Config::default()
        });
    }

    // Validate collection name: it gets interpolated into Qdrant URL paths via format!()
    // with no percent-encoding. Reject anything that could break out of the path or
    // collide with reserved characters (CWE-22 — bd axon_rust-d71.6 / H2).
    validate_collection_name(&global.collection)?;

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
        collection: global.collection,
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
        ask_candidate_limit: performance::env_usize_clamped(
            "AXON_ASK_CANDIDATE_LIMIT",
            150,
            8,
            300,
        ),
        ask_chunk_limit: performance::env_usize_clamped("AXON_ASK_CHUNK_LIMIT", 10, 3, 40),
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
        ask_min_relevance_score: performance::env_f64_clamped(
            "AXON_ASK_MIN_RELEVANCE_SCORE",
            0.45,
            -1.0,
            2.0,
        ),
        ask_authoritative_domains: env::var("AXON_ASK_AUTHORITATIVE_DOMAINS")
            .ok()
            .map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_ascii_lowercase())
                    .collect()
            })
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
        hybrid_search_enabled: env_bool("AXON_HYBRID_SEARCH", true) && !global.no_hybrid_search,
        hybrid_search_candidates: performance::env_usize_clamped(
            "AXON_HYBRID_CANDIDATES",
            100,
            10,
            500,
        ),
        ask_hybrid_candidates: performance::env_usize_clamped(
            "AXON_ASK_HYBRID_CANDIDATES",
            150,
            10,
            500,
        ),
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

    // Validate collection name — Qdrant only allows [a-zA-Z0-9_-] (ASCII only, non-empty)
    if cfg.collection.is_empty()
        || !cfg
            .collection
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!(
            "invalid collection name '{}': must be non-empty and contain only ASCII letters, digits, underscores and hyphens",
            cfg.collection
        ));
    }

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
        && let Some(data_dir) = crate::crates::core::paths::axon_data_dir()
    {
        cfg.output_dir = data_dir.join("axon/output");
    }

    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::cli::Cli;
    use clap::Parser;
    use std::env;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

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
}
