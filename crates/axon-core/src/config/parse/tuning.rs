use super::helpers::{env_bool_opt, read_env};
use super::performance;
use super::toml_config::{TomlConfig, load_toml_config};
use crate::config::types::Config;
use crate::llm::{SynthesisModelProfile, SynthesisModelTier};

pub(super) fn apply_env_toml_tuning(cfg: &mut Config, toml: &TomlConfig) {
    // Computed into locals first: these defaults read the resolved LLM
    // backend/model off `cfg` (model-aware retrieval depth), which can't be
    // borrowed while assigning the field in the same expression.
    let max_context_chars = ask_max_context_chars(cfg, toml);
    let candidate_limit = ask_candidate_limit(cfg, toml);
    let chunk_limit = ask_chunk_limit(cfg, toml);
    cfg.ask_max_context_chars = max_context_chars;
    cfg.ask_candidate_limit = candidate_limit;
    cfg.ask_chunk_limit = chunk_limit;
    cfg.ask_full_docs = resolve_clamped_usize("AXON_ASK_FULL_DOCS", toml.ask.full_docs, 6, 1, 20);
    cfg.ask_full_docs_explicit =
        std::env::var_os("AXON_ASK_FULL_DOCS").is_some() || toml.ask.full_docs.is_some();
    cfg.ask_backfill_chunks = resolve_clamped_usize(
        "AXON_ASK_BACKFILL_CHUNKS",
        toml.ask.backfill_chunks,
        5,
        0,
        20,
    );
    cfg.ask_doc_fetch_concurrency = resolve_clamped_usize(
        "AXON_ASK_DOC_FETCH_CONCURRENCY",
        toml.ask.doc_fetch_concurrency,
        4,
        1,
        16,
    );
    cfg.ask_doc_chunk_limit = resolve_clamped_usize(
        "AXON_ASK_DOC_CHUNK_LIMIT",
        toml.ask.doc_chunk_limit,
        96,
        8,
        2000,
    );
    cfg.ask_min_relevance_score = ask_min_relevance_score(toml);
    cfg.ask_authoritative_boost = resolve_clamped_f64(
        "AXON_ASK_AUTHORITATIVE_BOOST",
        toml.ask.authoritative_boost,
        0.0,
        0.0,
        0.5,
    );
    cfg.ask_min_citations_nontrivial = resolve_clamped_usize(
        "AXON_ASK_MIN_CITATIONS_NONTRIVIAL",
        toml.ask.min_citations_nontrivial,
        2,
        1,
        5,
    );

    let ask_hybrid = ask_hybrid_candidates(cfg, toml);
    // Explicit high-context override: env wins over TOML; absent = `None`, which
    // leaves `high_context_synthesis_model` on its substring-heuristic fallback.
    cfg.synthesis_high_context =
        env_bool_opt("AXON_SYNTHESIS_HIGH_CONTEXT").or(toml.llm.synthesis_high_context);
    cfg.hybrid_search_enabled = hybrid_search_enabled(toml);
    cfg.hybrid_search_candidates = hybrid_search_candidates(toml);
    cfg.ask_hybrid_candidates = ask_hybrid;
    cfg.ask_cache_enabled = toml.ask.cache.enabled.unwrap_or(false);
    cfg.ask_cache_max_capacity_bytes = toml
        .ask
        .cache
        .max_capacity_bytes
        .unwrap_or(256 * 1024 * 1024);
    cfg.ask_cache_ttl_secs = toml.ask.cache.ttl_secs.unwrap_or(300).min(300);
    cfg.ask_fulldoc_skip_enabled = toml.ask.adaptive.fulldoc_skip_enabled.unwrap_or(false);
    cfg.ask_fulldoc_skip_min_urls = toml
        .ask
        .adaptive
        .fulldoc_skip_min_urls
        .map(|v| v.clamp(1, 50))
        .unwrap_or(3);
    cfg.ask_fulldoc_skip_min_chars = toml
        .ask
        .adaptive
        .fulldoc_skip_min_chars
        .map(|v| v.clamp(500, 200_000))
        .unwrap_or(4000);
    cfg.ask_fulldoc_skip_score_delta = toml
        .ask
        .adaptive
        .fulldoc_skip_score_delta
        .map(|v| v.clamp(0.0, 1.0))
        .unwrap_or(0.15);
    cfg.tei_max_retries = tei_max_retries(toml);
    cfg.scrape_batch_timeout_secs = resolve_clamped_u64(
        "AXON_SCRAPE_BATCH_TIMEOUT_SECS",
        toml.scrape.batch_timeout_secs,
        120,
        1,
        3600,
    );
    cfg.tei_request_timeout_ms = tei_request_timeout_ms(toml);
    cfg.tei_max_client_batch_size = tei_max_client_batch_size(toml);
    cfg.embed_tei_max_concurrent = resolve_clamped_usize(
        "AXON_TEI_MAX_CONCURRENT",
        toml.embed.tei_max_concurrent,
        8,
        1,
        64,
    );
    cfg.embed_tei_max_in_flight_inputs = resolve_clamped_usize(
        "AXON_TEI_MAX_IN_FLIGHT_INPUTS",
        toml.embed.tei_max_in_flight_inputs,
        320,
        1,
        4096,
    );
    cfg.embed_pool_max_inputs = resolve_clamped_usize(
        "AXON_EMBED_POOL_MAX_INPUTS",
        toml.embed.pool_max_inputs,
        512,
        64,
        65_536,
    );
    cfg.embed_prep_concurrency = resolve_clamped_usize(
        "AXON_EMBED_PREP_CONCURRENCY",
        toml.embed.prep_concurrency,
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8)
            .clamp(2, 16),
        1,
        64,
    );
    cfg.embed_max_chunks_per_doc = resolve_optional_usize(
        "AXON_EMBED_MAX_CHUNKS_PER_DOC",
        toml.embed.max_chunks_per_doc,
        1,
        100_000,
    );
    cfg.embed_max_source_chunks_per_doc = resolve_optional_usize(
        "AXON_EMBED_MAX_SOURCE_CHUNKS_PER_DOC",
        toml.embed.max_source_chunks_per_doc,
        1,
        100_000,
    );
    cfg.embed_dedupe_exact_chunks = env_bool_opt("AXON_EMBED_DEDUPE_EXACT_CHUNKS")
        .or(toml.embed.dedupe_exact_chunks)
        .unwrap_or(true);
    cfg.openai_embed_model = read_env("AXON_OPENAI_EMBEDDING_MODEL")
        .or_else(|| {
            toml.embed
                .openai_model
                .clone()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| read_env("VLLM_SERVED_MODEL_NAME"))
        .unwrap_or_else(|| "axon-qwen3-embedding".to_string());
    cfg.openai_embed_max_client_batch_size = resolve_clamped_usize(
        "AXON_OPENAI_EMBED_MAX_CLIENT_BATCH_SIZE",
        toml.embed.openai_max_client_batch_size,
        32,
        1,
        256,
    );
    cfg.openai_embed_max_concurrent = resolve_clamped_usize(
        "AXON_OPENAI_EMBED_MAX_CONCURRENT",
        toml.embed.openai_max_concurrent,
        32,
        1,
        64,
    );
    cfg.openai_embed_max_in_flight_inputs = resolve_clamped_usize(
        "AXON_OPENAI_EMBED_MAX_IN_FLIGHT_INPUTS",
        toml.embed.openai_max_in_flight_inputs,
        512,
        1,
        4096,
    );
    cfg.openai_embed_pool_max_inputs = resolve_clamped_usize(
        "AXON_OPENAI_EMBED_POOL_MAX_INPUTS",
        toml.embed.openai_pool_max_inputs,
        1024,
        64,
        65_536,
    );
    cfg.ingest_lanes = ingest_lanes(toml);
    cfg.embed_lanes = embed_lanes(toml);
    cfg.unified_worker_concurrency = unified_worker_concurrency(toml);
    cfg.crawl_job_concurrency_limit = crawl_job_concurrency_limit(toml);
    cfg.embed_doc_timeout_secs = embed_doc_timeout_secs(toml);
    cfg.queue_summary_secs = queue_summary_secs(toml);
    cfg.freshness_tick_secs = freshness_tick_secs(toml);
    cfg.freshness_lease_secs = freshness_lease_secs(toml);
    cfg.freshness_max_due_per_tick = freshness_max_due_per_tick(toml);
    cfg.freshness_max_concurrent_runs = freshness_max_concurrent_runs(toml);
    cfg.freshness_run_retention_days = freshness_run_retention_days(toml);
    cfg.qdrant_point_buffer = qdrant_point_buffer(toml);
    cfg.max_pending_crawl_jobs = max_pending(toml, "crawl");
    cfg.max_pending_embed_jobs = max_pending(toml, "embed");
    cfg.max_pending_extract_jobs = max_pending(toml, "extract");
    cfg.max_pending_ingest_jobs = max_pending(toml, "ingest");
    cfg.hnsw_ef_search =
        resolve_clamped_usize("AXON_HNSW_EF_SEARCH", toml.search.hnsw_ef, 128, 32, 512);
    cfg.hnsw_ef_search_legacy = resolve_clamped_usize(
        "AXON_HNSW_EF_SEARCH_LEGACY",
        toml.search.hnsw_ef_legacy,
        64,
        16,
        256,
    );
    cfg.job_wait_timeout_secs = job_wait_timeout_secs(toml);

    // ── Webclaw port (axon_rust-zehr) ──────────────────────────────────────
    cfg.enable_verticals = env_bool_opt("AXON_ENABLE_VERTICALS")
        .or(toml.verticals.enabled)
        .unwrap_or(true);
    cfg.auto_dispatch_skip = resolve_auto_dispatch_skip(toml);
    cfg.vertical_cache_ttl_secs = resolve_vertical_cache_ttl_secs(toml);
    cfg.structured_data_max_bytes = resolve_clamped_usize(
        "AXON_STRUCTURED_DATA_MAX_BYTES",
        toml.payload.structured_data_max_bytes,
        65_536,
        1_024,
        16_777_216,
    );
    cfg.ladder_word_threshold_strategy1 = resolve_clamped_usize(
        "AXON_LADDER_STRATEGY1_THRESHOLD",
        toml.scrape.ladder_strategy1_threshold,
        30,
        1,
        1_000,
    );
    cfg.ladder_word_threshold_strategy2 = resolve_clamped_usize(
        "AXON_LADDER_STRATEGY2_THRESHOLD",
        toml.scrape.ladder_strategy2_threshold,
        200,
        1,
        10_000,
    );
    cfg.ladder_body_multiplier = resolve_clamped_f64(
        "AXON_LADDER_BODY_MULTIPLIER",
        toml.scrape.ladder_body_multiplier,
        2.0,
        1.0,
        10.0,
    );
    cfg.antibot_cookie_warmup = env_bool_opt("AXON_CHALLENGE_WARMUP")
        .or(toml.antibot.cookie_warmup)
        .unwrap_or(true);
    cfg.antibot_max_body_scan_bytes = resolve_clamped_usize(
        "AXON_ANTIBOT_MAX_BODY_SCAN_BYTES",
        toml.antibot.max_body_scan_bytes,
        150_000,
        1_000,
        10_485_760,
    );
}

fn resolve_auto_dispatch_skip(toml: &TomlConfig) -> Vec<String> {
    if let Ok(v) = std::env::var("AXON_AUTO_DISPATCH_SKIP") {
        return v
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
    }
    toml.verticals
        .auto_dispatch_skip
        .clone()
        .unwrap_or_default()
}

fn resolve_vertical_cache_ttl_secs(toml: &TomlConfig) -> std::collections::HashMap<String, u64> {
    let mut map: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    map.insert("github".to_string(), 86_400);
    map.insert("reddit".to_string(), 3_600);
    map.insert("hn".to_string(), 21_600);
    // TOML overrides (whole-table replacement of defaults at the key level).
    if let Some(ref t) = toml.verticals.cache_ttl_secs {
        for (k, v) in t {
            map.insert(k.to_lowercase(), *v);
        }
    }
    // Env per-vertical override: AXON_VERTICAL_CACHE_TTL_<UPPER>=secs
    for (k, v) in std::env::vars() {
        if let Some(name) = k.strip_prefix("AXON_VERTICAL_CACHE_TTL_")
            && let Ok(secs) = v.parse::<u64>()
        {
            map.insert(name.to_lowercase(), secs);
        }
    }
    map
}

pub(crate) fn apply_default_minimal_tuning(cfg: &mut Config) {
    match load_toml_config() {
        Ok(toml) => apply_env_toml_tuning(cfg, &toml),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "axon: failed to load TOML tuning for default_minimal; using hardcoded defaults"
            );
        }
    }
}

fn load_toml_or_default() -> TomlConfig {
    // Unit tests must be hermetic: without this guard, a developer's real
    // `~/.axon/config.toml` (or any ambient config on the machine/CI runner)
    // silently overrides the tuning defaults every test in this workspace
    // assumes, since these `tuning::*()` functions read the filesystem
    // directly rather than going through the `Config` a test constructs.
    // An explicit `AXON_CONFIG_PATH` override still applies during tests —
    // this only skips the implicit fallback to `~/.axon/config.toml`.
    // `cfg!(test)` alone only covers axon-core's own test binary — other
    // crates' tests link axon-core as a normal dependency, so this also
    // gates on the `test-util` feature they enable as a dev-dependency
    // (same convention as `axon_core::http`/`config_impls`).
    #[cfg(any(test, feature = "test-util"))]
    if std::env::var("AXON_CONFIG_PATH").is_err() {
        return TomlConfig::default();
    }
    match load_toml_config() {
        Ok(toml) => toml,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "axon: failed to load TOML tuning for runtime resolver; using hardcoded defaults"
            );
            TomlConfig::default()
        }
    }
}

pub fn embed_tei_max_concurrent() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_TEI_MAX_CONCURRENT",
        toml.embed.tei_max_concurrent,
        8,
        1,
        64,
    )
}

pub fn embed_tei_max_in_flight_inputs() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_TEI_MAX_IN_FLIGHT_INPUTS",
        toml.embed.tei_max_in_flight_inputs,
        320,
        1,
        4096,
    )
}

pub fn embed_openai_max_client_batch_size() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_OPENAI_EMBED_MAX_CLIENT_BATCH_SIZE",
        toml.embed.openai_max_client_batch_size,
        32,
        1,
        256,
    )
}

pub fn embed_openai_max_concurrent() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_OPENAI_EMBED_MAX_CONCURRENT",
        toml.embed.openai_max_concurrent,
        32,
        1,
        64,
    )
}

pub fn embed_openai_max_in_flight_inputs() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_OPENAI_EMBED_MAX_IN_FLIGHT_INPUTS",
        toml.embed.openai_max_in_flight_inputs,
        512,
        1,
        4096,
    )
}

pub fn embed_pool_max_inputs(openai_compatible: bool) -> usize {
    let toml = load_toml_or_default();
    if openai_compatible {
        resolve_clamped_usize(
            "AXON_OPENAI_EMBED_POOL_MAX_INPUTS",
            toml.embed.openai_pool_max_inputs,
            1024,
            64,
            65_536,
        )
    } else {
        resolve_clamped_usize(
            "AXON_EMBED_POOL_MAX_INPUTS",
            toml.embed.pool_max_inputs,
            512,
            64,
            65_536,
        )
    }
}

pub fn embed_prep_concurrency() -> usize {
    let toml = load_toml_or_default();
    let default = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
        .clamp(2, 16);
    resolve_clamped_usize(
        "AXON_EMBED_PREP_CONCURRENCY",
        toml.embed.prep_concurrency,
        default,
        1,
        64,
    )
}

pub fn embed_max_chunks_per_doc() -> Option<usize> {
    let toml = load_toml_or_default();
    optional_env_or_toml_usize(
        "AXON_EMBED_MAX_CHUNKS_PER_DOC",
        toml.embed.max_chunks_per_doc,
        1,
        100_000,
    )
}

pub fn embed_max_source_chunks_per_doc() -> Option<usize> {
    let toml = load_toml_or_default();
    optional_env_or_toml_usize(
        "AXON_EMBED_MAX_SOURCE_CHUNKS_PER_DOC",
        toml.embed.max_source_chunks_per_doc,
        1,
        100_000,
    )
}

pub fn embed_dedupe_exact_chunks() -> bool {
    let toml = load_toml_or_default();
    env_bool_opt("AXON_EMBED_DEDUPE_EXACT_CHUNKS")
        .or(toml.embed.dedupe_exact_chunks)
        .unwrap_or(true)
}

pub fn chunking_markdown_max_chars() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_MARKDOWN_CHUNK_MAX_CHARS",
        toml.chunking.markdown_max_chars,
        2000,
        256,
        16_384,
    )
}

pub fn chunking_markdown_min_chars(max_chars: usize) -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_MARKDOWN_CHUNK_MIN_CHARS",
        toml.chunking.markdown_min_chars,
        500.min(max_chars),
        1,
        max_chars,
    )
}

pub fn chunking_overlap_chars(max_chars: usize) -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_CHUNK_OVERLAP_CHARS",
        toml.chunking.overlap_chars,
        200.min(max_chars.saturating_sub(1)),
        0,
        max_chars.saturating_sub(1),
    )
}

pub fn qdrant_upsert_batch_size() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_QDRANT_UPSERT_BATCH_SIZE",
        toml.qdrant.upsert_batch_size,
        1024,
        1,
        4096,
    )
}

pub fn qdrant_upsert_parallelism() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_QDRANT_UPSERT_PARALLELISM",
        toml.qdrant.upsert_parallelism,
        1,
        1,
        16,
    )
}

pub fn qdrant_bulk_load() -> bool {
    let toml = load_toml_or_default();
    env_bool_opt("AXON_QDRANT_BULK_LOAD")
        .or(toml.qdrant.bulk_load)
        .unwrap_or(false)
}

pub fn qdrant_bulk_indexing_threshold_kb() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_QDRANT_BULK_INDEXING_THRESHOLD_KB",
        toml.qdrant.bulk_indexing_threshold_kb,
        10_485_760,
        20_000,
        1_073_741_824,
    )
}

pub fn qdrant_indexing_threshold_kb() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_QDRANT_INDEXING_THRESHOLD_KB",
        toml.qdrant.indexing_threshold_kb,
        20_000,
        1,
        1_073_741_824,
    )
}

pub fn qdrant_hnsw_m() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize("AXON_QDRANT_HNSW_M", toml.qdrant.hnsw_m, 32, 8, 64)
}

pub fn qdrant_hnsw_ef_construct() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_QDRANT_HNSW_EF_CONSTRUCT",
        toml.qdrant.hnsw_ef_construct,
        256,
        64,
        512,
    )
}

pub fn qdrant_payload_index_profile() -> String {
    let toml = load_toml_or_default();
    std::env::var("AXON_QDRANT_PAYLOAD_INDEX_PROFILE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or(toml.qdrant.payload_index_profile)
        .unwrap_or_else(|| "full".to_string())
}

pub fn qdrant_payload_index_parallelism() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_QDRANT_PAYLOAD_INDEX_PARALLELISM",
        toml.qdrant.payload_index_parallelism,
        16,
        1,
        64,
    )
}

pub fn qdrant_hnsw_on_disk() -> bool {
    let toml = load_toml_or_default();
    env_bool_opt("AXON_QDRANT_HNSW_ON_DISK")
        .or(toml.qdrant.hnsw_on_disk)
        .unwrap_or(false)
}

pub fn qdrant_quantization_always_ram() -> bool {
    let toml = load_toml_or_default();
    env_bool_opt("AXON_QDRANT_QUANTIZATION_ALWAYS_RAM")
        .or(toml.qdrant.quantization_always_ram)
        .unwrap_or(true)
}

pub fn code_search_freshness_ttl_secs() -> u64 {
    let toml = load_toml_or_default();
    resolve_clamped_u64(
        "AXON_CODE_SEARCH_FRESHNESS_TTL_SECS",
        toml.code_search.freshness_ttl_secs,
        30,
        0,
        86_400,
    )
}

pub fn code_search_reindex_timeout_secs() -> u64 {
    let toml = load_toml_or_default();
    resolve_clamped_u64(
        "AXON_CODE_SEARCH_REINDEX_TIMEOUT_SECS",
        toml.code_search.reindex_timeout_secs,
        300,
        1,
        3_600,
    )
}

pub fn code_search_max_file_bytes() -> u64 {
    let toml = load_toml_or_default();
    resolve_clamped_u64(
        "AXON_CODE_SEARCH_MAX_FILE_BYTES",
        toml.code_search.max_file_bytes,
        10 * 1024 * 1024,
        1,
        512 * 1024 * 1024,
    )
}

pub fn code_search_changed_file_batch_size() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_CODE_SEARCH_CHANGED_FILE_BATCH_SIZE",
        toml.code_search.changed_file_batch_size,
        64,
        1,
        1000,
    )
}

pub fn watch_tick_secs() -> u64 {
    let toml = load_toml_or_default();
    resolve_clamped_u64("AXON_WATCH_TICK_SECS", toml.watch.tick_secs, 15, 1, 3600)
}

pub fn watch_lease_secs() -> i64 {
    let toml = load_toml_or_default();
    std::env::var("AXON_WATCH_LEASE_SECS")
        .ok()
        .and_then(|raw| raw.parse::<i64>().ok())
        .or(toml.watch.lease_secs)
        .unwrap_or(300)
        .max(1)
}

pub fn endpoints_bundle_concurrency() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_ENDPOINT_BUNDLE_CONCURRENCY",
        toml.endpoints.bundle_concurrency,
        8,
        1,
        64,
    )
}

pub fn endpoints_chrome_concurrency() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_ENDPOINT_CHROME_CONCURRENCY",
        toml.endpoints.chrome_concurrency,
        1,
        1,
        16,
    )
}

pub fn endpoints_verify_concurrency() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_ENDPOINT_VERIFY_CONCURRENCY",
        toml.endpoints.verify_concurrency,
        16,
        1,
        128,
    )
}

pub fn endpoints_probe_concurrency() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_ENDPOINT_PROBE_CONCURRENCY",
        toml.endpoints.probe_concurrency,
        4,
        1,
        128,
    )
}

pub fn mcp_embed_max_local_bytes() -> u64 {
    let toml = load_toml_or_default();
    resolve_clamped_u64(
        "AXON_MCP_EMBED_MAX_LOCAL_BYTES",
        toml.mcp.embed.max_local_bytes,
        10 * 1024 * 1024,
        1,
        u64::MAX,
    )
}

pub fn mcp_task_result_wait_timeout_secs() -> u64 {
    let toml = load_toml_or_default();
    resolve_clamped_u64(
        "AXON_TASK_RESULT_WAIT_TIMEOUT_SECS",
        toml.mcp.task_result_wait_timeout_secs,
        300,
        1,
        86_400,
    )
}

pub fn mcp_embed_max_local_depth() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_MCP_EMBED_MAX_LOCAL_DEPTH",
        toml.mcp.embed.max_local_depth,
        16,
        1,
        1024,
    )
}

pub fn mcp_embed_max_local_entries() -> usize {
    let toml = load_toml_or_default();
    resolve_clamped_usize(
        "AXON_MCP_EMBED_MAX_LOCAL_ENTRIES",
        toml.mcp.embed.max_local_entries,
        10_000,
        1,
        1_000_000,
    )
}

fn optional_env_or_toml_usize(
    env_key: &str,
    toml_value: Option<usize>,
    min: usize,
    max: usize,
) -> Option<usize> {
    if let Some(value) = performance::env_usize_opt(env_key, 0, max) {
        return (value != 0).then(|| value.clamp(min, max));
    }
    toml_value.and_then(|value| (value != 0).then(|| value.clamp(min, max)))
}

fn resolve_clamped_usize(
    env_key: &str,
    toml_value: Option<usize>,
    default: usize,
    min: usize,
    max: usize,
) -> usize {
    performance::env_usize_opt(env_key, min, max)
        .or_else(|| toml_value.map(|v| v.clamp(min, max)))
        .unwrap_or(default)
}

fn resolve_clamped_u64(
    env_key: &str,
    toml_value: Option<u64>,
    default: u64,
    min: u64,
    max: u64,
) -> u64 {
    performance::env_u64_opt(env_key, min, max)
        .or_else(|| toml_value.map(|v| v.clamp(min, max)))
        .unwrap_or(default)
}

fn resolve_clamped_f64(
    env_key: &str,
    toml_value: Option<f64>,
    default: f64,
    min: f64,
    max: f64,
) -> f64 {
    performance::env_f64_opt(env_key, min, max)
        .or_else(|| toml_value.map(|v| v.clamp(min, max)))
        .unwrap_or(default)
}

fn resolve_optional_usize(
    env_key: &str,
    toml_value: Option<usize>,
    min: usize,
    max: usize,
) -> Option<usize> {
    read_env(env_key)
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| match value {
            0 => None,
            value => Some(value.clamp(min, max)),
        })
        .unwrap_or_else(|| match toml_value {
            Some(0) | None => None,
            Some(value) => Some(value.clamp(min, max)),
        })
}

fn ask_max_context_chars(cfg: &Config, toml: &TomlConfig) -> usize {
    // Default scales with the configured model's context window (overridable by
    // env/TOML). An explicit `AXON_ASK_MAX_CONTEXT_CHARS` or `ask.max-context-chars`
    // still wins; this only changes the fallback when neither is set.
    resolve_clamped_usize(
        "AXON_ASK_MAX_CONTEXT_CHARS",
        toml.ask.max_context_chars,
        model_context_char_budget(cfg),
        20_000,
        1_000_000,
    )
}

fn ask_model_tier(cfg: &Config) -> SynthesisModelTier {
    SynthesisModelProfile::from_config(cfg).tier()
}

/// Context-char budget default per tier (≈ window-in-tokens as a char count,
/// i.e. retrieved context is roughly a quarter of the window).
fn model_context_char_budget(cfg: &Config) -> usize {
    match ask_model_tier(cfg) {
        SynthesisModelTier::Large => 1_000_000,
        SynthesisModelTier::Medium => 400_000,
        SynthesisModelTier::LocalGemma => 128_000,
        SynthesisModelTier::Small => 40_000,
    }
}

/// Max chunks injected into the LLM context per tier.
fn model_chunk_limit(cfg: &Config) -> usize {
    match ask_model_tier(cfg) {
        SynthesisModelTier::Large => 50,
        SynthesisModelTier::Medium => 28,
        SynthesisModelTier::LocalGemma => 20,
        SynthesisModelTier::Small => 10,
    }
}

/// Candidate pool fetched from Qdrant before reranking per tier. Must be large
/// enough to feed the tier's chunk limit (chunks selected can't exceed it).
fn model_candidate_limit(cfg: &Config) -> usize {
    match ask_model_tier(cfg) {
        SynthesisModelTier::Large => 250,
        SynthesisModelTier::Medium => 150,
        SynthesisModelTier::LocalGemma => 120,
        SynthesisModelTier::Small => 60,
    }
}

/// Hybrid (dense+sparse) prefetch window per arm before RRF fusion, per tier.
fn model_hybrid_candidates(cfg: &Config) -> usize {
    match ask_model_tier(cfg) {
        SynthesisModelTier::Large => 200,
        SynthesisModelTier::Medium => 120,
        SynthesisModelTier::LocalGemma => 100,
        SynthesisModelTier::Small => 60,
    }
}

fn ask_candidate_limit(cfg: &Config, toml: &TomlConfig) -> usize {
    resolve_clamped_usize(
        "AXON_ASK_CANDIDATE_LIMIT",
        toml.ask.candidate_limit,
        model_candidate_limit(cfg),
        8,
        300,
    )
}

fn ask_chunk_limit(cfg: &Config, toml: &TomlConfig) -> usize {
    resolve_clamped_usize(
        "AXON_ASK_CHUNK_LIMIT",
        toml.ask.chunk_limit,
        model_chunk_limit(cfg),
        3,
        64,
    )
}

fn ask_min_relevance_score(toml: &TomlConfig) -> f64 {
    resolve_clamped_f64(
        "AXON_ASK_MIN_RELEVANCE_SCORE",
        toml.ask.min_relevance_score,
        0.45,
        -1.0,
        2.0,
    )
}

fn hybrid_search_enabled(toml: &TomlConfig) -> bool {
    env_bool_opt("AXON_HYBRID_SEARCH")
        .or(toml.search.hybrid_enabled)
        .unwrap_or(true)
}

fn hybrid_search_candidates(toml: &TomlConfig) -> usize {
    resolve_clamped_usize(
        "AXON_HYBRID_CANDIDATES",
        toml.search.hybrid_candidates,
        100,
        10,
        500,
    )
}

fn ask_hybrid_candidates(cfg: &Config, toml: &TomlConfig) -> usize {
    resolve_clamped_usize(
        "AXON_ASK_HYBRID_CANDIDATES",
        toml.search.ask_hybrid_candidates,
        model_hybrid_candidates(cfg),
        10,
        500,
    )
}

fn tei_max_retries(toml: &TomlConfig) -> usize {
    resolve_clamped_usize("TEI_MAX_RETRIES", toml.tei.max_retries, 5, 0, 20)
}

fn tei_request_timeout_ms(toml: &TomlConfig) -> u64 {
    resolve_clamped_u64(
        "TEI_REQUEST_TIMEOUT_MS",
        toml.tei.request_timeout_ms,
        30_000,
        1000,
        300_000,
    )
}

fn tei_max_client_batch_size(toml: &TomlConfig) -> usize {
    resolve_clamped_usize(
        "TEI_MAX_CLIENT_BATCH_SIZE",
        toml.tei.max_client_batch_size,
        128,
        1,
        256,
    )
}

fn ingest_lanes(toml: &TomlConfig) -> usize {
    resolve_clamped_usize("AXON_INGEST_LANES", toml.workers.ingest_lanes, 2, 1, 16)
}

fn embed_lanes(toml: &TomlConfig) -> usize {
    resolve_clamped_usize("AXON_EMBED_LANES", toml.workers.embed_lanes, 2, 1, 32)
}

fn unified_worker_concurrency(toml: &TomlConfig) -> usize {
    resolve_clamped_usize(
        "AXON_UNIFIED_WORKER_CONCURRENCY",
        toml.workers.unified_worker_concurrency,
        8,
        1,
        64,
    )
}

fn crawl_job_concurrency_limit(toml: &TomlConfig) -> usize {
    resolve_clamped_usize(
        "AXON_CRAWL_JOB_CONCURRENCY_LIMIT",
        toml.workers.crawl_job_concurrency_limit,
        1,
        1,
        64,
    )
}

fn embed_doc_timeout_secs(toml: &TomlConfig) -> u64 {
    resolve_clamped_u64(
        "AXON_EMBED_DOC_TIMEOUT_SECS",
        toml.workers.embed_doc_timeout_secs,
        300,
        30,
        3600,
    )
}

fn queue_summary_secs(toml: &TomlConfig) -> u64 {
    resolve_clamped_u64(
        "AXON_QUEUE_SUMMARY_SECS",
        toml.workers.queue_summary_secs,
        30,
        0,
        3600,
    )
}

fn freshness_tick_secs(toml: &TomlConfig) -> u64 {
    resolve_clamped_u64(
        "AXON_FRESHNESS_TICK_SECS",
        toml.freshness.tick_secs,
        60,
        1,
        3600,
    )
}

fn freshness_lease_secs(toml: &TomlConfig) -> u64 {
    resolve_clamped_u64(
        "AXON_FRESHNESS_LEASE_SECS",
        toml.freshness.lease_secs,
        1800,
        1,
        86_400,
    )
}

fn freshness_max_due_per_tick(toml: &TomlConfig) -> i64 {
    std::env::var("AXON_FRESHNESS_MAX_DUE_PER_TICK")
        .ok()
        .and_then(|raw| raw.parse::<i64>().ok())
        .or(toml.freshness.max_due_per_tick)
        .map(|value| value.clamp(1, 100))
        .unwrap_or(4)
}

fn freshness_max_concurrent_runs(toml: &TomlConfig) -> usize {
    resolve_clamped_usize(
        "AXON_FRESHNESS_MAX_CONCURRENT_RUNS",
        toml.freshness.max_concurrent_runs,
        2,
        1,
        16,
    )
}

fn freshness_run_retention_days(toml: &TomlConfig) -> i64 {
    std::env::var("AXON_FRESHNESS_RUN_RETENTION_DAYS")
        .ok()
        .and_then(|raw| raw.parse::<i64>().ok())
        .or(toml.freshness.run_retention_days)
        .map(|value| value.clamp(1, 3660))
        .unwrap_or(90)
}

fn qdrant_point_buffer(toml: &TomlConfig) -> usize {
    resolve_clamped_usize(
        "AXON_QDRANT_POINT_BUFFER",
        toml.workers.qdrant_point_buffer,
        1024,
        128,
        16_384,
    )
}

fn job_wait_timeout_secs(toml: &TomlConfig) -> u64 {
    resolve_clamped_u64(
        "AXON_JOB_WAIT_TIMEOUT_SECS",
        toml.workers.job_wait_timeout_secs,
        300,
        30,
        3600,
    )
}

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
    resolve_clamped_usize(env_key, toml_value, default, 0, 10_000)
}

#[cfg(test)]
#[path = "tuning_tests.rs"]
mod tests;
