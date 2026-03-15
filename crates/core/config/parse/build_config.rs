use super::super::cli::{Cli, CliCommand, RefreshScheduleSubcommand, RefreshSubcommand};
use super::super::types::{
    CommandKind, Config, EvaluateResponsesMode, McpTransport, RedditSort, RedditTime,
};
use super::docker::normalize_local_service_url;
use super::excludes;
use super::helpers::{
    parse_viewport, positional_from_job, positional_from_refresh_subcommand,
    positional_from_watch_subcommand,
};
use super::performance;
use clap::ValueEnum;
use std::env;

fn env_bool(key: &str, default: bool) -> bool {
    match env::var(key).ok().as_deref().map(str::trim) {
        None | Some("") => default,
        Some("true" | "1" | "yes") => true,
        Some("false" | "0" | "no") => false,
        Some(_) => default,
    }
}

fn parse_origin_allowlist(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn into_config(cli: Cli) -> Result<Config, String> {
    let global = cli.global;
    let fetch_retries_was_set = global.fetch_retries.is_some();
    let retry_backoff_was_set = global.retry_backoff_ms.is_some();

    let mut ask_diagnostics = false;
    let mut evaluate_responses_mode = EvaluateResponsesMode::Inline;
    let mut github_include_source = true;
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
    let mut serve_port = 49000u16;
    let mut mcp_transport = None;
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
        CliCommand::Refresh(args) => (
            CommandKind::Refresh,
            if let Some(action) = args.action {
                match action {
                    RefreshSubcommand::Schedule {
                        action:
                            RefreshScheduleSubcommand::Add {
                                name,
                                seed_url,
                                every_seconds,
                                tier,
                                urls,
                            },
                    } => {
                        if seed_url.is_none() && urls.is_none() {
                            return Err(
                                "refresh schedule add requires either [seed_url] or --urls <csv>"
                                    .to_string(),
                            );
                        }
                        positional_from_refresh_subcommand(RefreshSubcommand::Schedule {
                            action: RefreshScheduleSubcommand::Add {
                                name,
                                seed_url,
                                every_seconds,
                                tier,
                                urls,
                            },
                        })
                    }
                    other => positional_from_refresh_subcommand(other),
                }
            } else {
                args.positional_urls
            },
        ),
        CliCommand::Map(args) => (
            CommandKind::Map,
            args.value.into_iter().collect::<Vec<String>>(),
        ),
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
        CliCommand::Query(args) => (CommandKind::Query, args.value),
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
        CliCommand::Graph(args) => (CommandKind::Graph, args.value),
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
        CliCommand::Mcp(args) => {
            mcp_transport = args.transport;
            (CommandKind::Mcp, Vec::new())
        }
        CliCommand::Serve(args) => {
            serve_port = args.port;
            (CommandKind::Serve, Vec::new())
        }
    };

    if matches!(command, CommandKind::Completions) {
        return Ok(Config {
            command,
            positional,
            ..Config::default()
        });
    }

    let pg_url = normalize_local_service_url(
        global
            .pg_url
            .or_else(|| env::var("AXON_PG_URL").ok())
            .ok_or_else(|| {
                "AXON_PG_URL environment variable is required (or pass --pg-url). Copy .env.example to .env and fill in credentials.".to_string()
            })?,
    );

    let redis_url = normalize_local_service_url(
        global
            .redis_url
            .or_else(|| env::var("AXON_REDIS_URL").ok())
            .ok_or_else(|| {
                "AXON_REDIS_URL environment variable is required (or pass --redis-url). Copy .env.example to .env and fill in credentials.".to_string()
            })?,
    );

    let amqp_url = normalize_local_service_url(
        global
            .amqp_url
            .or_else(|| env::var("AXON_AMQP_URL").ok())
            .ok_or_else(|| {
                "AXON_AMQP_URL environment variable is required (or pass --amqp-url). Copy .env.example to .env and fill in credentials.".to_string()
            })?,
    );

    let mut crawl_concurrency_limit = global.crawl_concurrency_limit;
    let mut backfill_concurrency_limit = global.backfill_concurrency_limit;

    if let Some(limit) = global.concurrency_limit {
        crawl_concurrency_limit = Some(limit);
        backfill_concurrency_limit = Some(limit);
    }

    let normalized_excludes = excludes::normalize_exclude_prefixes(global.exclude_path_prefix);

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
        cache: global.cache,
        cache_skip_browser: global.cache_skip_browser,
        format: global.format,
        collection: global.collection,
        embed: global.embed,
        batch_concurrency: global.batch_concurrency.clamp(1, 512),
        wait: global.wait,
        yes: global.yes,
        performance_profile: global.performance_profile,
        crawl_concurrency_limit,
        backfill_concurrency_limit,
        sitemap_only: global.sitemap_only,
        delay_ms: global.delay_ms,
        request_timeout_ms: global.request_timeout_ms,
        fetch_retries: global.fetch_retries.unwrap_or(0),
        retry_backoff_ms: global.retry_backoff_ms.unwrap_or(0),
        shared_queue: global.shared_queue,
        pg_url,
        redis_url,
        amqp_url,
        crawl_queue: global
            .crawl_queue
            .or_else(|| env::var("AXON_CRAWL_QUEUE").ok())
            .unwrap_or_else(|| "axon.crawl.jobs".to_string()),
        refresh_queue: global
            .refresh_queue
            .or_else(|| env::var("AXON_REFRESH_QUEUE").ok())
            .unwrap_or_else(|| "axon.refresh.jobs".to_string()),
        extract_queue: global
            .extract_queue
            .or_else(|| env::var("AXON_EXTRACT_QUEUE").ok())
            .unwrap_or_else(|| "axon.extract.jobs".to_string()),
        embed_queue: global
            .embed_queue
            .or_else(|| env::var("AXON_EMBED_QUEUE").ok())
            .unwrap_or_else(|| "axon.embed.jobs".to_string()),
        ingest_queue: global
            .ingest_queue
            .or_else(|| env::var("AXON_INGEST_QUEUE").ok())
            .unwrap_or_else(|| "axon.ingest.jobs".to_string()),
        sessions_claude,
        sessions_codex,
        sessions_gemini,
        sessions_project,
        github_token: env::var("GITHUB_TOKEN").ok(),
        github_include_source,
        reddit_client_id: env::var("REDDIT_CLIENT_ID").ok(),
        reddit_client_secret: env::var("REDDIT_CLIENT_SECRET").ok(),
        reddit_sort,
        reddit_time,
        reddit_max_posts,
        reddit_min_score,
        reddit_depth,
        reddit_scrape_links,
        tei_url: global
            .tei_url
            .or_else(|| env::var("TEI_URL").ok())
            .unwrap_or_default(),
        qdrant_url: global
            .qdrant_url
            .or_else(|| env::var("QDRANT_URL").ok())
            .map(normalize_local_service_url)
            .unwrap_or_else(|| "http://127.0.0.1:53333".to_string()),
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
        acp_adapter_cmd: env::var("AXON_ACP_ADAPTER_CMD")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        acp_adapter_args: env::var("AXON_ACP_ADAPTER_ARGS")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        tavily_api_key: env::var("TAVILY_API_KEY").ok().unwrap_or_default(),
        neo4j_url: env::var("AXON_NEO4J_URL").ok().unwrap_or_default(),
        neo4j_user: env::var("AXON_NEO4J_USER")
            .ok()
            .unwrap_or_else(|| "neo4j".to_string()),
        neo4j_password: env::var("AXON_NEO4J_PASSWORD").ok().unwrap_or_default(),
        graph_queue: env::var("AXON_GRAPH_QUEUE")
            .ok()
            .unwrap_or_else(|| "axon.graph.jobs".to_string()),
        graph_concurrency: env::var("AXON_GRAPH_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4),
        graph_llm_url: env::var("AXON_GRAPH_LLM_URL")
            .ok()
            .unwrap_or_else(|| "http://localhost:11434".to_string()),
        graph_llm_model: env::var("AXON_GRAPH_LLM_MODEL")
            .ok()
            .unwrap_or_else(|| "qwen3.5:2b".to_string()),
        graph_similarity_threshold: env::var("AXON_GRAPH_SIMILARITY_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.75),
        graph_similarity_limit: env::var("AXON_GRAPH_SIMILARITY_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20),
        graph_context_max_chars: env::var("AXON_GRAPH_CONTEXT_MAX_CHARS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2_000),
        graph_taxonomy_path: env::var("AXON_GRAPH_TAXONOMY_PATH")
            .ok()
            .unwrap_or_default(),
        web_allowed_origins: env::var("AXON_WEB_ALLOWED_ORIGINS")
            .ok()
            .map(|raw| parse_origin_allowlist(&raw))
            .unwrap_or_default(),
        shell_allowed_origins: env::var("AXON_SHELL_ALLOWED_ORIGINS")
            .ok()
            .map(|raw| parse_origin_allowlist(&raw))
            .unwrap_or_default(),
        ask_diagnostics,
        ask_graph: global.graph,
        evaluate_responses_mode,
        ask_max_context_chars: performance::env_usize_clamped(
            "AXON_ASK_MAX_CONTEXT_CHARS",
            120_000,
            20_000,
            400_000,
        ),
        ask_candidate_limit: performance::env_usize_clamped("AXON_ASK_CANDIDATE_LIMIT", 64, 8, 200),
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
                    .map(|item| item.trim().to_ascii_lowercase())
                    .filter(|item| !item.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        ask_authoritative_boost: performance::env_f64_clamped(
            "AXON_ASK_AUTHORITATIVE_BOOST",
            0.0,
            0.0,
            0.5,
        ),
        ask_authoritative_allowlist: env::var("AXON_ASK_AUTHORITATIVE_ALLOWLIST")
            .ok()
            .map(|raw| {
                raw.split(',')
                    .map(|item| item.trim().to_ascii_lowercase())
                    .filter(|item| !item.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        ask_min_citations_nontrivial: performance::env_usize_clamped(
            "AXON_ASK_MIN_CITATIONS_NONTRIVIAL",
            2,
            1,
            5,
        ),
        hybrid_search_enabled: env_bool("AXON_HYBRID_SEARCH", true),
        hybrid_search_candidates: performance::env_usize_clamped(
            "AXON_HYBRID_CANDIDATES",
            100,
            10,
            500,
        ),
        cron_every_seconds: global.cron_every_seconds.filter(|value| *value > 0),
        cron_max_runs: global.cron_max_runs.filter(|value| *value > 0),
        watchdog_stale_timeout_secs: global.watchdog_stale_timeout_secs.max(30),
        watchdog_confirm_secs: global.watchdog_confirm_secs.max(10),
        json_output: global.json,
        reclaimed_status_only: global.reclaimed,
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
        bypass_csp: global.bypass_csp,
        accept_invalid_certs: global.accept_invalid_certs,
        screenshot_full_page: global.screenshot_full_page,
        viewport_width: {
            let (w, _) = parse_viewport(&global.viewport);
            w
        },
        viewport_height: {
            let (_, h) = parse_viewport(&global.viewport);
            h
        },
        serve_port,
        mcp_transport: resolve_mcp_transport(mcp_transport, env::var("AXON_MCP_TRANSPORT").ok())?,
        mcp_http_host: env::var("AXON_MCP_HTTP_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
        mcp_http_port: env::var("AXON_MCP_HTTP_PORT")
            .ok()
            .as_deref()
            .map(parse_mcp_http_port)
            .transpose()?
            .unwrap_or(8001),
        custom_headers: global.custom_headers,
    };

    if cfg.exclude_path_prefix.is_empty() && !normalized_excludes.disable_defaults {
        cfg.exclude_path_prefix = excludes::default_exclude_prefixes();
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
    // This unifies local dev and Docker: both write to $AXON_DATA_DIR/axon/output.
    if cfg.output_dir == std::path::Path::new(".cache/axon-rust/output")
        && let Ok(data_dir) = env::var("AXON_DATA_DIR")
    {
        let data_dir = data_dir.trim();
        if !data_dir.is_empty() {
            cfg.output_dir = std::path::PathBuf::from(data_dir).join("axon/output");
        }
    }

    Ok(cfg)
}

fn resolve_mcp_transport(
    cli_transport: Option<McpTransport>,
    env_transport: Option<String>,
) -> Result<McpTransport, String> {
    if let Some(transport) = cli_transport {
        return Ok(transport);
    }

    match env_transport
        .as_deref()
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
    {
        None => Ok(McpTransport::Http),
        Some(raw) => match raw {
            "stdio" => Ok(McpTransport::Stdio),
            "http" => Ok(McpTransport::Http),
            "both" => Ok(McpTransport::Both),
            _ => Err(format!(
                "invalid AXON_MCP_TRANSPORT '{raw}' (expected stdio, http, or both)"
            )),
        },
    }
}

fn parse_mcp_http_port(raw: &str) -> Result<u16, String> {
    raw.parse::<u16>()
        .map_err(|e| format!("invalid AXON_MCP_HTTP_PORT '{raw}': {e}"))
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
    fn into_config_parses_web_origin_allowlists_from_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        const WEB: &str = "AXON_WEB_ALLOWED_ORIGINS";
        const SHELL: &str = "AXON_SHELL_ALLOWED_ORIGINS";

        unsafe {
            env::set_var(WEB, " https://axon.example.com , http://localhost:49010 ");
            env::set_var(SHELL, " http://localhost:49011 ");
        }

        let cli = Cli::parse_from([
            "axon",
            "--pg-url",
            "postgresql://axon:postgres@127.0.0.1:53432/axon",
            "--redis-url",
            "redis://127.0.0.1:53379",
            "--amqp-url",
            "amqp://axon:axonrabbit@127.0.0.1:45535/%2f",
            "status",
        ]);
        let cfg = into_config(cli).expect("status config should parse");

        assert_eq!(
            cfg.web_allowed_origins,
            vec![
                "https://axon.example.com".to_string(),
                "http://localhost:49010".to_string(),
            ]
        );
        assert_eq!(
            cfg.shell_allowed_origins,
            vec!["http://localhost:49011".to_string()]
        );

        unsafe {
            env::remove_var(WEB);
            env::remove_var(SHELL);
        }
    }
}
