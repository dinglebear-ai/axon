//! Production composition for [`TargetLocalSourceRuntime`].
//!
//! The `#[cfg(test)]` [`TargetLocalSourceRuntime::new`] constructor (in
//! `context.rs`) wires fakes for unit tests. This module owns the real
//! data-plane composition: it builds the ledger / vector / embedding stores from
//! [`Config`] so long-lived processes (`serve`, `mcp`) carry a working target
//! local-source runtime.

use std::sync::Arc;
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
use axon_jobs::scheduler::{ProviderScheduler, SqliteWriteGate};
use axon_ledger::sqlite::SqliteLedgerStore;
use axon_vectors::store::VectorStore;
use sqlx::SqlitePool;
use tokio::sync::{Semaphore, watch};

mod embedding_identity_cache;
mod read_stores;
mod schedulers;

#[cfg(test)]
pub(crate) use embedding_identity_cache::abort_embedding_identity_probe;
pub use embedding_identity_cache::invalidate_embedding_identity_cache;
pub(crate) use embedding_identity_cache::{
    resolve_embedding_identity, resolve_embedding_identity_with_pool,
};
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
        max_batch_tokens: cfg.embed_tei_max_batch_tokens,
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
pub(crate) struct EmbeddingIdentity {
    pub(crate) model: String,
    pub(crate) dimensions: u32,
    pub(crate) verified: bool,
}

pub(crate) struct VerifiedEmbeddingPlane {
    pub(crate) provider: Arc<dyn EmbeddingProvider>,
    pub(crate) identity: EmbeddingIdentity,
}

struct DeferredEmbeddingProvider {
    receiver: watch::Receiver<Option<Arc<VerifiedEmbeddingPlane>>>,
}

#[async_trait::async_trait]
impl EmbeddingProvider for DeferredEmbeddingProvider {
    async fn embed(
        &self,
        batch: axon_api::source::EmbeddingBatch,
    ) -> Result<axon_api::source::EmbeddingResult, axon_api::source::ApiError> {
        wait_for_verified_plane(self.receiver.clone())
            .await?
            .provider
            .embed(batch)
            .await
    }

    async fn capabilities(
        &self,
    ) -> Result<axon_api::source::ProviderCapability, axon_api::source::ApiError> {
        wait_for_verified_plane(self.receiver.clone())
            .await?
            .provider
            .capabilities()
            .await
    }
}

async fn wait_for_verified_plane(
    mut receiver: watch::Receiver<Option<Arc<VerifiedEmbeddingPlane>>>,
) -> Result<Arc<VerifiedEmbeddingPlane>, axon_api::source::ApiError> {
    loop {
        if let Some(plane) = receiver.borrow().clone() {
            return Ok(plane);
        }
        receiver.changed().await.map_err(|_| {
            axon_api::source::ApiError::new(
                "embedding.identity_unavailable",
                axon_error::ErrorStage::Embedding,
                "embedding identity verification task stopped",
            )
        })?;
    }
}

const EMBEDDING_IDENTITY_CACHE_TTL: Duration = Duration::from_secs(30);
const EMBEDDING_IDENTITY_FALLBACK_TTL: Duration = Duration::from_secs(5);
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
        max_batch_tokens: cfg.embed_tei_max_batch_tokens,
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
struct EmbeddingComposition {
    provider: Arc<dyn EmbeddingProvider>,
}

fn build_embedding_composition(
    cfg: &Config,
    identity: &EmbeddingIdentity,
    cache_store: Option<Arc<SqliteEmbeddingVectorCacheStore>>,
) -> EmbeddingComposition {
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
    let cache_store = identity.verified.then_some(cache_store).flatten();
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
    EmbeddingComposition { provider }
}

async fn build_target_runtime(
    cfg: Config,
    jobs: Arc<dyn JobStore>,
    pool: SqlitePool,
) -> Result<TargetLocalSourceRuntime, Box<dyn std::error::Error + Send + Sync>> {
    // The composed migrations prepare ledger tables in this shared job-runtime pool.
    let db_stage_slots = Arc::new(Semaphore::new(source_db_stage_capacity(&pool)));
    let ledger: Arc<dyn axon_ledger::store::LedgerStore> = Arc::new(DbLimitedLedgerStore::new(
        Arc::new(SqliteLedgerStore::from_pool(pool.clone())),
        Arc::clone(&db_stage_slots),
    ));

    let sqlite_write_gate = SqliteWriteGate::default();
    let embedding_cache_store = cfg.embed_cache_enabled.then(|| {
        Arc::new(SqliteEmbeddingVectorCacheStore::new(
            pool.clone(),
            sqlite_write_gate.clone(),
            cfg.embed_cache_max_entries,
        ))
    });
    let (verified_sender, verified_embedding) = watch::channel(None);
    let identity_cfg = cfg.clone();
    let identity_pool = pool.clone();
    let identity_cache_store = embedding_cache_store.clone();
    tokio::spawn(async move {
        let identity = resolve_embedding_identity_with_pool(&identity_cfg, &identity_pool).await;
        let composition =
            build_embedding_composition(&identity_cfg, &identity, identity_cache_store);
        verified_sender.send_replace(Some(Arc::new(VerifiedEmbeddingPlane {
            provider: composition.provider,
            identity,
        })));
    });
    let embedding_provider: Arc<dyn EmbeddingProvider> = Arc::new(DeferredEmbeddingProvider {
        receiver: verified_embedding.clone(),
    });

    let vector_store = build_qdrant_store(&cfg)?;

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
        &cfg,
        &pool,
        &embedding_provider_id,
        &vector_provider_id,
        sqlite_write_gate.clone(),
    )
    .await?;

    let (fetch_provider, render_provider, web_source_adapter) =
        build_scheduled_web_boundaries(&cfg, fetch_scheduler, render_scheduler);
    let artifact_store = FileArtifactStore::new(cfg.output_dir.join("artifacts"));
    let document_cache = crate::source::document_cache::InProcessDocumentCache::new();
    let artifact_candidate_sink = artifact_candidate_sink_from_env()?;
    let artifact_candidate_outbox = Arc::new(
        crate::artifact_candidate_outbox::ArtifactCandidateOutbox::new(
            cfg.output_dir.join("artifact-candidate-outbox"),
        ),
    );

    let runtime = TargetLocalSourceRuntime {
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
        embedding_model: EMBEDDING_MODEL_FALLBACK.to_string(),
        embedding_dimensions: EMBEDDING_DIMENSIONS_FALLBACK,
        verified_embedding,
        document_preparer: DocumentPreparer::new(DocumentPreparerConfig {
            markdown_max_chars: cfg.chunking_markdown_max_chars,
            markdown_min_chars: cfg.chunking_markdown_min_chars,
            markdown_overlap_chars: cfg.chunking_overlap_chars,
        }),
        document_prepare_concurrency: cfg.embed_prep_concurrency.max(1),
        document_prepare_max_in_flight_bytes: cfg.embed_prep_max_in_flight_bytes,
        embed_pool_max_inputs: cfg.embed_pool_max_inputs.max(1),
        embed_prepared_byte_budget: cfg.embed_prepared_byte_budget.max(1),
        document_batch_size: cfg.document_batch_size,
        document_status_batch_size: cfg.document_status_batch_size,
        embed_scheduler_enabled: cfg.embed_scheduler_enabled,
        embed_scheduler_flush_delay: Duration::from_millis(cfg.embed_scheduler_flush_ms),
        vector_upsert_embed_overlap: cfg.vector_upsert_embed_overlap,
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
    };
    crate::reserved_call::replay_artifact_cleanup_journals(&runtime).await;
    Ok(runtime)
}

fn build_scheduled_web_boundaries(
    cfg: &Config,
    fetch_scheduler: ProviderScheduler,
    render_scheduler: ProviderScheduler,
) -> (
    Arc<dyn FetchProvider>,
    Arc<dyn RenderProvider>,
    Arc<dyn SourceAdapter>,
) {
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
    let web_source_adapter: Arc<dyn SourceAdapter> = Arc::new(WebSourceAdapter::new(
        Arc::clone(&fetch_provider),
        Arc::clone(&render_provider),
    ));
    (fetch_provider, render_provider, web_source_adapter)
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
        build_target_runtime(cfg.clone(), jobs, pool).await
    }

    pub(crate) async fn from_config_owned(
        cfg: Config,
        jobs: Arc<dyn JobStore>,
        pool: SqlitePool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        build_target_runtime(cfg, jobs, pool).await
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
