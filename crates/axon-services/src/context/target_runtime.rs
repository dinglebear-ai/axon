//! Production composition for [`TargetLocalSourceRuntime`].
//!
//! The `#[cfg(test)]` [`TargetLocalSourceRuntime::new`] constructor (in
//! `context.rs`) wires fakes for unit tests. This module owns the real
//! data-plane composition: it builds the ledger / vector / embedding stores from
//! [`Config`] so long-lived processes (`serve`, `mcp`) carry a working target
//! local-source runtime.

use std::sync::Arc;
use std::time::Duration;

use axon_adapters::providers::chrome_render::{ChromeRenderConfig, ChromeRenderProvider};
use axon_adapters::providers::http_fetch::{HttpFetchConfig, HttpFetchProvider};
use axon_api::source::{InstructionSupport, ProviderId, ProviderKind};
use axon_core::config::Config;
use axon_embedding::provider::EmbeddingProvider;
use axon_embedding::reservation::{ProviderReservationConfig, ProviderReservationManager};
use axon_embedding::tei::{TeiEmbeddingConfig, TeiEmbeddingProvider};
use axon_jobs::boundary::JobStore;
use axon_ledger::sqlite::SqliteLedgerStore;
use axon_vectors::qdrant::QdrantVectorStore;
use axon_vectors::store::VectorStore;
use sqlx::SqlitePool;

use super::TargetLocalSourceRuntime;

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
    let vector_store = QdrantVectorStore::new(cfg.qdrant_url.clone(), VECTOR_PROVIDER_ID);
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
        max_input_tokens: MAX_INPUT_TOKENS,
        max_batch_tokens: MAX_BATCH_TOKENS,
        instruction_support: InstructionSupport::QueryAndDocument,
    })
}

/// Resolved embedding model + dimensions used to size the collection, seed the
/// provider, and stamp vector payloads.
struct EmbeddingIdentity {
    model: String,
    dimensions: u32,
}

/// Resolve the embedding model + dimensions from the live TEI endpoint (`/info`
/// for `model_id`, a probe embed for dimensions). Builds a probe provider seeded
/// with the fallback identity purely to issue the derivation requests. Falls
/// back to the configured defaults when the provider is unreachable, so a
/// fire-and-forget CLI enqueue or an offline TEI never blocks store construction.
async fn resolve_embedding_identity(cfg: &Config) -> EmbeddingIdentity {
    let probe = TeiEmbeddingProvider::new(TeiEmbeddingConfig {
        endpoint: cfg.tei_url.clone(),
        model: EMBEDDING_MODEL_FALLBACK.to_string(),
        dimensions: EMBEDDING_DIMENSIONS_FALLBACK,
        timeout: Duration::from_millis(cfg.tei_request_timeout_ms),
        max_batch_inputs: cfg.tei_max_client_batch_size as u32,
        max_input_tokens: MAX_INPUT_TOKENS,
        max_batch_tokens: MAX_BATCH_TOKENS,
        instruction_support: InstructionSupport::QueryAndDocument,
    });
    match probe.derive_embedding_identity().await {
        Ok(derived) => {
            tracing::info!(
                model = %derived.model,
                dimensions = derived.dimensions,
                "derived embedding model/dimensions from TEI provider"
            );
            EmbeddingIdentity {
                model: derived.model,
                dimensions: derived.dimensions,
            }
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                fallback_model = EMBEDDING_MODEL_FALLBACK,
                fallback_dimensions = EMBEDDING_DIMENSIONS_FALLBACK,
                "could not derive embedding identity from TEI provider; using config/default fallback"
            );
            EmbeddingIdentity {
                model: EMBEDDING_MODEL_FALLBACK.to_string(),
                dimensions: EMBEDDING_DIMENSIONS_FALLBACK,
            }
        }
    }
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

/// Reservation capacities mirror the `#[cfg(test)]` `new()` constructor so the
/// production runtime behaves identically to the fixtures exercised in tests.
const RESERVATION_CAPACITY: u32 = 2;
const RESERVATION_INTERACTIVE_RESERVE: u32 = 1;
const RESERVATION_COOLDOWN_AFTER_FAILURES: u32 = 1;
const RESERVATION_COOLDOWN_SECS: u64 = 30;

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
        let ledger = SqliteLedgerStore::from_pool(pool);

        let identity = resolve_embedding_identity(cfg).await;
        let embedding_provider = build_tei_provider(cfg, &identity);

        let vector_store = QdrantVectorStore::new(cfg.qdrant_url.clone(), VECTOR_PROVIDER_ID);

        let embedding_provider_id = ProviderId::new(EMBEDDING_PROVIDER_ID);
        let vector_provider_id = ProviderId::new(VECTOR_PROVIDER_ID);

        let fetch_provider = HttpFetchProvider::new(HttpFetchConfig {
            timeout: Duration::from_millis(cfg.request_timeout_ms.unwrap_or(30_000)),
            max_bytes: cfg.max_page_bytes,
            user_agent: cfg.chrome_user_agent.clone(),
        });
        let render_provider = ChromeRenderProvider::new(ChromeRenderConfig {
            chrome_remote_url: cfg.chrome_remote_url.clone(),
            default_timeout_ms: cfg.request_timeout_ms,
        });

        Ok(Self {
            jobs,
            ledger: Arc::new(ledger),
            embedding_provider: Arc::new(embedding_provider),
            vector_store: Arc::new(vector_store),
            embedding_reservations: Arc::new(ProviderReservationManager::new(reservation_config(
                embedding_provider_id.clone(),
                ProviderKind::Embedding,
            ))),
            vector_reservations: Arc::new(ProviderReservationManager::new(reservation_config(
                vector_provider_id.clone(),
                ProviderKind::Vector,
            ))),
            embedding_provider_id,
            vector_provider_id,
            embedding_model: identity.model,
            embedding_dimensions: identity.dimensions,
            fetch_provider: Arc::new(fetch_provider),
            render_provider: Arc::new(render_provider),
        })
    }
}

/// Reservation config mirroring the capacities of the `#[cfg(test)]` `new()`.
fn reservation_config(
    provider_id: ProviderId,
    provider_kind: ProviderKind,
) -> ProviderReservationConfig {
    ProviderReservationConfig {
        provider_id,
        provider_kind,
        capacity: RESERVATION_CAPACITY,
        interactive_reserve: RESERVATION_INTERACTIVE_RESERVE,
        cooldown_after_failures: RESERVATION_COOLDOWN_AFTER_FAILURES,
        cooldown_secs: RESERVATION_COOLDOWN_SECS,
    }
}

#[cfg(test)]
#[path = "target_runtime_tests.rs"]
mod tests;
