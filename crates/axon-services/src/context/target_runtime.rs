//! Production composition for [`TargetLocalSourceRuntime`].
//!
//! The `#[cfg(test)]` [`TargetLocalSourceRuntime::new`] constructor (in
//! `context.rs`) wires fakes for unit tests. This module owns the real
//! data-plane composition: it builds the ledger / vector / embedding stores from
//! [`Config`] so long-lived processes (`serve`, `mcp`) carry a working target
//! local-source runtime.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axon_adapters::boundary::{FetchProvider, RenderProvider};
use axon_adapters::providers::chrome_render::{
    CHROME_RENDER_PROVIDER_ID, ChromeRenderConfig, ChromeRenderProvider,
};
use axon_adapters::providers::http_fetch::{
    HTTP_FETCH_PROVIDER_ID, HttpFetchConfig, HttpFetchProvider,
};
use axon_adapters::{NoopSourceEnricher, SourceAdapter, web::WebSourceAdapter};
use axon_api::source::{InstructionSupport, ProviderId, ProviderKind};
use axon_core::boundary::FileArtifactStore;
use axon_core::config::Config;
use axon_embedding::provider::EmbeddingProvider;
use axon_embedding::tei::{TeiEmbeddingConfig, TeiEmbeddingProvider};
use axon_jobs::boundary::JobStore;
use axon_jobs::scheduler::{ProviderCapacityDomain, ProviderScheduler, SchedulerConfig};
use axon_ledger::sqlite::SqliteLedgerStore;
use axon_vectors::qdrant::QdrantVectorStore;
use axon_vectors::store::VectorStore;
use sqlx::SqlitePool;
use tokio::sync::{Semaphore, watch};

use super::{
    TargetLocalSourceRuntime,
    db_limited_ledger::DbLimitedLedgerStore,
    scheduled_web::{ScheduledFetchProvider, ScheduledRenderProvider},
};

/// Read-plane stores plus their provider identity, built from [`Config`].
///
/// This is the minimal seam the read/RAG path (`query`) needs — a vector store
/// and an embedding provider — without the write-plane jobs/ledger wiring. The
/// full [`TargetLocalSourceRuntime::from_config`] reuses the same constructors.
pub struct TargetReadStores {
    pub vector_store: Arc<dyn VectorStore>,
    pub embedding_provider: Arc<dyn EmbeddingProvider>,
    pub embedding_provider_id: ProviderId,
    pub embedding_model: String,
    pub embedding_dimensions: u32,
}

/// Build the read-plane stores (vector store + embedding provider) from
/// [`Config`]. Store constructors do not perform I/O; only the embedding
/// identity is derived from the live TEI provider (with a config/default
/// fallback when it is unreachable).
pub async fn build_read_stores_from_config(cfg: &Config) -> TargetReadStores {
    let identity = resolve_embedding_identity(cfg).await;
    let embedding_provider = build_tei_provider(cfg, &identity);
    let mut vector_store = QdrantVectorStore::new(cfg.qdrant_url.clone(), VECTOR_PROVIDER_ID);
    axon_vectors::qdrant::configure_point_buffer(&mut vector_store, cfg.qdrant_point_buffer);
    axon_vectors::qdrant::configure_parallelism(
        &mut vector_store,
        axon_core::config::parse::tuning::qdrant_upsert_parallelism(),
        axon_core::config::parse::tuning::qdrant_payload_index_parallelism(),
    );
    TargetReadStores {
        vector_store: Arc::new(vector_store),
        embedding_provider: Arc::new(embedding_provider),
        embedding_provider_id: ProviderId::new(EMBEDDING_PROVIDER_ID),
        embedding_model: identity.model,
        embedding_dimensions: identity.dimensions,
    }
}

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
        max_batch_tokens: MAX_BATCH_TOKENS,
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
        max_batch_tokens: MAX_BATCH_TOKENS,
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

/// Durable vector-scheduler capacity. `[providers.vector]` has no equivalent
/// config-driven capacity/reserve knobs yet, so these remain explicit defaults
/// for the shared SQLite vector lane.
const VECTOR_RESERVATION_CAPACITY: u32 = 2;
const VECTOR_RESERVATION_INTERACTIVE_RESERVE: u32 = 1;

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
        let embedding_provider = build_tei_provider(cfg, &identity);

        let mut vector_store = QdrantVectorStore::new(cfg.qdrant_url.clone(), VECTOR_PROVIDER_ID);
        axon_vectors::qdrant::configure_point_buffer(&mut vector_store, cfg.qdrant_point_buffer);
        axon_vectors::qdrant::configure_parallelism(
            &mut vector_store,
            axon_core::config::parse::tuning::qdrant_upsert_parallelism(),
            axon_core::config::parse::tuning::qdrant_payload_index_parallelism(),
        );

        let embedding_provider_id = ProviderId::new(EMBEDDING_PROVIDER_ID);
        let vector_provider_id = ProviderId::new(VECTOR_PROVIDER_ID);
        let scheduler_authority_id = scheduler_authority_id(&cfg.sqlite_path);
        let embedding_scheduler = ProviderScheduler::new(
            pool.clone(),
            ProviderCapacityDomain {
                kind: ProviderKind::Embedding,
                instance_id: embedding_provider_id.0.clone(),
                authority_id: scheduler_authority_id.clone(),
            },
            scheduler_config(
                cfg.embed_tei_max_concurrent as u32,
                cfg.embed_tei_interactive_reserved_requests as u32,
            ),
        )?;
        let vector_scheduler = ProviderScheduler::new(
            pool.clone(),
            ProviderCapacityDomain {
                kind: ProviderKind::Vector,
                instance_id: vector_provider_id.0.clone(),
                authority_id: scheduler_authority_id.clone(),
            },
            scheduler_config(
                VECTOR_RESERVATION_CAPACITY,
                VECTOR_RESERVATION_INTERACTIVE_RESERVE,
            ),
        )?;
        let fetch_scheduler = ProviderScheduler::new(
            pool.clone(),
            ProviderCapacityDomain {
                kind: ProviderKind::Fetch,
                instance_id: HTTP_FETCH_PROVIDER_ID.to_string(),
                authority_id: scheduler_authority_id.clone(),
            },
            scheduler_config(cfg.fetch_provider_concurrency as u32, 1),
        )?;
        let render_scheduler = ProviderScheduler::new(
            pool.clone(),
            ProviderCapacityDomain {
                kind: ProviderKind::Render,
                instance_id: CHROME_RENDER_PROVIDER_ID.to_string(),
                authority_id: scheduler_authority_id.clone(),
            },
            scheduler_config(cfg.render_provider_concurrency as u32, 1),
        )?;
        let parse_scheduler = ProviderScheduler::new(
            pool.clone(),
            ProviderCapacityDomain {
                kind: ProviderKind::Parser,
                instance_id: "source-parse".to_string(),
                authority_id: scheduler_authority_id.clone(),
            },
            scheduler_config(cfg.embed_prep_concurrency.max(1) as u32, 1),
        )?;
        let graph_scheduler = ProviderScheduler::new(
            pool.clone(),
            ProviderCapacityDomain {
                kind: ProviderKind::Graph,
                instance_id: "sqlite-graph".to_string(),
                authority_id: scheduler_authority_id.clone(),
            },
            scheduler_config(source_db_stage_capacity(&pool) as u32, 1),
        )?;
        let artifact_scheduler = ProviderScheduler::new(
            pool.clone(),
            ProviderCapacityDomain {
                kind: ProviderKind::Artifact,
                instance_id: "file-artifact-store".to_string(),
                authority_id: scheduler_authority_id,
            },
            scheduler_config(cfg.batch_concurrency.max(1) as u32, 1),
        )?;
        embedding_scheduler.reconcile().await?;
        vector_scheduler.reconcile().await?;
        fetch_scheduler.reconcile().await?;
        render_scheduler.reconcile().await?;
        parse_scheduler.reconcile().await?;
        graph_scheduler.reconcile().await?;
        artifact_scheduler.reconcile().await?;

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

        Ok(Self {
            jobs,
            ledger,
            embedding_provider: Arc::new(embedding_provider),
            vector_store: Arc::new(vector_store),
            embedding_scheduler: Some(Arc::new(embedding_scheduler)),
            vector_scheduler: Some(Arc::new(vector_scheduler)),
            parse_scheduler: Some(Arc::new(parse_scheduler)),
            graph_scheduler: Some(Arc::new(graph_scheduler)),
            artifact_scheduler: Some(Arc::new(artifact_scheduler)),
            embedding_provider_id,
            vector_provider_id,
            embedding_model: identity.model,
            embedding_dimensions: identity.dimensions,
            document_prepare_concurrency: cfg.embed_prep_concurrency.max(1),
            embed_pool_max_inputs: cfg.embed_pool_max_inputs.max(1),
            db_stage_slots,
            fetch_provider,
            render_provider,
            web_source_adapter,
            artifact_store: Arc::new(artifact_store),
            document_cache: Arc::new(document_cache),
            source_adapters: Arc::new(tokio::sync::OnceCell::new()),
            enricher: Arc::new(NoopSourceEnricher::new()),
        })
    }
}

fn source_db_stage_capacity(pool: &SqlitePool) -> usize {
    // Preserve one pool connection for job heartbeats, provider scheduler
    // control, and other liveness work. A single-connection test pool cannot
    // reserve a spare lane, so it retains one data-plane permit.
    usize::try_from(
        pool.options()
            .get_max_connections()
            .saturating_sub(1)
            .max(1),
    )
    .unwrap_or(1)
}

fn scheduler_authority_id(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let stable = std::fs::canonicalize(&absolute).unwrap_or(absolute);
    format!("sqlite:{}", stable.display())
}

fn scheduler_config(capacity: u32, interactive_reserve: u32) -> SchedulerConfig {
    let capacity = capacity.max(1);
    SchedulerConfig {
        capacity,
        interactive_reserve: interactive_reserve.min(capacity),
        max_entries: capacity.saturating_mul(256).max(256),
        max_units: capacity.saturating_mul(256).max(256),
    }
}

#[cfg(test)]
#[path = "target_runtime_tests.rs"]
mod tests;
