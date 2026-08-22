//! Folds the [`super::raw::RawTomlConfig`] wire shape onto the flat runtime
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

/// Individual keys that used to live inside a still-valid section but have
/// since been removed or renamed. Unlike `DEPRECATED_SECTIONS` above (whole
/// section gone), the *section* here still parses fine — only these specific
/// keys are gone — so a bare `deny_unknown_fields` "unknown field" error
/// would give no hint about where the knob went, or that it never did
/// anything at all. `(section, key, new_home)`.
const DEPRECATED_SECTION_KEYS: &[(&str, &str, &str)] = &[
    (
        "pipeline",
        "ingest-lanes",
        "removed — zero runtime consumers ever read this knob",
    ),
    (
        "pipeline",
        "embed-lanes",
        "removed — zero runtime consumers ever read this knob",
    ),
    (
        "pipeline",
        "max-pending-crawl-jobs",
        "removed — nothing ever enforced this queue cap",
    ),
    (
        "pipeline",
        "max-pending-embed-jobs",
        "removed — nothing ever enforced this queue cap",
    ),
    (
        "pipeline",
        "max-pending-extract-jobs",
        "removed — nothing ever enforced this queue cap",
    ),
    (
        "pipeline",
        "max-pending-ingest-jobs",
        "removed — nothing ever enforced this queue cap",
    ),
    (
        "pipeline",
        "crawl-job-concurrency-limit",
        "renamed to [pipeline].max-active-source-jobs (now gates every source job kind, not just crawls)",
    ),
    (
        "jobs",
        "crawl-job-timeout-secs",
        "removed — no timeout was ever enforced against a running job",
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
    for (section, key, new_home) in DEPRECATED_SECTION_KEYS {
        let present = table
            .get(*section)
            .and_then(toml::Value::as_table)
            .is_some_and(|sect| sect.contains_key(*key));
        if present {
            hits.push(format!("  [{section}].{key} -> {new_home}"));
        }
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

pub(super) fn flatten(raw: RawTomlConfig) -> TomlConfig {
    let mut flat = TomlConfig::default();

    flat.build.allow_fallback_web_assets = raw.server.allow_fallback_web_assets;
    flat.mcp.task_result_wait_timeout_secs = raw.server.mcp.task_result_wait_timeout_secs;
    flat.mcp.embed.max_local_bytes = raw.server.mcp.embed.max_local_bytes;
    flat.mcp.embed.max_local_depth = raw.server.mcp.embed.max_local_depth;
    flat.mcp.embed.max_local_entries = raw.server.mcp.embed.max_local_entries;

    flat.code_search.freshness_ttl_secs = raw.sources.code_search.freshness_ttl_secs;
    flat.code_search.reindex_timeout_secs = raw.sources.code_search.reindex_timeout_secs;
    flat.code_search.max_file_bytes = raw.sources.code_search.max_file_bytes;
    flat.code_search.changed_file_batch_size = raw.sources.code_search.changed_file_batch_size;

    apply_pipeline(&mut flat, &raw);
    apply_jobs(&mut flat, &raw);
    apply_providers(&mut flat, &raw);
    apply_retrieval(&mut flat, &raw);
    apply_ask(&mut flat, &raw);
    apply_crawl(&mut flat, &raw);

    flat.watch.tick_secs = raw.watch.tick_secs;
    flat.watch.lease_secs = raw.watch.lease_secs;
    flat.security.allow_tool_execution = raw.security.allow_tool_execution;

    // memory/graph/artifacts/prune/observability are parsed
    // (validated, unknown-field-checked) but have no flat runtime field to
    // land on yet — see raw.rs doc comment.
    flat
}

fn apply_pipeline(flat: &mut TomlConfig, raw: &RawTomlConfig) {
    let p = &raw.pipeline;
    flat.workers.source_job_concurrency_limit = p.max_active_source_jobs;
    flat.workers.unified_worker_concurrency = p.unified_worker_concurrency;
    flat.workers.embed_doc_timeout_secs = p.embed_doc_timeout_secs;
    flat.workers.queue_summary_secs = p.queue_summary_secs;
    flat.workers.qdrant_point_buffer = p.qdrant_point_buffer;
    flat.workers.job_wait_timeout_secs = p.job_wait_timeout_secs;
    flat.chunking.markdown_min_chars = p.chunking.markdown_min_chars;
    flat.chunking.markdown_max_chars = p.chunking.markdown_max_chars;
    flat.chunking.overlap_chars = p.chunking.overlap_chars;
    flat.endpoints.bundle_concurrency = p.endpoints.bundle_concurrency;
    flat.endpoints.chrome_concurrency = p.endpoints.chrome_concurrency;
    flat.endpoints.verify_concurrency = p.endpoints.verify_concurrency;
    flat.endpoints.probe_concurrency = p.endpoints.probe_concurrency;
}

fn apply_jobs(flat: &mut TomlConfig, raw: &RawTomlConfig) {
    let j = &raw.jobs;
    flat.workers.watchdog_stale_timeout_secs = j.stale_after_secs;
    flat.workers.watchdog_confirm_secs = j.stale_grace_secs;
    flat.workers.watchdog_sweep_secs = j.watchdog_sweep_secs;
    flat.workers.worker_starvation_secs = j.worker_starvation_secs;
    flat.workers.max_job_attempts = j.max_job_attempts;
    flat.workers.jobs_retention_terminal_days = j.terminal_retention_days.map(i64::from);
    flat.workers.jobs_retention_event_days = j.event_retention_days.map(i64::from);
    flat.workers.jobs_retention_failed_event_days = j.failed_event_retention_days.map(i64::from);
    flat.workers.jobs_retention_provider_health_days =
        j.provider_health_retention_days.map(i64::from);
    flat.workers.jobs_retention_artifact_days = j.artifact_retention_days.map(i64::from);
    flat.workers.jobs_retention_sweep_secs = j.retention_sweep_secs;
    flat.workers.jobs_interactive_starvation_slo_secs = j.interactive_starvation_slo_secs;
    flat.workers.jobs_default_priority = j.default_priority.clone();
    flat.workers.jobs_auto_worker = j.auto_worker;
    flat.workers.jobs_worker_idle_exit_secs = j.worker_idle_exit_secs;
}

fn apply_providers(flat: &mut TomlConfig, raw: &RawTomlConfig) {
    let e = &raw.providers.embedding;
    flat.tei.max_retries = e.max_retries;
    flat.tei.request_timeout_ms = e.request_timeout_ms;
    flat.tei.max_client_batch_size = e.batch_size;
    flat.embed.tei_max_concurrent = e.max_concurrent_requests;
    flat.embed.tei_max_in_flight_inputs = e.max_in_flight_inputs;
    // Previously parsed (round-tripped) but never copied onto the flat runtime
    // shape, so nothing downstream ever read them — see config-contract.md's
    // "Providers: Embedding" section and axon_rust-ldozg.
    flat.embed.tei_retry_backoff_ms = e.retry_backoff_ms;
    flat.embed.tei_cooldown_after_failures = e.cooldown_after_failures;
    flat.embed.tei_cooldown_secs = e.cooldown_secs;
    flat.embed.tei_interactive_reserved_requests = e.interactive_reserved_requests;
    flat.embed.tei_background_max_concurrent_requests = e.background_max_concurrent_requests;
    flat.embed.tei_maintenance_max_concurrent_requests = e.maintenance_max_concurrent_requests;
    flat.embed.tei_query_instruction_enabled = e.query_instruction_enabled;
    flat.embed.cache_enabled = e.cache_enabled;
    flat.embed.cache_max_entries = e.cache_max_entries;
    flat.embed.pool_max_inputs = e.pool_max_inputs;
    flat.embed.prep_concurrency = e.prep_concurrency;
    flat.embed.max_chunks_per_doc = e.max_chunks_per_doc;
    flat.embed.max_source_chunks_per_doc = e.max_source_chunks_per_doc;
    flat.embed.dedupe_exact_chunks = e.dedupe_exact_chunks;
    flat.embed.openai_model = e.openai_model.clone();
    flat.embed.openai_max_client_batch_size = e.openai_max_client_batch_size;
    flat.embed.openai_max_concurrent = e.openai_max_concurrent;
    flat.embed.openai_max_in_flight_inputs = e.openai_max_in_flight_inputs;
    flat.embed.openai_pool_max_inputs = e.openai_pool_max_inputs;

    let v = &raw.providers.vector;
    flat.search.hybrid_enabled = v.hybrid_enabled;
    flat.search.hnsw_ef = v.hnsw_ef;
    flat.payload.structured_data_max_bytes = v.structured_data_max_bytes;
    flat.qdrant.upsert_batch_size = v.upsert_batch_points;
    flat.qdrant.upsert_parallelism = v.write_concurrency;
    flat.qdrant.bulk_load = v.bulk_load;
    flat.qdrant.bulk_indexing_threshold_kb = v.bulk_indexing_threshold_kb;
    flat.qdrant.indexing_threshold_kb = v.indexing_threshold_kb;
    flat.qdrant.hnsw_m = v.hnsw_m;
    flat.qdrant.hnsw_ef_construct = v.hnsw_ef_construct;
    flat.qdrant.payload_index_profile = v.payload_index_profile.clone();
    flat.qdrant.payload_index_parallelism = v.payload_index_parallelism;
    flat.qdrant.hnsw_on_disk = v.hnsw_on_disk;
    flat.qdrant.quantization_always_ram = v.quantization_always_ram;

    let l = &raw.providers.llm;
    flat.llm.backend = l.backend.clone();
    flat.llm.completion_concurrency = l.completion_concurrency;
    flat.llm.completion_timeout_secs = l.completion_timeout_secs;
    flat.llm.codex_pool_idle_ttl_secs = l.codex_pool_idle_ttl_secs;
    flat.llm.synthesis_high_context = l.high_context;
    flat.llm.synthesis_gemini_model = l.synthesis_gemini_model.clone();
    flat.llm.chat_gemini_model = l.chat_gemini_model.clone();
    flat.llm.synthesis_openai_model = l.synthesis_openai_model.clone();
    flat.llm.chat_openai_model = l.chat_openai_model.clone();

    flat.search.research_full_content = raw.providers.search.research_full_content;

    let f = &raw.providers.fetch;
    flat.scrape.fetch_concurrency = f.concurrency;
    flat.scrape.request_timeout_ms = f.request_timeout_ms;
    flat.scrape.fetch_retries = f.retries;
    flat.scrape.retry_backoff_ms = f.retry_backoff_ms;
    flat.scrape.delay_ms = f.delay_ms;
    flat.scrape.batch_timeout_secs = f.batch_timeout_secs;

    let r = &raw.providers.render;
    flat.chrome.max_concurrent_pages = r.max_concurrent_pages;
    flat.chrome.user_agent = r.user_agent.clone();
    flat.chrome.bypass_csp = r.bypass_csp;
    flat.chrome.accept_invalid_certs = r.accept_invalid_certs;
    flat.chrome.network_idle_timeout_secs = r.network_idle_timeout_secs;
    flat.chrome.bootstrap_timeout_ms = r.bootstrap_timeout_ms;
    flat.chrome.bootstrap_retries = r.bootstrap_retries;
    flat.chrome.remote_local_policy = r.remote_local_policy;

    flat.search.collection = raw.server.default_collection.clone();
}

fn apply_retrieval(flat: &mut TomlConfig, raw: &RawTomlConfig) {
    flat.search.hybrid_candidates = raw.retrieval.hybrid_candidates;
    flat.search.ask_hybrid_candidates = raw.retrieval.ask_hybrid_candidates;
}

fn apply_ask(flat: &mut TomlConfig, raw: &RawTomlConfig) {
    let a = &raw.ask;
    flat.ask.max_context_chars = a.max_context_chars;
    flat.ask.chunk_limit = a.chunk_limit;
    flat.ask.candidate_limit = a.candidate_limit;
    flat.ask.full_docs = a.full_docs;
    flat.ask.backfill_chunks = a.backfill_chunks;
    flat.ask.doc_fetch_concurrency = a.doc_fetch_concurrency;
    flat.ask.doc_chunk_limit = a.doc_chunk_limit;
    flat.ask.min_relevance_score = a.min_relevance_score;
    flat.ask.authoritative_domains = a.authoritative_domains.clone();
    flat.ask.authoritative_boost = a.authoritative_boost;
    flat.ask.min_citations_nontrivial = a.min_citations_nontrivial;
    flat.ask.cache.enabled = a.cache.enabled;
    flat.ask.cache.max_capacity_bytes = a.cache.max_capacity_bytes;
    flat.ask.cache.ttl_secs = a.cache.ttl_secs;
    flat.ask.adaptive.fulldoc_skip_enabled = a.adaptive.fulldoc_skip_enabled;
    flat.ask.adaptive.fulldoc_skip_min_urls = a.adaptive.fulldoc_skip_min_urls;
    flat.ask.adaptive.fulldoc_skip_min_chars = a.adaptive.fulldoc_skip_min_chars;
    flat.ask.adaptive.fulldoc_skip_score_delta = a.adaptive.fulldoc_skip_score_delta;
}

fn apply_crawl(flat: &mut TomlConfig, raw: &RawTomlConfig) {
    let c = &raw.crawl;
    flat.scrape.respect_robots = c.respect_robots;
    flat.scrape.discover_sitemaps = c.discover_sitemaps;
    flat.scrape.min_markdown_chars = c.min_markdown_chars;
    flat.scrape.drop_thin_markdown = c.drop_thin_markdown;
    flat.scrape.crawl_memory_abort_percent = c.memory_abort_percent;
    flat.scrape.sitemap_since_days = c.sitemap_since_days;
    flat.scrape.max_sitemaps = c.max_sitemaps;
    flat.scrape.discover_llms_txt = c.discover_llms_txt;
    flat.scrape.max_llms_txt_urls = c.max_llms_txt_urls;
    flat.scrape.auto_switch_thin_ratio = c.auto_switch_thin_ratio;
    flat.scrape.auto_switch_min_pages = c.auto_switch_min_pages;
    flat.scrape.url_whitelist = c.url_whitelist.clone();
    flat.scrape.allow_unbounded_broad_crawl = c.allow_unbounded_broad_crawl;
    flat.scrape.max_page_bytes = c.max_page_bytes;
    flat.scrape.redirect_policy_strict = c.redirect_policy_strict;
    flat.scrape.ladder_strategy1_threshold = c.ladder_strategy1_threshold;
    flat.scrape.ladder_strategy2_threshold = c.ladder_strategy2_threshold;
    flat.scrape.ladder_body_multiplier = c.ladder_body_multiplier;
    flat.workers.concurrency_limit = c.concurrency_limit;
    flat.workers.crawl_concurrency_limit = c.crawl_concurrency_limit;
    flat.workers.backfill_concurrency_limit = c.backfill_concurrency_limit;
    flat.workers.adaptive_concurrency.enabled = c.adaptive_concurrency.enabled;
    flat.workers.adaptive_concurrency.min = c.adaptive_concurrency.min;
    flat.workers.adaptive_concurrency.max = c.adaptive_concurrency.max;
    flat.verticals.enabled = c.verticals.enabled;
    flat.verticals.auto_dispatch_skip = c.verticals.auto_dispatch_skip.clone();
    flat.verticals.cache_ttl_secs = c.verticals.cache_ttl_secs.clone();
    flat.antibot.cookie_warmup = c.antibot.cookie_warmup;
    flat.antibot.max_body_scan_bytes = c.antibot.max_body_scan_bytes;
}
