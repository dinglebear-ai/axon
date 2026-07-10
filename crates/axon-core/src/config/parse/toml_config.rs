use crate::paths::axon_config_path;
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};

mod convert;
mod raw;

/// TOML configuration — tuning knobs only, safe to commit to source control.
///
/// Phase 1 scope (~15 fields across 4 sections). All fields are `Option<T>`
/// so absent keys fall through to env var and hardcoded defaults.
/// `#[serde(deny_unknown_fields)]` turns typos into parse errors rather than
/// silent ignores.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct TomlConfig {
    #[serde(default)]
    #[allow(dead_code)]
    pub build: TomlBuildSection,
    #[serde(default)]
    pub services: TomlServicesSection,
    #[serde(default)]
    pub llm: TomlLlmSection,
    #[serde(default)]
    pub search: TomlSearchSection,
    #[serde(default)]
    pub ask: TomlAskSection,
    #[serde(default)]
    pub tei: TomlTeiSection,
    #[serde(default)]
    #[allow(dead_code)]
    pub embed: TomlEmbedSection,
    #[serde(default)]
    #[allow(dead_code)]
    pub chunking: TomlChunkingSection,
    #[serde(default)]
    #[allow(dead_code)]
    pub qdrant: TomlQdrantSection,
    #[serde(default)]
    #[allow(dead_code)]
    #[serde(rename = "code-search")]
    pub code_search: TomlCodeSearchSection,
    #[serde(default)]
    #[allow(dead_code)]
    pub watch: TomlWatchSection,
    #[serde(default)]
    #[allow(dead_code)]
    pub endpoints: TomlEndpointsSection,
    #[serde(default)]
    #[allow(dead_code)]
    pub mcp: TomlMcpSection,
    #[serde(default)]
    pub workers: TomlWorkersSection,
    #[serde(default)]
    pub freshness: TomlFreshnessSection,
    #[serde(default)]
    pub chrome: TomlChromeSection,
    #[serde(default)]
    pub scrape: TomlScrapeSection,
    #[serde(default)]
    pub verticals: TomlVerticalsSection,
    #[serde(default)]
    pub antibot: TomlAntibotSection,
    #[serde(default)]
    pub payload: TomlPayloadSection,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[allow(dead_code)]
pub(super) struct TomlBuildSection {
    /// Compile-time development escape hatch for embedding fallback web assets
    /// when `apps/web/out` is intentionally absent/incomplete.
    pub allow_fallback_web_assets: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlLlmSection {
    /// Synthesis model for Gemini headless. Env wins.
    pub synthesis_gemini_model: Option<String>,
    /// Direct chat model for Gemini headless. Env wins.
    pub chat_gemini_model: Option<String>,
    /// Synthesis model for OpenAI-compatible completions. Env wins.
    pub synthesis_openai_model: Option<String>,
    /// Direct chat model for OpenAI-compatible completions. Env wins.
    pub chat_openai_model: Option<String>,
    /// Explicit override for whether the synthesis backend has a large context
    /// window (drives the adaptive full-docs floor in `ask`). Absent = infer
    /// from the model name. Env `AXON_SYNTHESIS_HIGH_CONTEXT` wins.
    pub synthesis_high_context: Option<bool>,
    /// Max concurrent LLM completion requests across the selected backend
    /// (clamped 1–64). Env `AXON_LLM_COMPLETION_CONCURRENCY` wins.
    pub completion_concurrency: Option<usize>,
    /// Timeout in seconds for each LLM completion request (clamped
    /// 10–1800). Env `AXON_LLM_COMPLETION_TIMEOUT_SECS` wins.
    pub completion_timeout_secs: Option<u64>,
    /// How long (seconds) an idle pooled `codex app-server` child may sit
    /// unused before the pool discards it on next checkout (clamped 0–3600;
    /// 0 disables TTL eviction). Env `AXON_CODEX_POOL_IDLE_TTL_SECS` wins.
    pub codex_pool_idle_ttl_secs: Option<u64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlScrapeSection {
    /// Respect robots.txt directives. Default false.
    pub respect_robots: Option<bool>,
    /// Minimum content length; shorter pages are flagged thin. Default 200.
    pub min_markdown_chars: Option<usize>,
    /// Skip thin pages instead of saving or embedding them. Default true.
    pub drop_thin_markdown: Option<bool>,
    /// Discover and backfill URLs from sitemap.xml after crawl. Default true.
    pub discover_sitemaps: Option<bool>,
    /// Only backfill sitemap URLs with a recent `<lastmod>` date. Default 0.
    pub sitemap_since_days: Option<u32>,
    /// Maximum number of sitemap documents to parse. Default 512.
    pub max_sitemaps: Option<usize>,
    /// Probe `/llms.txt` at the site root and merge its links into backfill/map. Default true.
    pub discover_llms_txt: Option<bool>,
    /// Maximum number of URLs to take from a single `/llms.txt`. Default 512.
    pub max_llms_txt_urls: Option<usize>,
    /// Delay between requests in milliseconds. Default 0.
    pub delay_ms: Option<u64>,
    /// Per-request HTTP timeout in milliseconds. Default comes from performance profile.
    pub request_timeout_ms: Option<u64>,
    /// End-to-end timeout for one service-level scrape batch. Default 120.
    pub batch_timeout_secs: Option<u64>,
    /// Number of retries on failed fetches. Default comes from performance profile.
    pub fetch_retries: Option<usize>,
    /// Backoff between retries in milliseconds. Default comes from performance profile.
    pub retry_backoff_ms: Option<u64>,
    /// Thin-page ratio to trigger auto-switch to Chrome. Default 0.60.
    pub auto_switch_thin_ratio: Option<f64>,
    /// Minimum pages before auto-switch eligibility check. Default 10.
    pub auto_switch_min_pages: Option<usize>,
    /// Only crawl URLs matching these regex patterns.
    pub url_whitelist: Option<Vec<String>>,
    /// Allow `max-pages = 0` without an explicit path budget or URL whitelist.
    pub allow_unbounded_broad_crawl: Option<bool>,
    /// Maximum response size per page in bytes; 0 means unlimited. Default 4 MiB.
    pub max_page_bytes: Option<u64>,
    /// RSS percent of host/cgroup memory that aborts a crawl; 0 disables. Default 85.
    pub crawl_memory_abort_percent: Option<f64>,
    /// Only follow same-origin redirects. Default false.
    pub redirect_policy_strict: Option<bool>,
    /// DOM retry ladder Strategy 1 threshold (words). Default 30.
    pub ladder_strategy1_threshold: Option<usize>,
    /// DOM retry ladder Strategy 2 threshold (words). Default 200.
    pub ladder_strategy2_threshold: Option<usize>,
    /// Body-fallback multiplier; fallback wins only if it produces N x scored
    /// word count. Default 2.0.
    pub ladder_body_multiplier: Option<f64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlVerticalsSection {
    /// Enable per-site vertical extractors. Default true.
    pub enabled: Option<bool>,
    /// Vertical extractor names to SKIP in auto-dispatch.
    pub auto_dispatch_skip: Option<Vec<String>>,
    /// Per-vertical cache TTL in seconds (extractor name → TTL).
    pub cache_ttl_secs: Option<std::collections::HashMap<String, u64>>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlAntibotSection {
    /// Enable Akamai/CF cookie warmup retry on challenge detection. Default true.
    pub cookie_warmup: Option<bool>,
    /// Maximum bytes scanned for antibot challenge patterns. Default 150000.
    pub max_body_scan_bytes: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlPayloadSection {
    /// Maximum bytes stored in Qdrant `structured_blob` payload per chunk.
    /// Default 65536 (64 KiB).
    pub structured_data_max_bytes: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlServicesSection {
    /// Deprecated compatibility fallback. Runtime still accepts this temporarily
    /// and warns; move to `QDRANT_URL` in `.env`.
    pub qdrant_url: Option<String>,
    /// Deprecated compatibility fallback. Runtime still accepts this temporarily
    /// and warns; move to `TEI_URL` in `.env`.
    pub tei_url: Option<String>,
    /// Deprecated compatibility fallback. Runtime still accepts this temporarily
    /// and warns; move to `AXON_CHROME_REMOTE_URL` in `.env`.
    pub chrome_remote_url: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlSearchSection {
    pub hybrid_enabled: Option<bool>,
    /// Candidates per prefetch arm before RRF fusion (clamped 10–500).
    pub hybrid_candidates: Option<usize>,
    /// Hybrid window for the ask pipeline (clamped 10–500).
    pub ask_hybrid_candidates: Option<usize>,
    /// HNSW ef for named-mode (dense+sparse) collections (clamped 32–512).
    pub hnsw_ef: Option<usize>,
    /// HNSW ef for legacy unnamed-mode collections (clamped 16–256).
    pub hnsw_ef_legacy: Option<usize>,
    /// Qdrant collection name.
    pub collection: Option<String>,
    /// When true (default), `research` fetches each top source's full page
    /// and synthesizes over it; when false it synthesizes over search
    /// snippets only (much faster). Env `AXON_RESEARCH_FULL_CONTENT` wins.
    pub research_full_content: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlAskSection {
    /// Deprecated compatibility field. Gemini headless is the only backend now,
    /// so old `[ask] backend = "headless"` config is accepted and ignored.
    #[serde(rename = "backend")]
    pub _backend: Option<serde::de::IgnoredAny>,
    /// Max context characters passed to the LLM (clamped 20_000–1_000_000).
    pub max_context_chars: Option<usize>,
    /// Max chunks returned per ask query (clamped 3–40).
    pub chunk_limit: Option<usize>,
    /// Max candidate chunks fetched before scoring (clamped 8–300).
    pub candidate_limit: Option<usize>,
    /// Max full documents included in context (clamped 1–20).
    pub full_docs: Option<usize>,
    /// Backfill chunks from top documents to pad context (clamped 0–20).
    pub backfill_chunks: Option<usize>,
    /// Concurrent document fetches during context build (clamped 1–16).
    pub doc_fetch_concurrency: Option<usize>,
    /// Max chunks per document in context (clamped 8–2000).
    pub doc_chunk_limit: Option<usize>,
    /// Minimum relevance score threshold (clamped -1.0–2.0).
    pub min_relevance_score: Option<f64>,
    /// Authoritative domains to boost in reranking.
    pub authoritative_domains: Option<Vec<String>>,
    /// Boost weight for authoritative domains in reranking.
    pub authoritative_boost: Option<f64>,
    /// Min unique citations for non-trivial answers (clamped 1–5).
    pub min_citations_nontrivial: Option<usize>,
    /// In-process document-chunk cache for the ask full-doc fetch path.
    /// Only useful in long-lived parents (`axon serve`, `axon mcp`).
    /// (bd axon_rust-pmc)
    #[serde(default)]
    pub cache: TomlAskCacheSection,
    /// Adaptive ask heuristics — currently the full-doc fetch skip gate.
    /// Opt-in until validated against the `axon evaluate` golden set.
    /// (bd axon_rust-30y)
    #[serde(default)]
    pub adaptive: TomlAskAdaptiveSection,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlAskCacheSection {
    /// Enable the cache. Default: false.
    pub enabled: Option<bool>,
    /// Max bytes (summed `chunk_text` length). Default: 268_435_456 (256 MiB).
    pub max_capacity_bytes: Option<u64>,
    /// TTL in seconds. Capped at 300s. Default: 300.
    pub ttl_secs: Option<u64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlAskAdaptiveSection {
    /// Enable the adaptive full-doc fetch skip gate. Default: false (opt-in).
    pub fulldoc_skip_enabled: Option<bool>,
    /// Minimum unique URLs required in reranked top-K. Default: 3.
    pub fulldoc_skip_min_urls: Option<usize>,
    /// Minimum total chunk_text bytes summed across reranked top-K. Default: 4000.
    pub fulldoc_skip_min_chars: Option<usize>,
    /// Cosine-mode score floor offset on top of `ask_min_relevance_score`. Default: 0.15.
    pub fulldoc_skip_score_delta: Option<f64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlTeiSection {
    /// Max retry attempts after the initial TEI request.
    pub max_retries: Option<usize>,
    /// Per-attempt timeout in milliseconds.
    pub request_timeout_ms: Option<u64>,
    /// Default batch size (auto-splits on HTTP 413).
    pub max_client_batch_size: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[allow(dead_code)]
pub(super) struct TomlEmbedSection {
    pub tei_max_concurrent: Option<usize>,
    pub tei_max_in_flight_inputs: Option<usize>,
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
#[allow(dead_code)]
pub(super) struct TomlChunkingSection {
    pub markdown_min_chars: Option<usize>,
    pub markdown_max_chars: Option<usize>,
    pub overlap_chars: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[allow(dead_code)]
pub(super) struct TomlQdrantSection {
    pub upsert_batch_size: Option<usize>,
    pub upsert_parallelism: Option<usize>,
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
#[allow(dead_code)]
pub(super) struct TomlCodeSearchSection {
    pub freshness_ttl_secs: Option<u64>,
    pub reindex_timeout_secs: Option<u64>,
    pub max_file_bytes: Option<u64>,
    pub changed_file_batch_size: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[allow(dead_code)]
pub(super) struct TomlWatchSection {
    pub tick_secs: Option<u64>,
    pub lease_secs: Option<i64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[allow(dead_code)]
pub(super) struct TomlEndpointsSection {
    pub bundle_concurrency: Option<usize>,
    pub chrome_concurrency: Option<usize>,
    pub verify_concurrency: Option<usize>,
    pub probe_concurrency: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[allow(dead_code)]
pub(super) struct TomlMcpSection {
    pub task_result_wait_timeout_secs: Option<u64>,
    #[serde(default)]
    pub embed: TomlMcpEmbedSection,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[allow(dead_code)]
pub(super) struct TomlMcpEmbedSection {
    pub max_local_bytes: Option<u64>,
    pub max_local_depth: Option<usize>,
    pub max_local_entries: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlWorkersSection {
    /// Parallel ingest worker lanes.
    pub ingest_lanes: Option<usize>,
    /// Parallel embed worker lanes.
    pub embed_lanes: Option<usize>,
    /// Maximum concurrent jobs the unified worker runs at once.
    pub unified_worker_concurrency: Option<usize>,
    /// Maximum concurrent `JobKind::Crawl` jobs the unified worker runs at
    /// once, independent of `unified_worker_concurrency` (crawl jobs share
    /// one Chrome instance).
    pub crawl_job_concurrency_limit: Option<usize>,
    /// Per-document embed timeout in seconds.
    pub embed_doc_timeout_secs: Option<u64>,
    /// Queue summary interval in seconds.
    pub queue_summary_secs: Option<u64>,
    /// Buffered Qdrant points before flush.
    pub qdrant_point_buffer: Option<usize>,
    /// Crawl queue cap (0 = unlimited).
    pub max_pending_crawl_jobs: Option<usize>,
    /// Embed queue cap (0 = unlimited).
    pub max_pending_embed_jobs: Option<usize>,
    /// Extract queue cap (0 = unlimited).
    pub max_pending_extract_jobs: Option<usize>,
    /// Ingest queue cap (0 = unlimited).
    pub max_pending_ingest_jobs: Option<usize>,
    /// Timeout in seconds for `--wait true` job polling (clamped 30–3600).
    /// Env: `AXON_JOB_WAIT_TIMEOUT_SECS`.
    pub job_wait_timeout_secs: Option<u64>,
    /// Override crawl and backfill concurrency limits at once.
    pub concurrency_limit: Option<usize>,
    /// Override crawl concurrency. Default comes from performance profile.
    pub crawl_concurrency_limit: Option<usize>,
    /// Override sitemap backfill concurrency. Default comes from performance profile.
    pub backfill_concurrency_limit: Option<usize>,
    /// Opt-in adaptive Spider crawl concurrency controls.
    #[serde(default)]
    pub adaptive_concurrency: TomlAdaptiveConcurrencySection,
    /// Seconds before a running job is considered stale.
    pub watchdog_stale_timeout_secs: Option<i64>,
    /// Additional grace period before a stale job is reclaimed.
    pub watchdog_confirm_secs: Option<i64>,
    /// Seconds between watchdog sweeps.
    pub watchdog_sweep_secs: Option<i64>,
    /// Seconds pending jobs may starve (zero running) before the liveness
    /// watchdog kicks/respawns the lane. 0 disables.
    pub worker_starvation_secs: Option<i64>,
    /// Maximum wall-clock seconds a single crawl job may run before abort. 0 disables.
    pub crawl_job_timeout_secs: Option<i64>,
    /// Maximum reclaim attempts before a stale-running job is dead-lettered
    /// (marked failed) instead of re-queued. 0 disables the cap.
    pub max_job_attempts: Option<i64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlFreshnessSection {
    /// Seconds between due-schedule sweeps. Default 60.
    pub tick_secs: Option<u64>,
    /// Lease TTL for one running freshness dispatch. Default 1800.
    pub lease_secs: Option<u64>,
    /// Due schedules claimed per scheduler tick. Default 4.
    pub max_due_per_tick: Option<i64>,
    /// Global concurrent freshness dispatches. Default 2.
    pub max_concurrent_runs: Option<usize>,
    /// Retention window for run history. Default 90 days.
    pub run_retention_days: Option<i64>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlAdaptiveConcurrencySection {
    pub enabled: Option<bool>,
    pub min: Option<usize>,
    pub max: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlChromeSection {
    /// Custom `User-Agent` header sent by Chrome. Env: `AXON_CHROME_USER_AGENT`.
    pub user_agent: Option<String>,
    /// Bypass Content Security Policy in Chrome. Default false.
    pub bypass_csp: Option<bool>,
    /// Accept invalid/self-signed TLS certificates in Chrome. Default false.
    pub accept_invalid_certs: Option<bool>,
    /// Seconds to wait for Chrome network idle before capture. Default 15.
    pub network_idle_timeout_secs: Option<u64>,
    /// Timeout in milliseconds for the remote Chrome bootstrap probe. Default 3000.
    pub bootstrap_timeout_ms: Option<u64>,
    /// Number of retries for the remote Chrome bootstrap probe. Default 2.
    pub bootstrap_retries: Option<usize>,
    /// Push Spider/Chromey's local policy to capable remote Chrome engines.
    pub remote_local_policy: Option<bool>,
}

/// Load TOML config from the first found path:
/// 1. `AXON_CONFIG_PATH` env var (if set and non-empty)
/// 2. `~/.axon/config.toml` via `axon_config_path()` (returns None when HOME unset)
/// 3. Neither found → `Ok(TomlConfig::default())` (silent)
///
/// Error policy:
/// - Default file absent → `Ok(TomlConfig::default())` (silent)
/// - Explicit `AXON_CONFIG_PATH` absent or unreadable → `Err(...)` (caller hard-fails)
/// - Default file present but unreadable → `Err(...)` for permission errors, warning + default for other I/O errors
/// - File present, parse error → `Err(...)` with path + line number (caller hard-fails)
pub(super) fn load_toml_config() -> Result<TomlConfig, String> {
    let path = resolve_config_path()?;
    let Some(resolved) = path else {
        return Ok(TomlConfig::default());
    };
    load_from_path(&resolved.path, resolved.explicit)
}

struct ResolvedConfigPath {
    path: PathBuf,
    explicit: bool,
}

fn resolve_config_path() -> Result<Option<ResolvedConfigPath>, String> {
    // Explicit override takes highest priority.
    if let Ok(v) = std::env::var("AXON_CONFIG_PATH") {
        let trimmed = v.trim().to_string();
        if !trimmed.is_empty() {
            let path = PathBuf::from(&trimmed);
            // Require .toml extension: prevents accidental probing of arbitrary
            // file paths (e.g. /etc/passwd) and keeps parse error messages
            // informative without disclosing unexpected system files.
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
            {
                return Err(format!(
                    "axon: error: AXON_CONFIG_PATH must point to a .toml file: {trimmed:?}"
                ));
            }
            return Ok(Some(ResolvedConfigPath {
                path,
                explicit: true,
            }));
        }
    }
    // Fall back to ~/.axon/config.toml (None when HOME is unset).
    Ok(axon_config_path().map(|path| ResolvedConfigPath {
        path,
        explicit: false,
    }))
}

fn load_from_path(path: &Path, explicit: bool) -> Result<TomlConfig, String> {
    // Reject symlinks: ~/.axon/config.toml controls service URLs (Qdrant,
    // TEI, Chrome CDP, OpenAI base). A planted
    // symlink under a permissive ~/.axon would let a local attacker
    // redirect those baseline endpoints. `read_to_string` follows symlinks
    // by default — we lstat first.
    match std::fs::symlink_metadata(path) {
        Ok(md) if md.file_type().is_symlink() => {
            return Err(format!(
                "axon: error: refusing to load symlinked config file '{}' (potential symlink attack)",
                path.display()
            ));
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !explicit => {
            return Ok(TomlConfig::default());
        }
        Err(e)
            if explicit
                || matches!(
                    e.kind(),
                    std::io::ErrorKind::PermissionDenied
                        | std::io::ErrorKind::IsADirectory
                        | std::io::ErrorKind::NotADirectory
                ) =>
        {
            return Err(format!(
                "axon: error: cannot read config file '{}': {e}",
                path.display()
            ));
        }
        Err(e) => {
            eprintln!(
                "axon: warning: cannot read config file '{}': {e}",
                path.display()
            );
            return Ok(TomlConfig::default());
        }
    }

    let contents = match read_config_file_no_follow(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !explicit => {
            return Ok(TomlConfig::default());
        }
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::PermissionDenied
                    | std::io::ErrorKind::IsADirectory
                    | std::io::ErrorKind::NotADirectory
            ) =>
        {
            // File exists but is unreadable/mis-typed — return Err so the caller can hard-fail.
            // Silent fallback would hide a misconfiguration the user must fix.
            return Err(format!(
                "axon: error: cannot read config file '{}': {e}",
                path.display()
            ));
        }
        Err(e) if explicit => {
            return Err(format!(
                "axon: error: cannot read config file '{}': {e}",
                path.display()
            ));
        }
        Err(e) => {
            eprintln!(
                "axon: warning: cannot read config file '{}': {e}",
                path.display()
            );
            return Ok(TomlConfig::default());
        }
    };

    parse_toml_config_str(&contents, Some(path))
}

/// Parse `config.toml` contents against the current 20-section contract
/// shape ([`raw::RawTomlConfig`]), then fold the result onto the legacy flat
/// [`TomlConfig`] every downstream consumer already reads. Deprecated
/// pre-contract section names (`[llm]`, `[tei]`, `[scrape]`, ...) are
/// detected before the typed parse so the error names the offending
/// section(s) and their new home instead of a bare serde "unknown field".
fn parse_toml_config_str(contents: &str, path: Option<&Path>) -> Result<TomlConfig, String> {
    let where_clause = path
        .map(|p| format!(" '{}'", p.display()))
        .unwrap_or_default();
    if let Some(msg) = convert::deprecated_section_error(contents) {
        return Err(format!("axon: error: config file{where_clause} {msg}"));
    }
    let raw = toml::from_str::<raw::RawTomlConfig>(contents)
        .map_err(|e| format!("axon: error: config file{where_clause} has a parse error: {e}"))?;
    Ok(convert::into_legacy(raw))
}

#[cfg(unix)]
fn read_config_file_no_follow(path: &Path) -> Result<String, std::io::Error> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

#[cfg(not(unix))]
fn read_config_file_no_follow(path: &Path) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}

#[cfg(test)]
pub(super) fn load_toml_config_from_str(s: &str) -> Result<TomlConfig, String> {
    parse_toml_config_str(s, None)
}

#[cfg(test)]
#[path = "toml_config_tests.rs"]
mod tests;
