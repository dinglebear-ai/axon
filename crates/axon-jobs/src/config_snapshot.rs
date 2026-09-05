mod adaptive;
mod apply;
mod endpoint;
mod errors;
mod paths;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use adaptive::AdaptiveConcurrencySnapshot;
use axon_core::config::{Config, RenderMode, ScrapeFormat};
use endpoint::{snapshot_chrome_remote_url, snapshot_endpoints};
use errors::{running_in_container, serde_json_error};
use paths::normalize_container_output_dir;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct ConfigSnapshotEnvelope {
    version: u8,
    config: ConfigSnapshot,
    prompt: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct ConfigSnapshot {
    collection: Option<String>,
    output_dir: Option<PathBuf>,
    output_path: Option<PathBuf>,
    warc_output: Option<PathBuf>,
    automation_script: Option<PathBuf>,
    query: Option<String>,
    search_limit: Option<usize>,
    max_pages: Option<u32>,
    max_depth: Option<usize>,
    include_subdomains: Option<bool>,
    exclude_path_prefix: Option<Vec<String>>,
    render_mode: Option<RenderMode>,
    chrome_remote_url: Option<String>,
    chrome_proxy: Option<String>,
    user_agent: Option<String>,
    chrome_user_agent: Option<String>,
    chrome_bootstrap_timeout_ms: Option<u64>,
    chrome_bootstrap_retries: Option<usize>,
    chrome_remote_local_policy: Option<bool>,
    respect_robots: Option<bool>,
    min_markdown_chars: Option<usize>,
    drop_thin_markdown: Option<bool>,
    discover_sitemaps: Option<bool>,
    sitemap_since_days: Option<u32>,
    max_sitemaps: Option<usize>,
    discover_llms_txt: Option<bool>,
    max_llms_txt_urls: Option<usize>,
    cache: Option<bool>,
    cache_http_only: Option<bool>,
    format: Option<ScrapeFormat>,
    embed: Option<bool>,
    batch_concurrency: Option<usize>,
    crawl_concurrency_limit: Option<usize>,
    backfill_concurrency_limit: Option<usize>,
    adaptive_concurrency: Option<AdaptiveConcurrencySnapshot>,
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
    reddit_sort: Option<axon_core::config::RedditSort>,
    reddit_time: Option<axon_core::config::RedditTime>,
    reddit_max_posts: Option<usize>,
    reddit_min_score: Option<i32>,
    reddit_depth: Option<usize>,
    reddit_scrape_links: Option<bool>,
    tei_url: Option<String>,
    qdrant_url: Option<String>,
    llm_backend: Option<String>,
    headless_gemini_model: Option<String>,
    headless_gemini_cmd: Option<String>,
    headless_gemini_home: Option<PathBuf>,
    codex_model: Option<String>,
    codex_completion_concurrency: Option<usize>,
    codex_load_user_config: Option<bool>,
    openai_base_url: Option<String>,
    openai_model: Option<String>,
    llm_completion_concurrency: Option<usize>,
    llm_completion_timeout_secs: Option<u64>,
    ask_diagnostics: Option<bool>,
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
    seed_url: Option<String>,
    bypass_csp: Option<bool>,
    accept_invalid_certs: Option<bool>,
    screenshot_full_page: Option<bool>,
    viewport_width: Option<u32>,
    viewport_height: Option<u32>,
    custom_headers: Option<Vec<String>>,
    quiet: Option<bool>,
    tei_max_retries: Option<usize>,
    tei_request_timeout_ms: Option<u64>,
    tei_max_client_batch_size: Option<usize>,
    embed_tei_max_concurrent: Option<usize>,
    embed_tei_max_in_flight_inputs: Option<usize>,
    embed_tei_retry_backoff_ms: Option<u64>,
    embed_tei_cooldown_after_failures: Option<usize>,
    embed_tei_cooldown_secs: Option<u64>,
    embed_tei_interactive_reserved_requests: Option<usize>,
    embed_tei_background_max_concurrent_requests: Option<usize>,
    embed_tei_maintenance_max_concurrent_requests: Option<usize>,
    embed_tei_query_instruction_enabled: Option<bool>,
    embed_cache_enabled: Option<bool>,
    embed_cache_max_entries: Option<usize>,
    embed_pool_max_inputs: Option<usize>,
    document_batch_size: Option<usize>,
    document_status_batch_size: Option<usize>,
    embed_tei_max_batch_tokens: Option<u32>,
    embed_scheduler_enabled: Option<bool>,
    vector_upsert_embed_overlap: Option<bool>,
    embed_prepared_byte_budget: Option<usize>,
    embed_prep_concurrency: Option<usize>,
    embed_prep_max_in_flight_bytes: Option<usize>,
    embed_scheduler_flush_ms: Option<u64>,
    chunking_markdown_max_chars: Option<usize>,
    chunking_markdown_min_chars: Option<usize>,
    chunking_overlap_chars: Option<usize>,
    embed_max_chunks_per_doc: Option<usize>,
    embed_max_source_chunks_per_doc: Option<usize>,
    embed_dedupe_exact_chunks: Option<bool>,
    openai_embed_model: Option<String>,
    openai_embed_max_client_batch_size: Option<usize>,
    openai_embed_max_concurrent: Option<usize>,
    openai_embed_max_in_flight_inputs: Option<usize>,
    openai_embed_pool_max_inputs: Option<usize>,
    unified_worker_concurrency: Option<usize>,
    source_job_concurrency_limit: Option<usize>,
    embed_doc_timeout_secs: Option<u64>,
    process_fallback_fields: Vec<String>,
}

impl ConfigSnapshot {
    fn from_config(cfg: &Config) -> Result<Self, String> {
        let mut process_fallback_fields = Vec::new();
        let endpoints = snapshot_endpoints(cfg, &mut process_fallback_fields)?;
        Ok(Self {
            collection: Some(cfg.collection.clone()),
            output_dir: Some(cfg.output_dir.clone()),
            output_path: cfg.output_path.clone(),
            warc_output: cfg.warc_output.clone(),
            automation_script: cfg.automation_script.clone(),
            query: cfg.query.clone(),
            search_limit: Some(cfg.search_limit),
            max_pages: Some(cfg.max_pages),
            max_depth: Some(cfg.max_depth),
            include_subdomains: Some(cfg.include_subdomains),
            exclude_path_prefix: Some(cfg.exclude_path_prefix.clone()),
            render_mode: Some(cfg.render_mode),
            chrome_remote_url: snapshot_chrome_remote_url(cfg, &mut process_fallback_fields)?,
            chrome_proxy: cfg.chrome_proxy.clone(),
            user_agent: cfg.user_agent.clone(),
            chrome_user_agent: cfg.chrome_user_agent.clone(),
            chrome_bootstrap_timeout_ms: Some(cfg.chrome_bootstrap_timeout_ms),
            chrome_bootstrap_retries: Some(cfg.chrome_bootstrap_retries),
            chrome_remote_local_policy: Some(cfg.chrome_remote_local_policy),
            respect_robots: Some(cfg.respect_robots),
            min_markdown_chars: Some(cfg.min_markdown_chars),
            drop_thin_markdown: Some(cfg.drop_thin_markdown),
            discover_sitemaps: Some(cfg.discover_sitemaps),
            sitemap_since_days: Some(cfg.sitemap_since_days),
            max_sitemaps: Some(cfg.max_sitemaps),
            discover_llms_txt: Some(cfg.discover_llms_txt),
            max_llms_txt_urls: Some(cfg.max_llms_txt_urls),
            cache: Some(cfg.cache),
            cache_http_only: Some(cfg.cache_http_only),
            format: Some(cfg.format),
            embed: Some(cfg.embed),
            batch_concurrency: Some(cfg.batch_concurrency),
            crawl_concurrency_limit: cfg.crawl_concurrency_limit,
            backfill_concurrency_limit: cfg.backfill_concurrency_limit,
            adaptive_concurrency: Some((&cfg.adaptive_concurrency).into()),
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
            ..Self::runtime_fields(
                cfg,
                endpoints.tei_url,
                endpoints.qdrant_url,
                endpoints.openai_base_url,
                process_fallback_fields,
            )
        })
    }

    fn runtime_fields(
        cfg: &Config,
        tei_url: Option<String>,
        qdrant_url: Option<String>,
        openai_base_url: Option<String>,
        process_fallback_fields: Vec<String>,
    ) -> Self {
        Self {
            tei_url,
            qdrant_url,
            llm_backend: Some(llm_backend_snapshot(cfg.llm_backend)),
            headless_gemini_model: Some(cfg.headless_gemini_model.clone()),
            headless_gemini_cmd: Some(cfg.headless_gemini_cmd.clone()),
            headless_gemini_home: cfg.headless_gemini_home.clone(),
            codex_model: Some(cfg.codex_model.clone()),
            codex_completion_concurrency: Some(cfg.codex_completion_concurrency),
            codex_load_user_config: Some(cfg.codex_load_user_config),
            openai_base_url,
            openai_model: Some(cfg.openai_model.clone()),
            llm_completion_concurrency: Some(cfg.llm_completion_concurrency),
            llm_completion_timeout_secs: Some(cfg.llm_completion_timeout_secs),
            ask_diagnostics: Some(cfg.ask_diagnostics),
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
            seed_url: cfg.seed_url.clone(),
            bypass_csp: Some(cfg.bypass_csp),
            accept_invalid_certs: Some(cfg.accept_invalid_certs),
            screenshot_full_page: Some(cfg.screenshot_full_page),
            viewport_width: Some(cfg.viewport_width),
            viewport_height: Some(cfg.viewport_height),
            custom_headers: Some(
                cfg.custom_headers
                    .iter()
                    .filter(|header| !header_contains_credential(header))
                    .cloned()
                    .collect(),
            ),
            quiet: Some(cfg.quiet),
            tei_max_retries: Some(cfg.tei_max_retries),
            tei_request_timeout_ms: Some(cfg.tei_request_timeout_ms),
            tei_max_client_batch_size: Some(cfg.tei_max_client_batch_size),
            embed_tei_max_concurrent: Some(cfg.embed_tei_max_concurrent),
            embed_tei_max_in_flight_inputs: Some(cfg.embed_tei_max_in_flight_inputs),
            embed_tei_retry_backoff_ms: Some(cfg.embed_tei_retry_backoff_ms),
            embed_tei_cooldown_after_failures: Some(cfg.embed_tei_cooldown_after_failures),
            embed_tei_cooldown_secs: Some(cfg.embed_tei_cooldown_secs),
            embed_tei_interactive_reserved_requests: Some(
                cfg.embed_tei_interactive_reserved_requests,
            ),
            embed_tei_background_max_concurrent_requests: Some(
                cfg.embed_tei_background_max_concurrent_requests,
            ),
            embed_tei_maintenance_max_concurrent_requests: Some(
                cfg.embed_tei_maintenance_max_concurrent_requests,
            ),
            embed_tei_query_instruction_enabled: Some(cfg.embed_tei_query_instruction_enabled),
            embed_cache_enabled: Some(cfg.embed_cache_enabled),
            embed_cache_max_entries: Some(cfg.embed_cache_max_entries),
            embed_pool_max_inputs: Some(cfg.embed_pool_max_inputs),
            document_batch_size: Some(cfg.document_batch_size),
            document_status_batch_size: Some(cfg.document_status_batch_size),
            embed_tei_max_batch_tokens: Some(cfg.embed_tei_max_batch_tokens),
            embed_scheduler_enabled: Some(cfg.embed_scheduler_enabled),
            vector_upsert_embed_overlap: Some(cfg.vector_upsert_embed_overlap),
            embed_prepared_byte_budget: Some(cfg.embed_prepared_byte_budget),
            embed_prep_concurrency: Some(cfg.embed_prep_concurrency),
            embed_prep_max_in_flight_bytes: Some(cfg.embed_prep_max_in_flight_bytes),
            embed_scheduler_flush_ms: Some(cfg.embed_scheduler_flush_ms),
            chunking_markdown_max_chars: Some(cfg.chunking_markdown_max_chars),
            chunking_markdown_min_chars: Some(cfg.chunking_markdown_min_chars),
            chunking_overlap_chars: Some(cfg.chunking_overlap_chars),
            embed_max_chunks_per_doc: cfg.embed_max_chunks_per_doc,
            embed_max_source_chunks_per_doc: cfg.embed_max_source_chunks_per_doc,
            embed_dedupe_exact_chunks: Some(cfg.embed_dedupe_exact_chunks),
            openai_embed_model: Some(cfg.openai_embed_model.clone()),
            openai_embed_max_client_batch_size: Some(cfg.openai_embed_max_client_batch_size),
            openai_embed_max_concurrent: Some(cfg.openai_embed_max_concurrent),
            openai_embed_max_in_flight_inputs: Some(cfg.openai_embed_max_in_flight_inputs),
            openai_embed_pool_max_inputs: Some(cfg.openai_embed_pool_max_inputs),
            unified_worker_concurrency: Some(cfg.unified_worker_concurrency),
            source_job_concurrency_limit: Some(cfg.source_job_concurrency_limit),
            embed_doc_timeout_secs: Some(cfg.embed_doc_timeout_secs),
            process_fallback_fields,
            ..Self::default()
        }
    }
}

fn header_contains_credential(header: &str) -> bool {
    let name = header.split_once(':').map_or(header, |(name, _)| name);
    let normalized = name.trim().to_ascii_lowercase().replace('_', "-");
    normalized == "authorization"
        || normalized == "proxy-authorization"
        || normalized == "cookie"
        || normalized == "set-cookie"
        || normalized.contains("api-key")
        || normalized.contains("apikey")
        || normalized.contains("token")
        || normalized.contains("secret")
}

fn llm_backend_snapshot(kind: axon_core::llm::LlmBackendKind) -> String {
    match kind {
        axon_core::llm::LlmBackendKind::GeminiHeadless => "gemini-headless".to_string(),
        axon_core::llm::LlmBackendKind::OpenAiCompat => "openai-compat".to_string(),
        axon_core::llm::LlmBackendKind::CodexAppServer => "codex-app-server".to_string(),
    }
}

pub fn config_snapshot_json(cfg: &Config) -> Result<String, serde_json::Error> {
    serde_json::to_string(&ConfigSnapshotEnvelope {
        version: 2,
        config: ConfigSnapshot::from_config(cfg).map_err(serde_json_error)?,
        prompt: None,
    })
}

pub fn extract_config_json(
    cfg: &Config,
    prompt: Option<String>,
) -> Result<String, serde_json::Error> {
    let mut effective = cfg.clone();
    if let Some(prompt) = &prompt {
        effective.query = Some(prompt.clone());
    }
    serde_json::to_string(&ConfigSnapshotEnvelope {
        version: 2,
        config: ConfigSnapshot::from_config(&effective).map_err(serde_json_error)?,
        prompt,
    })
}

pub fn apply_config_snapshot(
    process_cfg: &Config,
    config_json: &str,
) -> Result<Config, Box<dyn std::error::Error + Send + Sync>> {
    apply_config_snapshot_for_container(process_cfg, config_json, running_in_container())
}

pub fn apply_config_snapshot_for_container(
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
    envelope.config.apply_to(&mut cfg, exact_options)?;
    if let Some(prompt) = envelope.prompt {
        cfg.query = Some(prompt);
    }
    normalize_container_output_dir(process_cfg, &mut cfg, in_container);
    Ok(cfg)
}

fn decode_config_envelope(
    config_json: &str,
) -> Result<ConfigSnapshotEnvelope, Box<dyn std::error::Error + Send + Sync>> {
    let value: serde_json::Value = serde_json::from_str(config_json)?;
    if value.get("config").is_some() || value.get("prompt").is_some() {
        return Ok(serde_json::from_value(value)?);
    }

    let snapshot = serde_json::from_value(value)?;
    Ok(ConfigSnapshotEnvelope {
        version: 0,
        config: snapshot,
        prompt: None,
    })
}

#[cfg(test)]
#[path = "config_snapshot_tests.rs"]
mod tests;
