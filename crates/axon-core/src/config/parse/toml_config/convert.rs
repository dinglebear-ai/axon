//! Folds the [`super::raw::RawTomlConfig`] wire shape onto the legacy flat
//! [`super::TomlConfig`] so every existing consumer (`tuning.rs`,
//! `config_literal.rs`, `build_config.rs`) keeps reading the same field
//! paths it always has. This is the only place that knows both shapes.

use super::raw::RawTomlConfig;
use super::*;

/// Top-level old section names that no longer exist in the 20-section
/// contract shape, paired with where their knobs now live. Used to produce a
/// clear deprecation diagnostic instead of a bare serde "unknown field"
/// error when someone's `config.toml` still uses the pre-contract layout.
const DEPRECATED_SECTIONS: &[(&str, &str)] = &[
    ("build", "[server].allow-fallback-web-assets"),
    (
        "services",
        "service URLs now live only in .env (QDRANT_URL/TEI_URL/AXON_CHROME_REMOTE_URL)",
    ),
    ("llm", "[providers.llm]"),
    ("tei", "[providers.embedding]"),
    ("embed", "[providers.embedding]"),
    ("qdrant", "[providers.vector]"),
    ("chunking", "[pipeline].chunking"),
    ("code-search", "[sources].code-search"),
    ("code_search", "[sources].code-search"),
    ("endpoints", "[pipeline].endpoints"),
    ("mcp", "[server].mcp"),
    ("workers", "[pipeline] / [jobs] / [crawl]"),
    ("chrome", "[providers.render]"),
    ("scrape", "[crawl] / [providers.fetch]"),
    ("verticals", "[crawl].verticals"),
    ("antibot", "[crawl].antibot"),
    ("payload", "[providers.vector].structured-data-max-bytes"),
    (
        "search",
        "[server].default-collection / [providers.vector] / [retrieval] / [providers.search]",
    ),
];

/// Scan the raw TOML for deprecated top-level section names before doing a
/// typed parse, so the error names every offending section and its new home
/// in one message instead of surfacing a generic "unknown field" per key.
pub(super) fn deprecated_section_error(contents: &str) -> Option<String> {
    let value: toml::Value = toml::from_str(contents).ok()?;
    let table = value.as_table()?;
    let mut hits: Vec<String> = DEPRECATED_SECTIONS
        .iter()
        .filter(|(name, _)| table.contains_key(*name))
        .map(|(name, new_home)| format!("  [{name}] -> {new_home}"))
        .collect();
    if table
        .get("ask")
        .and_then(toml::Value::as_table)
        .is_some_and(|ask| ask.contains_key("backend"))
    {
        hits.push("  [ask].backend -> AXON_LLM_BACKEND or [providers.llm].backend".to_string());
    }
    if table
        .get("providers")
        .and_then(toml::Value::as_table)
        .and_then(|providers| providers.get("vector"))
        .and_then(toml::Value::as_table)
        .is_some_and(|vector| vector.contains_key("hnsw-ef-legacy"))
    {
        hits.push("  [providers.vector].hnsw-ef-legacy -> [providers.vector].hnsw-ef".to_string());
    }
    if hits.is_empty() {
        return None;
    }
    hits.sort();
    Some(format!(
        "config.toml uses deprecated section name(s) from before the config contract rewrite:\n{}\n\
         See docs/pipeline-unification/configuration/config-contract.md for the current 20-section shape.",
        hits.join("\n")
    ))
}

pub(super) fn into_legacy(raw: RawTomlConfig) -> TomlConfig {
    let mut legacy = TomlConfig::default();

    legacy.build.allow_fallback_web_assets = raw.server.allow_fallback_web_assets;
    legacy.mcp.task_result_wait_timeout_secs = raw.server.mcp.task_result_wait_timeout_secs;
    legacy.mcp.embed.max_local_bytes = raw.server.mcp.embed.max_local_bytes;
    legacy.mcp.embed.max_local_depth = raw.server.mcp.embed.max_local_depth;
    legacy.mcp.embed.max_local_entries = raw.server.mcp.embed.max_local_entries;

    legacy.code_search.freshness_ttl_secs = raw.sources.code_search.freshness_ttl_secs;
    legacy.code_search.reindex_timeout_secs = raw.sources.code_search.reindex_timeout_secs;
    legacy.code_search.max_file_bytes = raw.sources.code_search.max_file_bytes;
    legacy.code_search.changed_file_batch_size = raw.sources.code_search.changed_file_batch_size;

    apply_pipeline(&mut legacy, &raw);
    apply_jobs(&mut legacy, &raw);
    apply_providers(&mut legacy, &raw);
    apply_retrieval(&mut legacy, &raw);
    apply_ask(&mut legacy, &raw);
    apply_crawl(&mut legacy, &raw);

    legacy.watch.tick_secs = raw.watch.tick_secs;
    legacy.watch.lease_secs = raw.watch.lease_secs;

    // memory/graph/artifacts/prune/observability/security are parsed
    // (validated, unknown-field-checked) but have no legacy runtime field to
    // land on yet — see raw.rs doc comment.
    legacy
}

fn apply_pipeline(legacy: &mut TomlConfig, raw: &RawTomlConfig) {
    let p = &raw.pipeline;
    legacy.workers.ingest_lanes = p.ingest_lanes;
    legacy.workers.embed_lanes = p.embed_lanes;
    legacy.workers.unified_worker_concurrency = p.unified_worker_concurrency;
    legacy.workers.crawl_job_concurrency_limit = p.crawl_job_concurrency_limit;
    legacy.workers.embed_doc_timeout_secs = p.embed_doc_timeout_secs;
    legacy.workers.queue_summary_secs = p.queue_summary_secs;
    legacy.workers.qdrant_point_buffer = p.qdrant_point_buffer;
    legacy.workers.job_wait_timeout_secs = p.job_wait_timeout_secs;
    legacy.workers.max_pending_crawl_jobs = p.max_pending_crawl_jobs;
    legacy.workers.max_pending_embed_jobs = p.max_pending_embed_jobs;
    legacy.workers.max_pending_extract_jobs = p.max_pending_extract_jobs;
    legacy.workers.max_pending_ingest_jobs = p.max_pending_ingest_jobs;
    legacy.chunking.markdown_min_chars = p.chunking.markdown_min_chars;
    legacy.chunking.markdown_max_chars = p.chunking.markdown_max_chars;
    legacy.chunking.overlap_chars = p.chunking.overlap_chars;
    legacy.endpoints.bundle_concurrency = p.endpoints.bundle_concurrency;
    legacy.endpoints.chrome_concurrency = p.endpoints.chrome_concurrency;
    legacy.endpoints.verify_concurrency = p.endpoints.verify_concurrency;
    legacy.endpoints.probe_concurrency = p.endpoints.probe_concurrency;
}

fn apply_jobs(legacy: &mut TomlConfig, raw: &RawTomlConfig) {
    let j = &raw.jobs;
    legacy.workers.watchdog_stale_timeout_secs = j.stale_after_secs;
    legacy.workers.watchdog_confirm_secs = j.stale_grace_secs;
    legacy.workers.watchdog_sweep_secs = j.watchdog_sweep_secs;
    legacy.workers.worker_starvation_secs = j.worker_starvation_secs;
    legacy.workers.crawl_job_timeout_secs = j.crawl_job_timeout_secs;
    legacy.workers.max_job_attempts = j.max_job_attempts;
    legacy.workers.jobs_retention_terminal_days = j.terminal_retention_days.map(i64::from);
    legacy.workers.jobs_retention_event_days = j.event_retention_days.map(i64::from);
    legacy.workers.jobs_retention_failed_event_days = j.failed_event_retention_days.map(i64::from);
    legacy.workers.jobs_retention_provider_health_days =
        j.provider_health_retention_days.map(i64::from);
    legacy.workers.jobs_retention_artifact_days = j.artifact_retention_days.map(i64::from);
    legacy.workers.jobs_retention_sweep_secs = j.retention_sweep_secs;
    legacy.workers.jobs_interactive_starvation_slo_secs = j.interactive_starvation_slo_secs;
}

fn apply_providers(legacy: &mut TomlConfig, raw: &RawTomlConfig) {
    let e = &raw.providers.embedding;
    legacy.tei.max_retries = e.max_retries;
    legacy.tei.request_timeout_ms = e.request_timeout_ms;
    legacy.tei.max_client_batch_size = e.batch_size;
    legacy.embed.tei_max_concurrent = e.max_concurrent_requests;
    legacy.embed.tei_max_in_flight_inputs = e.max_in_flight_inputs;
    // Previously parsed (round-tripped) but never copied onto the legacy
    // shape, so nothing downstream ever read them — see config-contract.md's
    // "Providers: Embedding" section and axon_rust-ldozg.
    legacy.embed.tei_retry_backoff_ms = e.retry_backoff_ms;
    legacy.embed.tei_cooldown_after_failures = e.cooldown_after_failures;
    legacy.embed.tei_cooldown_secs = e.cooldown_secs;
    legacy.embed.tei_interactive_reserved_requests = e.interactive_reserved_requests;
    legacy.embed.tei_background_max_concurrent_requests = e.background_max_concurrent_requests;
    legacy.embed.tei_maintenance_max_concurrent_requests = e.maintenance_max_concurrent_requests;
    legacy.embed.tei_query_instruction_enabled = e.query_instruction_enabled;
    legacy.embed.pool_max_inputs = e.pool_max_inputs;
    legacy.embed.prep_concurrency = e.prep_concurrency;
    legacy.embed.max_chunks_per_doc = e.max_chunks_per_doc;
    legacy.embed.max_source_chunks_per_doc = e.max_source_chunks_per_doc;
    legacy.embed.dedupe_exact_chunks = e.dedupe_exact_chunks;
    legacy.embed.openai_model = e.openai_model.clone();
    legacy.embed.openai_max_client_batch_size = e.openai_max_client_batch_size;
    legacy.embed.openai_max_concurrent = e.openai_max_concurrent;
    legacy.embed.openai_max_in_flight_inputs = e.openai_max_in_flight_inputs;
    legacy.embed.openai_pool_max_inputs = e.openai_pool_max_inputs;

    let v = &raw.providers.vector;
    legacy.search.hybrid_enabled = v.hybrid_enabled;
    legacy.search.hnsw_ef = v.hnsw_ef;
    legacy.payload.structured_data_max_bytes = v.structured_data_max_bytes;
    legacy.qdrant.upsert_batch_size = v.upsert_batch_points;
    legacy.qdrant.upsert_parallelism = v.write_concurrency;
    legacy.qdrant.bulk_load = v.bulk_load;
    legacy.qdrant.bulk_indexing_threshold_kb = v.bulk_indexing_threshold_kb;
    legacy.qdrant.indexing_threshold_kb = v.indexing_threshold_kb;
    legacy.qdrant.hnsw_m = v.hnsw_m;
    legacy.qdrant.hnsw_ef_construct = v.hnsw_ef_construct;
    legacy.qdrant.payload_index_profile = v.payload_index_profile.clone();
    legacy.qdrant.payload_index_parallelism = v.payload_index_parallelism;
    legacy.qdrant.hnsw_on_disk = v.hnsw_on_disk;
    legacy.qdrant.quantization_always_ram = v.quantization_always_ram;

    let l = &raw.providers.llm;
    legacy.llm.backend = l.backend.clone();
    legacy.llm.completion_concurrency = l.completion_concurrency;
    legacy.llm.completion_timeout_secs = l.completion_timeout_secs;
    legacy.llm.codex_pool_idle_ttl_secs = l.codex_pool_idle_ttl_secs;
    legacy.llm.synthesis_high_context = l.high_context;
    legacy.llm.synthesis_gemini_model = l.synthesis_gemini_model.clone();
    legacy.llm.chat_gemini_model = l.chat_gemini_model.clone();
    legacy.llm.synthesis_openai_model = l.synthesis_openai_model.clone();
    legacy.llm.chat_openai_model = l.chat_openai_model.clone();

    legacy.search.research_full_content = raw.providers.search.research_full_content;

    let f = &raw.providers.fetch;
    legacy.scrape.request_timeout_ms = f.request_timeout_ms;
    legacy.scrape.fetch_retries = f.retries;
    legacy.scrape.retry_backoff_ms = f.retry_backoff_ms;
    legacy.scrape.delay_ms = f.delay_ms;
    legacy.scrape.batch_timeout_secs = f.batch_timeout_secs;

    let r = &raw.providers.render;
    legacy.chrome.user_agent = r.user_agent.clone();
    legacy.chrome.bypass_csp = r.bypass_csp;
    legacy.chrome.accept_invalid_certs = r.accept_invalid_certs;
    legacy.chrome.network_idle_timeout_secs = r.network_idle_timeout_secs;
    legacy.chrome.bootstrap_timeout_ms = r.bootstrap_timeout_ms;
    legacy.chrome.bootstrap_retries = r.bootstrap_retries;
    legacy.chrome.remote_local_policy = r.remote_local_policy;

    legacy.search.collection = raw.server.default_collection.clone();
}

fn apply_retrieval(legacy: &mut TomlConfig, raw: &RawTomlConfig) {
    legacy.search.hybrid_candidates = raw.retrieval.hybrid_candidates;
    legacy.search.ask_hybrid_candidates = raw.retrieval.ask_hybrid_candidates;
}

fn apply_ask(legacy: &mut TomlConfig, raw: &RawTomlConfig) {
    let a = &raw.ask;
    legacy.ask.max_context_chars = a.max_context_chars;
    legacy.ask.chunk_limit = a.chunk_limit;
    legacy.ask.candidate_limit = a.candidate_limit;
    legacy.ask.full_docs = a.full_docs;
    legacy.ask.backfill_chunks = a.backfill_chunks;
    legacy.ask.doc_fetch_concurrency = a.doc_fetch_concurrency;
    legacy.ask.doc_chunk_limit = a.doc_chunk_limit;
    legacy.ask.min_relevance_score = a.min_relevance_score;
    legacy.ask.authoritative_domains = a.authoritative_domains.clone();
    legacy.ask.authoritative_boost = a.authoritative_boost;
    legacy.ask.min_citations_nontrivial = a.min_citations_nontrivial;
    legacy.ask.cache.enabled = a.cache.enabled;
    legacy.ask.cache.max_capacity_bytes = a.cache.max_capacity_bytes;
    legacy.ask.cache.ttl_secs = a.cache.ttl_secs;
    legacy.ask.adaptive.fulldoc_skip_enabled = a.adaptive.fulldoc_skip_enabled;
    legacy.ask.adaptive.fulldoc_skip_min_urls = a.adaptive.fulldoc_skip_min_urls;
    legacy.ask.adaptive.fulldoc_skip_min_chars = a.adaptive.fulldoc_skip_min_chars;
    legacy.ask.adaptive.fulldoc_skip_score_delta = a.adaptive.fulldoc_skip_score_delta;
}

fn apply_crawl(legacy: &mut TomlConfig, raw: &RawTomlConfig) {
    let c = &raw.crawl;
    legacy.scrape.respect_robots = c.respect_robots;
    legacy.scrape.discover_sitemaps = c.discover_sitemaps;
    legacy.scrape.min_markdown_chars = c.min_markdown_chars;
    legacy.scrape.drop_thin_markdown = c.drop_thin_markdown;
    legacy.scrape.crawl_memory_abort_percent = c.memory_abort_percent;
    legacy.scrape.sitemap_since_days = c.sitemap_since_days;
    legacy.scrape.max_sitemaps = c.max_sitemaps;
    legacy.scrape.discover_llms_txt = c.discover_llms_txt;
    legacy.scrape.max_llms_txt_urls = c.max_llms_txt_urls;
    legacy.scrape.auto_switch_thin_ratio = c.auto_switch_thin_ratio;
    legacy.scrape.auto_switch_min_pages = c.auto_switch_min_pages;
    legacy.scrape.url_whitelist = c.url_whitelist.clone();
    legacy.scrape.allow_unbounded_broad_crawl = c.allow_unbounded_broad_crawl;
    legacy.scrape.max_page_bytes = c.max_page_bytes;
    legacy.scrape.redirect_policy_strict = c.redirect_policy_strict;
    legacy.scrape.ladder_strategy1_threshold = c.ladder_strategy1_threshold;
    legacy.scrape.ladder_strategy2_threshold = c.ladder_strategy2_threshold;
    legacy.scrape.ladder_body_multiplier = c.ladder_body_multiplier;
    legacy.workers.concurrency_limit = c.concurrency_limit;
    legacy.workers.crawl_concurrency_limit = c.crawl_concurrency_limit;
    legacy.workers.backfill_concurrency_limit = c.backfill_concurrency_limit;
    legacy.workers.adaptive_concurrency.enabled = c.adaptive_concurrency.enabled;
    legacy.workers.adaptive_concurrency.min = c.adaptive_concurrency.min;
    legacy.workers.adaptive_concurrency.max = c.adaptive_concurrency.max;
    legacy.verticals.enabled = c.verticals.enabled;
    legacy.verticals.auto_dispatch_skip = c.verticals.auto_dispatch_skip.clone();
    legacy.verticals.cache_ttl_secs = c.verticals.cache_ttl_secs.clone();
    legacy.antibot.cookie_warmup = c.antibot.cookie_warmup;
    legacy.antibot.max_body_scan_bytes = c.antibot.max_body_scan_bytes;
}
