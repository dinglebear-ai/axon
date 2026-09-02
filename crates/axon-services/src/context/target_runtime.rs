//! Production composition for [`TargetLocalSourceRuntime`].
//!
//! The `#[cfg(test)]` [`TargetLocalSourceRuntime::new`] constructor (in
//! `context.rs`) wires fakes for unit tests. This module owns the real
//! data-plane composition: it builds the ledger / vector / embedding stores from
//! [`Config`] so long-lived processes (`serve`, `mcp`) carry a working target
//! local-source runtime.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axon_adapters::boundary::{FetchProvider, RenderProvider};
use axon_adapters::providers::chrome_render::{
    CHROME_RENDER_PROVIDER_ID, ChromeRenderConfig, ChromeRenderProvider,
};
use axon_adapters::providers::http_fetch::{
    HTTP_FETCH_PROVIDER_ID, HttpFetchConfig, HttpFetchProvider,
};
use axon_adapters::{
    ArtifactCandidateSink, DepotArtifactCandidateSink, NoopArtifactCandidateSink,
    NoopSourceEnricher, SourceAdapter, web::WebSourceAdapter,
};
use axon_api::source::{InstructionSupport, ProviderId};
use axon_core::boundary::FileArtifactStore;
use axon_core::config::Config;
use axon_document::{DocumentPreparer, DocumentPreparerConfig};
use axon_embedding::cache::CachedEmbeddingProvider;
use axon_embedding::provider::EmbeddingProvider;
use axon_embedding::tei::{TeiEmbeddingConfig, TeiEmbeddingProvider};
use axon_jobs::boundary::JobStore;
use axon_jobs::embedding_cache_store::SqliteEmbeddingVectorCacheStore;
use axon_jobs::scheduler::SqliteWriteGate;
use axon_ledger::sqlite::SqliteLedgerStore;
use axon_vectors::store::VectorStore;
use sqlx::SqlitePool;
use tokio::sync::{Semaphore, watch};

mod read_stores;
mod schedulers;

use read_stores::build_qdrant_store;
pub use read_stores::{TargetReadStores, build_read_stores_from_config};
#[cfg(test)]
use schedulers::scheduler_authority_id;
use schedulers::{RuntimeSchedulers, build_runtime_schedulers, source_db_stage_capacity};

use super::{
    TargetLocalSourceRuntime,
    db_limited_ledger::DbLimitedLedgerStore,
    scheduled_web::{ScheduledFetchProvider, ScheduledRenderProvider},
};
const DEPOT_URL_ENV: &str = "AXON_ARTIFACT_CANDIDATE_DEPOT_URL";
const DEPOT_TOKEN_ENV: &str = "AXON_ARTIFACT_CANDIDATE_DEPOT_TOKEN";

/// Construct the TEI embedding provider seeded with the resolved embedding
/// identity, so `EmbeddingResult.model`/`dimensions` (stamped into every vector
/// payload) match the provider-derived values rather than a hardcoded seed.
fn build_tei_provider(cfg: &Config, identity: &EmbeddingIdentity) -> TeiEmbeddingProvider {
    TeiEmbeddingProvider::new(TeiEmbeddingConfig {
        endpoint: cfg.tei_url.clone(),
        model: identity.model.clone(),
        dimensions: identity.dimensions,
        timeout: Duration::from_millis(cfg.tei_request_timeout_ms),
        max_batch_inputs: cfg.tei_max_client_batch_size as u32,
        max_concurrent_requests: cfg.embed_tei_max_concurrent,
        max_in_flight_inputs: cfg.embed_tei_max_in_flight_inputs,
        max_input_tokens: MAX_INPUT_TOKENS,
        max_batch_tokens: tei_client_max_batch_tokens(),
        instruction_support: query_instruction_support(cfg),
        retry_backoff_ms: cfg.embed_tei_retry_backoff_ms,
        max_attempts: tei_max_attempts(cfg),
    })
}

/// Total TEI embed attempts per request = `cfg.tei_max_retries + 1` (1
/// initial attempt plus the configured retry count). Was previously a
/// hardcoded `MAX_ATTEMPTS = 6` constant inside `axon-embedding::tei`,
/// completely disconnected from `[providers.embedding].max-retries`/
/// `TEI_MAX_RETRIES` — setting either did nothing to the real retry budget.
fn tei_max_attempts(cfg: &Config) -> usize {
    cfg.tei_max_retries.saturating_add(1).max(1)
}

/// `[providers.embedding].query-instruction-enabled` gate: `false` forces
/// `InstructionSupport::None` at construction regardless of the model's real
/// capability, disabling the query/document instruction prefix entirely.
fn query_instruction_support(cfg: &Config) -> InstructionSupport {
    if cfg.embed_tei_query_instruction_enabled {
        InstructionSupport::QueryAndDocument
    } else {
        InstructionSupport::None
    }
}

/// Resolved embedding model + dimensions used to size the collection, seed the
/// provider, and stamp vector payloads.
#[derive(Debug, Clone)]
struct EmbeddingIdentity {
    model: String,
    dimensions: u32,
    verified: bool,
}

#[derive(Debug, Clone)]
struct CachedEmbeddingIdentity {
    identity: EmbeddingIdentity,
    expires_at: Instant,
}

#[derive(Default)]
struct EmbeddingIdentityCache {
    entries: HashMap<String, CachedEmbeddingIdentity>,
    in_flight: HashMap<String, watch::Sender<Option<EmbeddingIdentity>>>,
}

static EMBEDDING_IDENTITY_CACHE: OnceLock<Mutex<EmbeddingIdentityCache>> = OnceLock::new();
const EMBEDDING_IDENTITY_CACHE_TTL: Duration = Duration::from_secs(30);
const EMBEDDING_IDENTITY_FALLBACK_TTL: Duration = Duration::from_secs(5);
const EMBEDDING_IDENTITY_DURABLE_TTL: Duration = Duration::from_secs(30 * 60);

/// Resolve the embedding model + dimensions from the live TEI endpoint (`/info`
/// for `model_id`, a probe embed for dimensions). Builds a probe provider seeded
/// with the fallback identity purely to issue the derivation requests. Falls
/// back to the configured defaults when the provider is unreachable, so a
/// fire-and-forget CLI enqueue or an offline TEI never blocks store construction.
async fn resolve_embedding_identity(cfg: &Config) -> EmbeddingIdentity {
    let cache_key = embedding_identity_cache_key(cfg);
    let receiver = match claim_embedding_identity_resolution(&cache_key, cfg.clone()) {
        IdentityResolutionClaim::Cached(identity) => return identity,
        IdentityResolutionClaim::Wait(receiver) => receiver,
    };
    wait_for_embedding_identity(receiver).await
}

/// Resolve TEI identity with a process-independent SQLite cache. Short-lived
/// CLI reads therefore pay the `/info` + probe-embed cost only when the cache
/// is cold, while long-lived processes retain the faster in-memory singleflight
/// above. Fallback identities are deliberately never persisted.
async fn resolve_embedding_identity_with_pool(
    cfg: &Config,
    pool: &SqlitePool,
) -> EmbeddingIdentity {
    let cache_key = embedding_identity_cache_key(cfg);
    if let Some(identity) = load_durable_embedding_identity(pool, &cache_key).await {
        return identity;
    }

    let identity = resolve_embedding_identity(cfg).await;
    if identity.verified
        && let Err(error) = persist_durable_embedding_identity(pool, &cache_key, &identity).await
    {
        tracing::warn!(%error, "failed to persist verified embedding identity cache");
    }
    identity
}

async fn load_durable_embedding_identity(
    pool: &SqlitePool,
    cache_key: &str,
) -> Option<EmbeddingIdentity> {
    let ttl_ms = i64::try_from(EMBEDDING_IDENTITY_DURABLE_TTL.as_millis()).unwrap_or(i64::MAX);
    let cutoff = chrono::Utc::now().timestamp_millis().saturating_sub(ttl_ms);
    let row = sqlx::query_as::<_, (String, i64)>(
        "SELECT model, dimensions FROM provider_identity_cache \
         WHERE cache_key = ? AND provider_kind = 'embedding' AND updated_at >= ?",
    )
    .bind(cache_key)
    .bind(cutoff)
    .fetch_optional(pool)
    .await;

    match row {
        Ok(Some((model, dimensions))) => {
            let dimensions = u32::try_from(dimensions).ok()?;
            (!model.trim().is_empty() && dimensions > 0).then_some(EmbeddingIdentity {
                model,
                dimensions,
                verified: true,
            })
        }
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(%error, "failed to read durable embedding identity cache");
            None
        }
    }
}

async fn persist_durable_embedding_identity(
    pool: &SqlitePool,
    cache_key: &str,
    identity: &EmbeddingIdentity,
) -> Result<(), sqlx::Error> {
    debug_assert!(identity.verified);
    sqlx::query(
        "INSERT INTO provider_identity_cache \
         (cache_key, provider_kind, provider_id, model, dimensions, updated_at) \
         VALUES (?, 'embedding', ?, ?, ?, ?) \
         ON CONFLICT(cache_key) DO UPDATE SET \
           provider_kind = excluded.provider_kind, \
           provider_id = excluded.provider_id, \
           model = excluded.model, \
           dimensions = excluded.dimensions, \
           updated_at = excluded.updated_at",
    )
    .bind(cache_key)
    .bind(EMBEDDING_PROVIDER_ID)
    .bind(&identity.model)
    .bind(i64::from(identity.dimensions))
    .bind(chrono::Utc::now().timestamp_millis())
    .execute(pool)
    .await?;
    Ok(())
}

/// Invalidate the cached identity and any negative result for this exact TEI
/// configuration. Configuration reload callers can use this after changing a
/// model or endpoint instead of waiting for the bounded TTL to elapse.
pub fn invalidate_embedding_identity_cache(cfg: &Config) {
    let key = embedding_identity_cache_key(cfg);
    if let Ok(mut cache) = embedding_identity_cache().lock() {
        cache.entries.remove(&key);
    }
}

enum IdentityResolutionClaim {
    Cached(EmbeddingIdentity),
    Wait(watch::Receiver<Option<EmbeddingIdentity>>),
}

/// Atomically return a cached identity or join/create the one probe currently
/// allowed for a configuration key. The actual probe is detached from callers:
/// cancelling one query cannot strand the other callers behind its request.
fn claim_embedding_identity_resolution(key: &str, cfg: Config) -> IdentityResolutionClaim {
    let mut cache = embedding_identity_cache()
        .lock()
        .expect("embedding identity cache mutex poisoned");
    if let Some(entry) = cache.entries.get(key) {
        if entry.expires_at > Instant::now() {
            return IdentityResolutionClaim::Cached(entry.identity.clone());
        }
    }
    cache.entries.remove(key);
    if let Some(sender) = cache.in_flight.get(key) {
        return IdentityResolutionClaim::Wait(sender.subscribe());
    }

    let (sender, receiver) = watch::channel(None);
    cache.in_flight.insert(key.to_string(), sender);
    spawn_embedding_identity_probe(key.to_string(), cfg);
    IdentityResolutionClaim::Wait(receiver)
}

fn spawn_embedding_identity_probe(cache_key: String, cfg: Config) {
    tokio::spawn(async move {
        let (identity, ttl) = derive_embedding_identity(&cfg).await;
        let sender = {
            let mut cache = embedding_identity_cache()
                .lock()
                .expect("embedding identity cache mutex poisoned");
            cache.entries.insert(
                cache_key.clone(),
                CachedEmbeddingIdentity {
                    identity: identity.clone(),
                    expires_at: Instant::now() + ttl,
                },
            );
            cache.in_flight.remove(&cache_key)
        };
        if let Some(sender) = sender {
            sender.send_replace(Some(identity));
        }
    });
}

async fn wait_for_embedding_identity(
    mut receiver: watch::Receiver<Option<EmbeddingIdentity>>,
) -> EmbeddingIdentity {
    loop {
        if let Some(identity) = receiver.borrow().clone() {
            return identity;
        }
        receiver
            .changed()
            .await
            .expect("embedding identity probe sender dropped before publishing a result");
    }
}

async fn derive_embedding_identity(cfg: &Config) -> (EmbeddingIdentity, Duration) {
    let probe = TeiEmbeddingProvider::new(TeiEmbeddingConfig {
        endpoint: cfg.tei_url.clone(),
        model: EMBEDDING_MODEL_FALLBACK.to_string(),
        dimensions: EMBEDDING_DIMENSIONS_FALLBACK,
        timeout: Duration::from_millis(cfg.tei_request_timeout_ms),
        max_batch_inputs: cfg.tei_max_client_batch_size as u32,
        max_concurrent_requests: cfg.embed_tei_max_concurrent,
        max_in_flight_inputs: cfg.embed_tei_max_in_flight_inputs,
        max_input_tokens: MAX_INPUT_TOKENS,
        max_batch_tokens: tei_client_max_batch_tokens(),
        instruction_support: query_instruction_support(cfg),
        retry_backoff_ms: cfg.embed_tei_retry_backoff_ms,
        max_attempts: tei_max_attempts(cfg),
    });
    match probe.derive_embedding_identity().await {
        Ok(derived) => {
            tracing::info!(
                model = %derived.model,
                dimensions = derived.dimensions,
                "derived embedding model/dimensions from TEI provider"
            );
            let identity = EmbeddingIdentity {
                model: derived.model,
                dimensions: derived.dimensions,
                verified: true,
            };
            (identity, EMBEDDING_IDENTITY_CACHE_TTL)
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                fallback_model = EMBEDDING_MODEL_FALLBACK,
                fallback_dimensions = EMBEDDING_DIMENSIONS_FALLBACK,
                "could not derive embedding identity from TEI provider; using config/default fallback"
            );
            let identity = EmbeddingIdentity {
                model: EMBEDDING_MODEL_FALLBACK.to_string(),
                dimensions: EMBEDDING_DIMENSIONS_FALLBACK,
                verified: false,
            };
            (identity, EMBEDDING_IDENTITY_FALLBACK_TTL)
        }
    }
}

fn embedding_identity_cache_key(cfg: &Config) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        cfg.tei_url,
        EMBEDDING_MODEL_FALLBACK,
        EMBEDDING_DIMENSIONS_FALLBACK,
        cfg.tei_request_timeout_ms,
        cfg.tei_max_client_batch_size,
        cfg.embed_tei_query_instruction_enabled,
        cfg.embed_tei_retry_backoff_ms,
        cfg.tei_max_retries,
    )
}

fn embedding_identity_cache() -> &'static Mutex<EmbeddingIdentityCache> {
    EMBEDDING_IDENTITY_CACHE.get_or_init(|| Mutex::new(EmbeddingIdentityCache::default()))
}

/// Provider id for the target local-source embedding provider.
const EMBEDDING_PROVIDER_ID: &str = "target-local-embed";
/// Provider identity returned by the TEI adapter and persisted in cache rows.
const TEI_RESULT_PROVIDER_ID: &str = "tei";
/// Provider id for the target local-source vector store.
const VECTOR_PROVIDER_ID: &str = "target-local-vector";

/// Fallback embedding model when the TEI provider cannot be reached to derive
/// the live `model_id` (matches the model shipped in the Axon stack).
const EMBEDDING_MODEL_FALLBACK: &str = "Qwen3-Embedding-0.6B";
/// Fallback dense-vector dimensionality when a live probe embed is unavailable.
const EMBEDDING_DIMENSIONS_FALLBACK: u32 = 1024;
/// Max input tokens per embedding request (mirrors the provider capability).
const MAX_INPUT_TOKENS: u32 = 8192;
/// Max tokens pooled into one TEI embed batch.
const MAX_BATCH_TOKENS: u32 = 65_536;

fn tei_client_max_batch_tokens() -> u32 {
    let configured = std::env::var("AXON_TEI_CLIENT_MAX_BATCH_TOKENS").ok();
    tei_client_max_batch_tokens_from_value(configured.as_deref())
}

fn tei_client_max_batch_tokens_from_value(value: Option<&str>) -> u32 {
    value
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(MAX_BATCH_TOKENS)
        .clamp(MAX_INPUT_TOKENS, 1_048_576)
}

struct EmbeddingComposition {
    provider: Arc<dyn EmbeddingProvider>,
    #[cfg(test)]
    cache_store: Option<Arc<SqliteEmbeddingVectorCacheStore>>,
    write_gate: SqliteWriteGate,
}

fn build_embedding_composition(
    cfg: &Config,
    pool: &SqlitePool,
    identity: &EmbeddingIdentity,
) -> EmbeddingComposition {
    let write_gate = SqliteWriteGate::default();
    let raw_provider: Arc<dyn EmbeddingProvider> = Arc::new(build_tei_provider(cfg, identity));
    // The cache key and per-hit identity re-validation are only as good as the
    // resolved identity. An unverified (fallback or stale) identity could label
    // vectors from a different live model with the fallback name, mixing models
    // in one collection — so fail open to the raw provider instead.
    if cfg.embed_cache_enabled && !identity.verified {
        tracing::warn!(
            model = %identity.model,
            dimensions = identity.dimensions,
            "embedding vector cache skipped: embedding identity could not be verified \
             against the TEI provider; using the raw provider without cache decoration"
        );
    }
    let cache_store = (cfg.embed_cache_enabled && identity.verified).then(|| {
        Arc::new(SqliteEmbeddingVectorCacheStore::new(
            pool.clone(),
            write_gate.clone(),
            cfg.embed_cache_max_entries,
        ))
    });
    let provider: Arc<dyn EmbeddingProvider> = match &cache_store {
        Some(store) => Arc::new(CachedEmbeddingProvider::new(
            raw_provider,
            store.clone(),
            cfg.tei_url.as_str(),
            ProviderId::new(TEI_RESULT_PROVIDER_ID),
            identity.model.clone(),
            identity.dimensions,
            query_instruction_support(cfg),
            cfg.embed_cache_max_entries,
        )),
        None => raw_provider,
    };
    EmbeddingComposition {
        provider,
        #[cfg(test)]
        cache_store,
        write_gate,
    }
}

impl TargetLocalSourceRuntime {
    /// Build the production target local-source runtime from [`Config`].
    ///
    /// Constructs the three real data-plane stores:
    /// - the SQLite ledger at a sibling of the jobs DB (`ledger.db`), running
    ///   migrations on connect;
    /// - the Qdrant vector store addressed by `cfg.qdrant_url`;
    /// - the TEI embedding provider addressed by `cfg.tei_url`.
    ///
    /// The `jobs` [`JobStore`] is supplied by the caller (built from the shared
    /// SQLite pool of the job runtime). Vector/embedding constructors do not
    /// connect eagerly; only the ledger `connect` performs I/O (migrations).
    pub async fn from_config(
        cfg: &Config,
        jobs: Arc<dyn JobStore>,
        pool: SqlitePool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // The ledger binds to the SAME pool as the JobStore (one runtime DB), so
        // `jobs.source_id` FKs to `sources(source_id)`. The contract tables are
        // created by the composed cross-crate migration runner
        // (`axon_jobs::migrations::apply_all_migrations`), which applies
        // axon-ledger's own migration set FIRST against this pool; no separate
        // migration here.
        let db_stage_slots = Arc::new(Semaphore::new(source_db_stage_capacity(&pool)));
        let ledger: Arc<dyn axon_ledger::store::LedgerStore> = Arc::new(DbLimitedLedgerStore::new(
            Arc::new(SqliteLedgerStore::from_pool(pool.clone())),
            Arc::clone(&db_stage_slots),
        ));

        let identity = resolve_embedding_identity_with_pool(cfg, &pool).await;
        let EmbeddingComposition {
            provider: embedding_provider,
            #[cfg(test)]
                cache_store: embedding_cache_store,
            write_gate: sqlite_write_gate,
        } = build_embedding_composition(cfg, &pool, &identity);

        let vector_store = build_qdrant_store(cfg)?;

        let embedding_provider_id = ProviderId::new(EMBEDDING_PROVIDER_ID);
        let vector_provider_id = ProviderId::new(VECTOR_PROVIDER_ID);
        let RuntimeSchedulers {
            embedding: embedding_scheduler,
            vector: vector_scheduler,
            fetch: fetch_scheduler,
            render: render_scheduler,
            parse: parse_scheduler,
            graph: graph_scheduler,
            artifact: artifact_scheduler,
        } = build_runtime_schedulers(
            cfg,
            &pool,
            &embedding_provider_id,
            &vector_provider_id,
            sqlite_write_gate.clone(),
        )
        .await?;

        let raw_fetch_provider: Arc<dyn FetchProvider> =
            Arc::new(HttpFetchProvider::new(HttpFetchConfig {
                timeout: Duration::from_millis(cfg.request_timeout_ms.unwrap_or(30_000)),
                max_bytes: cfg.max_page_bytes,
                // General-purpose HTTP fetch boundary — use the general `user_agent`,
                // not the Chrome-specific `chrome_user_agent` (which itself falls
                // back to `user_agent`, not the other way around; see doc comments
                // on both fields in `axon-core/src/config/types/config.rs`).
                user_agent: cfg.user_agent.clone(),
            }));
        let raw_render_provider: Arc<dyn RenderProvider> =
            Arc::new(ChromeRenderProvider::new(ChromeRenderConfig {
                max_concurrent_pages: Some(cfg.render_provider_concurrency),
                chrome_remote_url: cfg.chrome_remote_url.clone(),
                default_timeout_ms: cfg.request_timeout_ms,
            }));
        let fetch_provider: Arc<dyn FetchProvider> = Arc::new(ScheduledFetchProvider::new(
            raw_fetch_provider,
            Arc::new(fetch_scheduler),
            HTTP_FETCH_PROVIDER_ID,
        ));
        let render_provider: Arc<dyn RenderProvider> = Arc::new(ScheduledRenderProvider::new(
            raw_render_provider,
            Arc::new(render_scheduler),
            CHROME_RENDER_PROVIDER_ID,
        ));
        let web_fetch_provider = Arc::clone(&fetch_provider);
        let web_render_provider = Arc::clone(&render_provider);
        let web_source_adapter: Arc<dyn SourceAdapter> = Arc::new(WebSourceAdapter::new(
            web_fetch_provider,
            web_render_provider,
        ));
        let artifact_store = FileArtifactStore::new(cfg.output_dir.join("artifacts"));
        let document_cache = crate::source::document_cache::InProcessDocumentCache::new();
        let artifact_candidate_sink = artifact_candidate_sink_from_env()?;
        let artifact_candidate_outbox = Arc::new(
            crate::artifact_candidate_outbox::ArtifactCandidateOutbox::new(
                cfg.output_dir.join("artifact-candidate-outbox"),
            ),
        );

        Ok(Self {
            jobs,
            ledger,
            embedding_provider,
            vector_store: Arc::new(vector_store),
            embedding_scheduler: Some(Arc::new(embedding_scheduler)),
            vector_scheduler: Some(Arc::new(vector_scheduler)),
            parse_scheduler: Some(Arc::new(parse_scheduler)),
            graph_scheduler: Some(Arc::new(graph_scheduler)),
            artifact_scheduler: Some(Arc::new(artifact_scheduler)),
            #[cfg(test)]
            sqlite_write_gate,
            #[cfg(test)]
            embedding_cache_store,
            embedding_provider_id,
            vector_provider_id,
            embedding_model: identity.model,
            embedding_dimensions: identity.dimensions,
            document_preparer: DocumentPreparer::new(DocumentPreparerConfig {
                markdown_max_chars: cfg.chunking_markdown_max_chars,
                markdown_min_chars: cfg.chunking_markdown_min_chars,
                markdown_overlap_chars: cfg.chunking_overlap_chars,
            }),
            document_prepare_concurrency: cfg.embed_prep_concurrency.max(1),
            embed_pool_max_inputs: cfg.embed_pool_max_inputs.max(1),
            db_stage_slots,
            fetch_provider,
            render_provider,
            web_source_adapter,
            artifact_store: Arc::new(artifact_store),
            document_cache: Arc::new(document_cache),
            artifact_candidate_sink,
            artifact_candidate_outbox: Some(artifact_candidate_outbox),
            source_adapters: Arc::new(tokio::sync::OnceCell::new()),
            enricher: Arc::new(NoopSourceEnricher::new()),
        })
    }
}

fn artifact_candidate_sink_from_env()
-> Result<Arc<dyn ArtifactCandidateSink>, Box<dyn std::error::Error + Send + Sync>> {
    artifact_candidate_sink_from_values(
        std::env::var(DEPOT_URL_ENV).ok(),
        std::env::var(DEPOT_TOKEN_ENV).ok(),
    )
}

fn artifact_candidate_sink_from_values(
    depot_url: Option<String>,
    depot_token: Option<String>,
) -> Result<Arc<dyn ArtifactCandidateSink>, Box<dyn std::error::Error + Send + Sync>> {
    match (depot_url, depot_token) {
        (None, None) => Ok(Arc::new(NoopArtifactCandidateSink)),
        (Some(url), Some(token)) => Ok(Arc::new(DepotArtifactCandidateSink::new(&url, token)?)),
        (Some(_), None) => {
            Err(format!("{DEPOT_TOKEN_ENV} is required when {DEPOT_URL_ENV} is configured").into())
        }
        (None, Some(_)) => {
            Err(format!("{DEPOT_URL_ENV} is required when {DEPOT_TOKEN_ENV} is configured").into())
        }
    }
}

#[cfg(test)]
#[path = "target_runtime_tests.rs"]
mod tests;
