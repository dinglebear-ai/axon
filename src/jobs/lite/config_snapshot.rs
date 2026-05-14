mod endpoint;
mod paths;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::config::{Config, RenderMode, ScrapeFormat};
use crate::jobs::ingest::IngestSource;

use endpoint::endpoint_snapshot;
use paths::normalize_container_output_dir;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct LiteConfigEnvelope {
    version: u8,
    config: LiteConfigSnapshot,
    prompt: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct LiteIngestConfigEnvelope {
    version: u8,
    source: Option<IngestSource>,
    config: LiteConfigSnapshot,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct LiteConfigSnapshot {
    collection: Option<String>,
    output_dir: Option<PathBuf>,
    output_path: Option<PathBuf>,
    query: Option<String>,
    search_limit: Option<usize>,
    max_pages: Option<u32>,
    max_depth: Option<usize>,
    include_subdomains: Option<bool>,
    exclude_path_prefix: Option<Vec<String>>,
    render_mode: Option<RenderMode>,
    chrome_remote_url: Option<String>,
    chrome_proxy: Option<String>,
    chrome_user_agent: Option<String>,
    chrome_headless: Option<bool>,
    chrome_anti_bot: Option<bool>,
    chrome_intercept: Option<bool>,
    chrome_stealth: Option<bool>,
    chrome_bootstrap: Option<bool>,
    chrome_bootstrap_timeout_ms: Option<u64>,
    chrome_bootstrap_retries: Option<usize>,
    respect_robots: Option<bool>,
    min_markdown_chars: Option<usize>,
    drop_thin_markdown: Option<bool>,
    discover_sitemaps: Option<bool>,
    sitemap_since_days: Option<u32>,
    map_fallback: Option<crate::core::config::MapFallback>,
    max_sitemaps: Option<usize>,
    cache: Option<bool>,
    cache_skip_browser: Option<bool>,
    format: Option<ScrapeFormat>,
    embed: Option<bool>,
    batch_concurrency: Option<usize>,
    crawl_concurrency_limit: Option<usize>,
    backfill_concurrency_limit: Option<usize>,
    sitemap_only: Option<bool>,
    delay_ms: Option<u64>,
    request_timeout_ms: Option<u64>,
    fetch_retries: Option<usize>,
    retry_backoff_ms: Option<u64>,
    sessions_claude: Option<bool>,
    sessions_codex: Option<bool>,
    sessions_gemini: Option<bool>,
    sessions_project: Option<String>,
    github_include_source: Option<bool>,
    github_max_issues: Option<usize>,
    github_max_prs: Option<usize>,
    reddit_sort: Option<crate::core::config::RedditSort>,
    reddit_time: Option<crate::core::config::RedditTime>,
    reddit_max_posts: Option<usize>,
    reddit_min_score: Option<i32>,
    reddit_depth: Option<usize>,
    reddit_scrape_links: Option<bool>,
    tei_url: Option<String>,
    qdrant_url: Option<String>,
    openai_base_url: Option<String>,
    openai_model: Option<String>,
    headless_gemini_model: Option<String>,
    headless_gemini_cmd: Option<String>,
    headless_gemini_home: Option<PathBuf>,
    llm_completion_concurrency: Option<usize>,
    llm_completion_timeout_secs: Option<u64>,
    ask_diagnostics: Option<bool>,
    ask_graph: Option<bool>,
    ask_max_context_chars: Option<usize>,
    ask_candidate_limit: Option<usize>,
    ask_chunk_limit: Option<usize>,
    ask_full_docs: Option<usize>,
    ask_backfill_chunks: Option<usize>,
    ask_doc_fetch_concurrency: Option<usize>,
    ask_doc_chunk_limit: Option<usize>,
    ask_min_relevance_score: Option<f64>,
    ask_authoritative_domains: Option<Vec<String>>,
    ask_authoritative_boost: Option<f64>,
    ask_min_citations_nontrivial: Option<usize>,
    hybrid_search_enabled: Option<bool>,
    evaluate_retrieval_ab: Option<bool>,
    hybrid_search_candidates: Option<usize>,
    ask_hybrid_candidates: Option<usize>,
    normalize: Option<bool>,
    chrome_network_idle_timeout_secs: Option<u64>,
    auto_switch_thin_ratio: Option<f64>,
    auto_switch_min_pages: Option<usize>,
    crawl_broadcast_buffer_min: Option<usize>,
    crawl_broadcast_buffer_max: Option<usize>,
    url_whitelist: Option<Vec<String>>,
    block_assets: Option<bool>,
    max_page_bytes: Option<u64>,
    redirect_policy_strict: Option<bool>,
    chrome_wait_for_selector: Option<String>,
    root_selector: Option<String>,
    exclude_selector: Option<String>,
    chrome_screenshot: Option<bool>,
    research_depth: Option<usize>,
    search_time_range: Option<String>,
    since: Option<String>,
    before: Option<String>,
    bypass_csp: Option<bool>,
    accept_invalid_certs: Option<bool>,
    screenshot_full_page: Option<bool>,
    viewport_width: Option<u32>,
    viewport_height: Option<u32>,
    custom_headers: Option<Vec<String>>,
    quiet: Option<bool>,
    process_fallback_fields: Vec<String>,
}

impl LiteConfigSnapshot {
    fn from_config(cfg: &Config) -> Result<Self, String> {
        let mut process_fallback_fields = Vec::new();
        Ok(Self {
            collection: Some(cfg.collection.clone()),
            output_dir: Some(cfg.output_dir.clone()),
            output_path: cfg.output_path.clone(),
            query: cfg.query.clone(),
            search_limit: Some(cfg.search_limit),
            max_pages: Some(cfg.max_pages),
            max_depth: Some(cfg.max_depth),
            include_subdomains: Some(cfg.include_subdomains),
            exclude_path_prefix: Some(cfg.exclude_path_prefix.clone()),
            render_mode: Some(cfg.render_mode),
            chrome_remote_url: cfg.chrome_remote_url.clone(),
            chrome_proxy: cfg.chrome_proxy.clone(),
            chrome_user_agent: cfg.chrome_user_agent.clone(),
            chrome_headless: Some(cfg.chrome_headless),
            chrome_anti_bot: Some(cfg.chrome_anti_bot),
            chrome_intercept: Some(cfg.chrome_intercept),
            chrome_stealth: Some(cfg.chrome_stealth),
            chrome_bootstrap: Some(cfg.chrome_bootstrap),
            chrome_bootstrap_timeout_ms: Some(cfg.chrome_bootstrap_timeout_ms),
            chrome_bootstrap_retries: Some(cfg.chrome_bootstrap_retries),
            respect_robots: Some(cfg.respect_robots),
            min_markdown_chars: Some(cfg.min_markdown_chars),
            drop_thin_markdown: Some(cfg.drop_thin_markdown),
            discover_sitemaps: Some(cfg.discover_sitemaps),
            sitemap_since_days: Some(cfg.sitemap_since_days),
            map_fallback: Some(cfg.map_fallback),
            max_sitemaps: Some(cfg.max_sitemaps),
            cache: Some(cfg.cache),
            cache_skip_browser: Some(cfg.cache_skip_browser),
            format: Some(cfg.format),
            embed: Some(cfg.embed),
            batch_concurrency: Some(cfg.batch_concurrency),
            crawl_concurrency_limit: cfg.crawl_concurrency_limit,
            backfill_concurrency_limit: cfg.backfill_concurrency_limit,
            sitemap_only: Some(cfg.sitemap_only),
            delay_ms: Some(cfg.delay_ms),
            request_timeout_ms: cfg.request_timeout_ms,
            fetch_retries: Some(cfg.fetch_retries),
            retry_backoff_ms: Some(cfg.retry_backoff_ms),
            sessions_claude: Some(cfg.sessions_claude),
            sessions_codex: Some(cfg.sessions_codex),
            sessions_gemini: Some(cfg.sessions_gemini),
            sessions_project: cfg.sessions_project.clone(),
            github_include_source: Some(cfg.github_include_source),
            github_max_issues: Some(cfg.github_max_issues),
            github_max_prs: Some(cfg.github_max_prs),
            reddit_sort: Some(cfg.reddit_sort),
            reddit_time: Some(cfg.reddit_time),
            reddit_max_posts: Some(cfg.reddit_max_posts),
            reddit_min_score: Some(cfg.reddit_min_score),
            reddit_depth: Some(cfg.reddit_depth),
            reddit_scrape_links: Some(cfg.reddit_scrape_links),
            tei_url: endpoint_snapshot("tei_url", &cfg.tei_url, &mut process_fallback_fields)?,
            qdrant_url: endpoint_snapshot(
                "qdrant_url",
                &cfg.qdrant_url,
                &mut process_fallback_fields,
            )?,
            openai_base_url: endpoint_snapshot(
                "openai_base_url",
                &cfg.openai_base_url,
                &mut process_fallback_fields,
            )?,
            openai_model: Some(cfg.openai_model.clone()),
            headless_gemini_model: Some(cfg.headless_gemini_model.clone()),
            headless_gemini_cmd: Some(cfg.headless_gemini_cmd.clone()),
            headless_gemini_home: cfg.headless_gemini_home.clone(),
            llm_completion_concurrency: Some(cfg.llm_completion_concurrency),
            llm_completion_timeout_secs: Some(cfg.llm_completion_timeout_secs),
            ask_diagnostics: Some(cfg.ask_diagnostics),
            ask_graph: Some(cfg.ask_graph),
            ask_max_context_chars: Some(cfg.ask_max_context_chars),
            ask_candidate_limit: Some(cfg.ask_candidate_limit),
            ask_chunk_limit: Some(cfg.ask_chunk_limit),
            ask_full_docs: Some(cfg.ask_full_docs),
            ask_backfill_chunks: Some(cfg.ask_backfill_chunks),
            ask_doc_fetch_concurrency: Some(cfg.ask_doc_fetch_concurrency),
            ask_doc_chunk_limit: Some(cfg.ask_doc_chunk_limit),
            ask_min_relevance_score: Some(cfg.ask_min_relevance_score),
            ask_authoritative_domains: Some(cfg.ask_authoritative_domains.clone()),
            ask_authoritative_boost: Some(cfg.ask_authoritative_boost),
            ask_min_citations_nontrivial: Some(cfg.ask_min_citations_nontrivial),
            hybrid_search_enabled: Some(cfg.hybrid_search_enabled),
            evaluate_retrieval_ab: Some(cfg.evaluate_retrieval_ab),
            hybrid_search_candidates: Some(cfg.hybrid_search_candidates),
            ask_hybrid_candidates: Some(cfg.ask_hybrid_candidates),
            normalize: Some(cfg.normalize),
            chrome_network_idle_timeout_secs: Some(cfg.chrome_network_idle_timeout_secs),
            auto_switch_thin_ratio: Some(cfg.auto_switch_thin_ratio),
            auto_switch_min_pages: Some(cfg.auto_switch_min_pages),
            crawl_broadcast_buffer_min: Some(cfg.crawl_broadcast_buffer_min),
            crawl_broadcast_buffer_max: Some(cfg.crawl_broadcast_buffer_max),
            url_whitelist: Some(cfg.url_whitelist.clone()),
            block_assets: Some(cfg.block_assets),
            max_page_bytes: cfg.max_page_bytes,
            redirect_policy_strict: Some(cfg.redirect_policy_strict),
            chrome_wait_for_selector: cfg.chrome_wait_for_selector.clone(),
            root_selector: cfg.root_selector.clone(),
            exclude_selector: cfg.exclude_selector.clone(),
            chrome_screenshot: Some(cfg.chrome_screenshot),
            research_depth: cfg.research_depth,
            search_time_range: cfg.search_time_range.clone(),
            since: cfg.since.clone(),
            before: cfg.before.clone(),
            bypass_csp: Some(cfg.bypass_csp),
            accept_invalid_certs: Some(cfg.accept_invalid_certs),
            screenshot_full_page: Some(cfg.screenshot_full_page),
            viewport_width: Some(cfg.viewport_width),
            viewport_height: Some(cfg.viewport_height),
            custom_headers: Some(cfg.custom_headers.clone()),
            quiet: Some(cfg.quiet),
            process_fallback_fields,
        })
    }

    fn apply_to(self, cfg: &mut Config, exact_options: bool) {
        let mut snapshot = self;
        let fallback_fields = std::mem::take(&mut snapshot.process_fallback_fields);
        snapshot.apply_regular_fields(cfg);
        snapshot.apply_option_fields(cfg, exact_options, &fallback_fields);
    }

    fn apply_regular_fields(&mut self, cfg: &mut Config) {
        macro_rules! set {
            ($($field:ident),+ $(,)?) => {
                $(if let Some(value) = self.$field.take() { cfg.$field = value; })+
            };
        }
        set!(
            collection,
            output_dir,
            search_limit,
            max_pages,
            max_depth,
            include_subdomains,
            exclude_path_prefix,
            render_mode,
            custom_headers,
            chrome_headless,
            chrome_anti_bot,
            chrome_intercept,
            chrome_stealth,
            chrome_bootstrap,
            chrome_bootstrap_timeout_ms,
            chrome_bootstrap_retries,
            respect_robots,
            min_markdown_chars,
            drop_thin_markdown,
            discover_sitemaps,
            sitemap_since_days,
            map_fallback,
            max_sitemaps,
            cache,
            cache_skip_browser,
            format,
            embed,
            batch_concurrency,
            sitemap_only,
            delay_ms,
            fetch_retries,
            retry_backoff_ms,
            sessions_claude,
            sessions_codex,
            sessions_gemini,
            github_include_source,
            github_max_issues,
            github_max_prs,
            reddit_sort,
            reddit_time,
            reddit_max_posts,
            reddit_min_score,
            reddit_depth,
            reddit_scrape_links,
            tei_url,
            qdrant_url,
            openai_base_url,
            openai_model,
            headless_gemini_model,
            headless_gemini_cmd,
            llm_completion_concurrency,
            llm_completion_timeout_secs,
            ask_diagnostics,
            ask_graph,
            ask_max_context_chars,
            ask_candidate_limit,
            ask_chunk_limit,
            ask_full_docs,
            ask_backfill_chunks,
            ask_doc_fetch_concurrency,
            ask_doc_chunk_limit,
            ask_min_relevance_score,
            ask_authoritative_domains,
            ask_authoritative_boost,
            ask_min_citations_nontrivial,
            hybrid_search_enabled,
            evaluate_retrieval_ab,
            hybrid_search_candidates,
            ask_hybrid_candidates,
            normalize,
            chrome_network_idle_timeout_secs,
            auto_switch_thin_ratio,
            auto_switch_min_pages,
            crawl_broadcast_buffer_min,
            crawl_broadcast_buffer_max,
            url_whitelist,
            block_assets,
            redirect_policy_strict,
            chrome_screenshot,
            bypass_csp,
            accept_invalid_certs,
            screenshot_full_page,
            viewport_width,
            viewport_height,
            quiet,
        );
    }

    fn apply_option_fields(
        &mut self,
        cfg: &mut Config,
        exact_options: bool,
        fallback_fields: &[String],
    ) {
        macro_rules! set_option_exact {
            ($($field:ident),+ $(,)?) => {
                $(if exact_options && !fallback_fields.iter().any(|name| name == stringify!($field)) {
                    cfg.$field = self.$field.take();
                } else if let Some(value) = self.$field.take() {
                    cfg.$field = Some(value);
                })+
            };
        }
        set_option_exact!(
            output_path,
            query,
            chrome_remote_url,
            chrome_proxy,
            chrome_user_agent,
            crawl_concurrency_limit,
            backfill_concurrency_limit,
            request_timeout_ms,
            headless_gemini_home,
            sessions_project,
            max_page_bytes,
            chrome_wait_for_selector,
            root_selector,
            exclude_selector,
            research_depth,
            search_time_range,
            since,
            before,
        );
    }
}

pub(crate) fn lite_config_snapshot_json(cfg: &Config) -> Result<String, serde_json::Error> {
    serde_json::to_string(&LiteConfigEnvelope {
        version: 2,
        config: LiteConfigSnapshot::from_config(cfg).map_err(serde_json_error)?,
        prompt: None,
    })
}

pub(crate) fn extract_config_json(
    cfg: &Config,
    prompt: Option<String>,
) -> Result<String, serde_json::Error> {
    let mut effective = cfg.clone();
    if let Some(prompt) = &prompt {
        effective.query = Some(prompt.clone());
    }
    serde_json::to_string(&LiteConfigEnvelope {
        version: 2,
        config: LiteConfigSnapshot::from_config(&effective).map_err(serde_json_error)?,
        prompt,
    })
}

pub(crate) fn apply_lite_config_snapshot(
    process_cfg: &Config,
    config_json: &str,
) -> Result<Config, Box<dyn std::error::Error + Send + Sync>> {
    apply_lite_config_snapshot_for_container(process_cfg, config_json, running_in_container())
}

pub(crate) fn apply_lite_config_snapshot_for_container(
    process_cfg: &Config,
    config_json: &str,
    in_container: bool,
) -> Result<Config, Box<dyn std::error::Error + Send + Sync>> {
    let mut cfg = process_cfg.clone();
    if config_json.trim().is_empty() {
        return Ok(cfg);
    }
    let envelope = decode_config_envelope(config_json)?;
    let exact_options = envelope.version >= 2;
    envelope.config.apply_to(&mut cfg, exact_options);
    if let Some(prompt) = envelope.prompt {
        cfg.query = Some(prompt);
    }
    normalize_container_output_dir(process_cfg, &mut cfg, in_container);
    Ok(cfg)
}

fn running_in_container() -> bool {
    std::env::var("AXON_IN_CONTAINER").is_ok_and(|value| value.trim() == "1")
}

pub(crate) fn ingest_config_json(
    cfg: &Config,
    source: &IngestSource,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&LiteIngestConfigEnvelope {
        version: 2,
        source: Some(source.clone()),
        config: LiteConfigSnapshot::from_config(cfg).map_err(serde_json_error)?,
    })
}

fn serde_json_error(message: String) -> serde_json::Error {
    serde_json::Error::io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message,
    ))
}

pub(crate) fn decode_ingest_job_config(
    process_cfg: &Config,
    config_json: &str,
) -> Result<(IngestSource, Config), Box<dyn std::error::Error + Send + Sync>> {
    if let Ok(envelope) = serde_json::from_str::<LiteIngestConfigEnvelope>(config_json)
        && let Some(source) = envelope.source
    {
        let mut cfg = process_cfg.clone();
        let exact_options = envelope.version >= 2;
        envelope.config.apply_to(&mut cfg, exact_options);
        return Ok((source, cfg));
    }

    let source: IngestSource = serde_json::from_str(config_json)?;
    Ok((source, process_cfg.clone()))
}

fn decode_config_envelope(
    config_json: &str,
) -> Result<LiteConfigEnvelope, Box<dyn std::error::Error + Send + Sync>> {
    let value: serde_json::Value = serde_json::from_str(config_json)?;
    if value.get("config").is_some() || value.get("prompt").is_some() {
        return Ok(serde_json::from_value(value)?);
    }

    let snapshot = serde_json::from_value(value)?;
    Ok(LiteConfigEnvelope {
        version: 0,
        config: snapshot,
        prompt: None,
    })
}
