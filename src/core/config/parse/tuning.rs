use super::helpers::env_bool_opt;
use super::performance;
use super::toml_config::{TomlConfig, load_toml_config};
use crate::core::config::types::Config;

pub(super) fn apply_env_toml_tuning(cfg: &mut Config, toml: &TomlConfig) {
    cfg.ask_max_context_chars = ask_max_context_chars(toml);
    cfg.ask_candidate_limit = ask_candidate_limit(toml);
    cfg.ask_chunk_limit = ask_chunk_limit(toml);
    cfg.ask_full_docs = resolve_clamped_usize("AXON_ASK_FULL_DOCS", toml.ask.full_docs, 6, 1, 20);
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

    cfg.hybrid_search_enabled = hybrid_search_enabled(toml);
    cfg.hybrid_search_candidates = hybrid_search_candidates(toml);
    cfg.ask_hybrid_candidates = ask_hybrid_candidates(toml);
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
    cfg.tei_request_timeout_ms = tei_request_timeout_ms(toml);
    cfg.tei_max_client_batch_size = tei_max_client_batch_size(toml);
    cfg.ingest_lanes = ingest_lanes(toml);
    cfg.embed_lanes = embed_lanes(toml);
    cfg.embed_doc_timeout_secs = embed_doc_timeout_secs(toml);
    cfg.queue_summary_secs = queue_summary_secs(toml);
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
}

pub(crate) fn apply_default_lite_tuning(cfg: &mut Config) {
    match load_toml_config() {
        Ok(toml) => apply_env_toml_tuning(cfg, &toml),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "axon: failed to load TOML tuning for default_lite; using hardcoded defaults"
            );
        }
    }
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

fn ask_max_context_chars(toml: &TomlConfig) -> usize {
    resolve_clamped_usize(
        "AXON_ASK_MAX_CONTEXT_CHARS",
        toml.ask.max_context_chars,
        300_000,
        20_000,
        1_000_000,
    )
}

fn ask_candidate_limit(toml: &TomlConfig) -> usize {
    resolve_clamped_usize(
        "AXON_ASK_CANDIDATE_LIMIT",
        toml.ask.candidate_limit,
        250,
        8,
        300,
    )
}

fn ask_chunk_limit(toml: &TomlConfig) -> usize {
    resolve_clamped_usize("AXON_ASK_CHUNK_LIMIT", toml.ask.chunk_limit, 20, 3, 40)
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

fn ask_hybrid_candidates(toml: &TomlConfig) -> usize {
    resolve_clamped_usize(
        "AXON_ASK_HYBRID_CANDIDATES",
        toml.search.ask_hybrid_candidates,
        150,
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
        64,
        1,
        128,
    )
}

fn ingest_lanes(toml: &TomlConfig) -> usize {
    resolve_clamped_usize("AXON_INGEST_LANES", toml.workers.ingest_lanes, 2, 1, 16)
}

fn embed_lanes(toml: &TomlConfig) -> usize {
    resolve_clamped_usize("AXON_EMBED_LANES", toml.workers.embed_lanes, 2, 1, 32)
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

fn qdrant_point_buffer(toml: &TomlConfig) -> usize {
    resolve_clamped_usize(
        "AXON_QDRANT_POINT_BUFFER",
        toml.workers.qdrant_point_buffer,
        256,
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
