//! Wire-format `config.toml` shape: the 20-section contract from
//! `docs/pipeline-unification/configuration/config-contract.md`.
//!
//! This is what `toml::from_str` actually parses. `convert::into_legacy`
//! folds every field here onto the existing flat [`super::TomlConfig`]
//! shape so the ~150 downstream consumers in `tuning.rs`, `config_literal.rs`,
//! and `build_config.rs` need zero changes. Fields with no current runtime
//! consumer are still parsed (so real `config.toml` files round-trip
//! cleanly) but are otherwise inert, matching the pre-existing precedent of
//! several `#[allow(dead_code)]` sections in the legacy shape.
//!
//! All sections/subsections `deny_unknown_fields` so typos and stale
//! section names fail loudly instead of being silently ignored.

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::config) struct RawTomlConfig {
    #[serde(default)]
    pub server: RawServerSection,
    #[serde(default)]
    pub sources: RawSourcesSection,
    #[serde(default)]
    pub pipeline: RawPipelineSection,
    #[serde(default)]
    pub jobs: RawJobsSection,
    #[serde(default)]
    pub providers: RawProvidersSection,
    #[serde(default)]
    pub retrieval: RawRetrievalSection,
    #[serde(default)]
    pub ask: RawAskSection,
    #[serde(default)]
    pub crawl: RawCrawlSection,
    #[serde(default)]
    pub watch: RawWatchSection,
    #[serde(default)]
    #[allow(dead_code)]
    pub memory: RawMemorySection,
    #[serde(default)]
    #[allow(dead_code)]
    pub graph: RawGraphSection,
    #[serde(default)]
    #[allow(dead_code)]
    pub artifacts: RawArtifactsSection,
    #[serde(default)]
    #[allow(dead_code)]
    pub prune: RawPruneSection,
    #[serde(default)]
    #[allow(dead_code)]
    pub observability: RawObservabilitySection,
    #[serde(default)]
    #[allow(dead_code)]
    pub security: RawSecuritySection,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawServerSection {
    pub default_collection: Option<String>,
    #[allow(dead_code)]
    pub json_pretty: Option<bool>,
    #[allow(dead_code)]
    pub request_timeout_secs: Option<u64>,
    /// Compile-time dev escape hatch for embedding fallback web assets.
    pub allow_fallback_web_assets: Option<bool>,
    #[serde(default)]
    pub mcp: RawServerMcpSection,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawServerMcpSection {
    pub task_result_wait_timeout_secs: Option<u64>,
    #[serde(default)]
    pub embed: RawServerMcpEmbedSection,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawServerMcpEmbedSection {
    pub max_local_bytes: Option<u64>,
    pub max_local_depth: Option<usize>,
    pub max_local_entries: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawSourcesSection {
    #[allow(dead_code)]
    pub embed_by_default: Option<bool>,
    #[allow(dead_code)]
    pub default_scope_web: Option<String>,
    #[allow(dead_code)]
    pub default_scope_local: Option<String>,
    #[allow(dead_code)]
    pub authority_map_enabled: Option<bool>,
    #[allow(dead_code)]
    pub max_source_items: Option<u64>,
    #[allow(dead_code)]
    pub source_id_strategy: Option<String>,
    #[serde(default, rename = "code-search")]
    pub code_search: RawCodeSearchSection,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawCodeSearchSection {
    pub freshness_ttl_secs: Option<u64>,
    pub reindex_timeout_secs: Option<u64>,
    pub max_file_bytes: Option<u64>,
    pub changed_file_batch_size: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawPipelineSection {
    #[allow(dead_code)]
    pub max_active_source_jobs: Option<usize>,
    #[allow(dead_code)]
    pub max_active_interactive_jobs: Option<usize>,
    #[allow(dead_code)]
    pub batch_yield_ms: Option<u64>,
    #[allow(dead_code)]
    pub item_failure_policy: Option<String>,
    #[allow(dead_code)]
    pub publish_requires_cleanup: Option<bool>,
    #[allow(dead_code)]
    pub max_document_bytes: Option<u64>,
    pub ingest_lanes: Option<usize>,
    pub embed_lanes: Option<usize>,
    pub unified_worker_concurrency: Option<usize>,
    pub crawl_job_concurrency_limit: Option<usize>,
    pub embed_doc_timeout_secs: Option<u64>,
    pub queue_summary_secs: Option<u64>,
    pub qdrant_point_buffer: Option<usize>,
    pub job_wait_timeout_secs: Option<u64>,
    pub max_pending_crawl_jobs: Option<usize>,
    pub max_pending_embed_jobs: Option<usize>,
    pub max_pending_extract_jobs: Option<usize>,
    pub max_pending_ingest_jobs: Option<usize>,
    #[serde(default)]
    pub chunking: RawChunkingSection,
    #[serde(default)]
    pub endpoints: RawEndpointsSection,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawChunkingSection {
    pub markdown_min_chars: Option<usize>,
    pub markdown_max_chars: Option<usize>,
    pub overlap_chars: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawEndpointsSection {
    pub bundle_concurrency: Option<usize>,
    pub chrome_concurrency: Option<usize>,
    pub verify_concurrency: Option<usize>,
    pub probe_concurrency: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawJobsSection {
    #[allow(dead_code)]
    pub heartbeat_secs: Option<u64>,
    pub stale_after_secs: Option<i64>,
    pub stale_grace_secs: Option<i64>,
    pub event_retention_days: Option<u32>,
    pub failed_event_retention_days: Option<u32>,
    pub terminal_retention_days: Option<u32>,
    /// Retention window (days) for `provider_reservations` rows.
    pub provider_health_retention_days: Option<u32>,
    /// Retention window (days) for `job_artifacts` rows.
    pub artifact_retention_days: Option<u32>,
    /// Seconds between periodic differentiated retention sweeps.
    pub retention_sweep_secs: Option<i64>,
    /// SLO in seconds for the priority-aware interactive-lane starvation watchdog.
    pub interactive_starvation_slo_secs: Option<i64>,
    #[allow(dead_code)]
    pub max_events_per_job: Option<u64>,
    #[allow(dead_code)]
    pub default_priority: Option<String>,
    pub watchdog_sweep_secs: Option<i64>,
    pub worker_starvation_secs: Option<i64>,
    pub crawl_job_timeout_secs: Option<i64>,
    pub max_job_attempts: Option<i64>,
    #[serde(default)]
    pub freshness: RawFreshnessSection,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawFreshnessSection {
    pub tick_secs: Option<u64>,
    pub lease_secs: Option<u64>,
    pub max_due_per_tick: Option<i64>,
    pub max_concurrent_runs: Option<usize>,
    pub run_retention_days: Option<i64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawProvidersSection {
    #[serde(default)]
    pub embedding: RawEmbeddingSection,
    #[serde(default)]
    pub vector: RawVectorSection,
    #[serde(default)]
    pub llm: RawLlmSection,
    #[serde(default)]
    pub search: RawProviderSearchSection,
    #[serde(default)]
    pub fetch: RawFetchSection,
    #[serde(default)]
    pub render: RawRenderSection,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawEmbeddingSection {
    pub batch_size: Option<usize>,
    pub max_concurrent_requests: Option<usize>,
    pub max_in_flight_inputs: Option<usize>,
    pub request_timeout_ms: Option<u64>,
    pub max_retries: Option<usize>,
    #[allow(dead_code)]
    pub retry_backoff_ms: Option<u64>,
    #[allow(dead_code)]
    pub cooldown_after_failures: Option<usize>,
    #[allow(dead_code)]
    pub cooldown_secs: Option<u64>,
    #[allow(dead_code)]
    pub interactive_reserved_requests: Option<usize>,
    #[allow(dead_code)]
    pub background_max_concurrent_requests: Option<usize>,
    #[allow(dead_code)]
    pub maintenance_max_concurrent_requests: Option<usize>,
    #[allow(dead_code)]
    pub query_instruction_enabled: Option<bool>,
    pub pool_max_inputs: Option<usize>,
    pub prep_concurrency: Option<usize>,
    pub max_chunks_per_doc: Option<usize>,
    pub max_source_chunks_per_doc: Option<usize>,
    pub dedupe_exact_chunks: Option<bool>,
    pub openai_model: Option<String>,
    pub openai_max_client_batch_size: Option<usize>,
    pub openai_max_concurrent: Option<usize>,
    pub openai_max_in_flight_inputs: Option<usize>,
    pub openai_pool_max_inputs: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawVectorSection {
    pub write_concurrency: Option<usize>,
    #[allow(dead_code)]
    pub read_concurrency: Option<usize>,
    #[allow(dead_code)]
    pub delete_concurrency: Option<usize>,
    pub upsert_batch_points: Option<usize>,
    #[allow(dead_code)]
    pub delete_batch_points: Option<usize>,
    pub hybrid_enabled: Option<bool>,
    pub hnsw_ef: Option<usize>,
    pub hnsw_ef_legacy: Option<usize>,
    pub structured_data_max_bytes: Option<usize>,
    pub bulk_load: Option<bool>,
    pub bulk_indexing_threshold_kb: Option<usize>,
    pub indexing_threshold_kb: Option<usize>,
    pub hnsw_m: Option<usize>,
    pub hnsw_ef_construct: Option<usize>,
    pub payload_index_profile: Option<String>,
    pub payload_index_parallelism: Option<usize>,
    pub hnsw_on_disk: Option<bool>,
    pub quantization_always_ram: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawLlmSection {
    #[allow(dead_code)]
    pub backend: Option<String>,
    pub completion_concurrency: Option<usize>,
    pub completion_timeout_secs: Option<u64>,
    #[allow(dead_code)]
    pub synthesis_model: Option<String>,
    #[allow(dead_code)]
    pub chat_model: Option<String>,
    pub high_context: Option<bool>,
    pub codex_pool_idle_ttl_secs: Option<u64>,
    pub synthesis_gemini_model: Option<String>,
    pub chat_gemini_model: Option<String>,
    pub synthesis_openai_model: Option<String>,
    pub chat_openai_model: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawProviderSearchSection {
    #[allow(dead_code)]
    pub default: Option<String>,
    #[allow(dead_code)]
    pub result_limit: Option<usize>,
    pub research_full_content: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawFetchSection {
    #[allow(dead_code)]
    pub concurrency: Option<usize>,
    pub request_timeout_ms: Option<u64>,
    pub retries: Option<usize>,
    pub retry_backoff_ms: Option<u64>,
    #[allow(dead_code)]
    pub user_agent: Option<String>,
    pub delay_ms: Option<u64>,
    pub batch_timeout_secs: Option<u64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawRenderSection {
    #[allow(dead_code)]
    pub enabled: Option<bool>,
    #[allow(dead_code)]
    pub max_concurrent_pages: Option<usize>,
    #[allow(dead_code)]
    pub page_timeout_ms: Option<u64>,
    pub user_agent: Option<String>,
    pub bypass_csp: Option<bool>,
    pub accept_invalid_certs: Option<bool>,
    pub network_idle_timeout_secs: Option<u64>,
    pub bootstrap_timeout_ms: Option<u64>,
    pub bootstrap_retries: Option<usize>,
    pub remote_local_policy: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawRetrievalSection {
    #[allow(dead_code)]
    pub limit: Option<usize>,
    pub hybrid_candidates: Option<usize>,
    pub ask_hybrid_candidates: Option<usize>,
    #[allow(dead_code)]
    pub min_score: Option<f64>,
    #[allow(dead_code)]
    pub exclude_local_code_by_default: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawAskSection {
    pub max_context_chars: Option<usize>,
    pub chunk_limit: Option<usize>,
    pub candidate_limit: Option<usize>,
    pub full_docs: Option<usize>,
    pub backfill_chunks: Option<usize>,
    pub doc_fetch_concurrency: Option<usize>,
    pub doc_chunk_limit: Option<usize>,
    pub min_relevance_score: Option<f64>,
    pub authoritative_domains: Option<Vec<String>>,
    pub authoritative_boost: Option<f64>,
    pub min_citations_nontrivial: Option<usize>,
    #[serde(default)]
    pub cache: RawAskCacheSection,
    #[serde(default)]
    pub adaptive: RawAskAdaptiveSection,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawAskCacheSection {
    pub enabled: Option<bool>,
    pub max_capacity_bytes: Option<u64>,
    pub ttl_secs: Option<u64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawAskAdaptiveSection {
    pub fulldoc_skip_enabled: Option<bool>,
    pub fulldoc_skip_min_urls: Option<usize>,
    pub fulldoc_skip_min_chars: Option<usize>,
    pub fulldoc_skip_score_delta: Option<f64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawCrawlSection {
    #[allow(dead_code)]
    pub max_pages: Option<u32>,
    #[allow(dead_code)]
    pub max_depth: Option<usize>,
    pub respect_robots: Option<bool>,
    pub discover_sitemaps: Option<bool>,
    pub min_markdown_chars: Option<usize>,
    pub drop_thin_markdown: Option<bool>,
    pub memory_abort_percent: Option<f64>,
    pub sitemap_since_days: Option<u32>,
    pub max_sitemaps: Option<usize>,
    pub discover_llms_txt: Option<bool>,
    pub max_llms_txt_urls: Option<usize>,
    pub auto_switch_thin_ratio: Option<f64>,
    pub auto_switch_min_pages: Option<usize>,
    pub url_whitelist: Option<Vec<String>>,
    pub allow_unbounded_broad_crawl: Option<bool>,
    pub max_page_bytes: Option<u64>,
    pub redirect_policy_strict: Option<bool>,
    pub ladder_strategy1_threshold: Option<usize>,
    pub ladder_strategy2_threshold: Option<usize>,
    pub ladder_body_multiplier: Option<f64>,
    pub concurrency_limit: Option<usize>,
    pub crawl_concurrency_limit: Option<usize>,
    pub backfill_concurrency_limit: Option<usize>,
    #[serde(default)]
    pub adaptive_concurrency: RawAdaptiveConcurrencySection,
    #[serde(default)]
    pub verticals: RawVerticalsSection,
    #[serde(default)]
    pub antibot: RawAntibotSection,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawAdaptiveConcurrencySection {
    pub enabled: Option<bool>,
    pub min: Option<usize>,
    pub max: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawVerticalsSection {
    pub enabled: Option<bool>,
    pub auto_dispatch_skip: Option<Vec<String>>,
    pub cache_ttl_secs: Option<HashMap<String, u64>>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawAntibotSection {
    pub cookie_warmup: Option<bool>,
    pub max_body_scan_bytes: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(in crate::config) struct RawWatchSection {
    pub tick_secs: Option<u64>,
    pub lease_secs: Option<i64>,
    #[allow(dead_code)]
    pub max_due_per_tick: Option<i64>,
    #[allow(dead_code)]
    pub max_concurrent_runs: Option<usize>,
    #[allow(dead_code)]
    pub coalesce_source_refreshes: Option<bool>,
}

/// Pure-contract-addition sections: parsed for forward compatibility (so real
/// `config.toml` files written against the contract round-trip cleanly) but
/// not yet wired to any runtime knob.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[allow(dead_code)]
pub(in crate::config) struct RawMemorySection {
    pub collection: Option<String>,
    pub decay_enabled: Option<bool>,
    pub review_interval_days: Option<u32>,
    pub pin_boost: Option<f64>,
    pub forget_deletes_vectors: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[allow(dead_code)]
pub(in crate::config) struct RawGraphSection {
    pub enabled: Option<bool>,
    pub candidate_confidence_floor: Option<f64>,
    pub auto_merge_confidence: Option<f64>,
    pub evidence_retention_days: Option<u32>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[allow(dead_code)]
pub(in crate::config) struct RawArtifactsSection {
    pub retention_days: Option<u32>,
    pub max_inline_bytes: Option<u64>,
    pub max_artifact_bytes: Option<u64>,
    pub write_warc_by_default: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[allow(dead_code)]
pub(in crate::config) struct RawPruneSection {
    pub dry_run_default: Option<bool>,
    pub require_confirm_for_destructive: Option<bool>,
    pub delete_batch_size: Option<usize>,
    pub cleanup_retry_secs: Option<u64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[allow(dead_code)]
pub(in crate::config) struct RawObservabilitySection {
    pub log_level: Option<String>,
    pub structured_logs: Option<bool>,
    pub progress_event_throttle_ms: Option<u64>,
    pub metrics_enabled: Option<bool>,
    pub redact_logs: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[allow(dead_code)]
pub(in crate::config) struct RawSecuritySection {
    pub allow_private_network_fetch: Option<bool>,
    pub allow_local_paths: Option<String>,
    pub allow_tool_execution: Option<bool>,
    pub max_tool_output_bytes: Option<u64>,
    pub redaction_fail_closed: Option<bool>,
}
