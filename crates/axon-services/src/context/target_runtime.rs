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

/// Vector reservation capacities mirror the `#[cfg(test)]` `new()` constructor
/// so the production vector-store gate behaves identically to the fixtures
/// exercised in tests. `[providers.vector]` has no equivalent config-driven
/// capacity/reserve knobs yet, so these stay hardcoded for the vector pool
/// only — the embedding pool below is now driven entirely by
/// `[providers.embedding]` config (see `embedding_reservation_config`).
const VECTOR_RESERVATION_CAPACITY: u32 = 2;
const VECTOR_RESERVATION_INTERACTIVE_RESERVE: u32 = 1;
const VECTOR_RESERVATION_COOLDOWN_AFTER_FAILURES: u32 = 1;
const VECTOR_RESERVATION_COOLDOWN_SECS: u64 = 30;

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
            embedding_reservations: Arc::new(ProviderReservationManager::new(
                embedding_reservation_config(cfg, embedding_provider_id.clone()),
            )),
            vector_reservations: Arc::new(ProviderReservationManager::new(
                ProviderReservationConfig {
                    provider_id: vector_provider_id.clone(),
                    provider_kind: ProviderKind::Vector,
                    capacity: VECTOR_RESERVATION_CAPACITY,
                    interactive_reserve: VECTOR_RESERVATION_INTERACTIVE_RESERVE,
                    cooldown_after_failures: VECTOR_RESERVATION_COOLDOWN_AFTER_FAILURES,
                    cooldown_secs: VECTOR_RESERVATION_COOLDOWN_SECS,
                },
            )),
            embedding_provider_id,
            vector_provider_id,
            embedding_model: identity.model,
            embedding_dimensions: identity.dimensions,
            fetch_provider: Arc::new(fetch_provider),
            render_provider: Arc::new(render_provider),
        })
    }
}

/// Embedding reservation gate config, driven end-to-end by
/// `[providers.embedding]`: `capacity` from `max-concurrent-requests`
/// (`cfg.embed_tei_max_concurrent`) and `interactive_reserve` from
/// `interactive-reserved-requests` (`cfg.embed_tei_interactive_reserved_requests`).
/// Previously both were hardcoded constants (`capacity: 2, interactive_reserve:
/// 1`) copied from the `#[cfg(test)]` fixture default and completely
/// disconnected from config — see axon_rust-ldozg.
///
/// `background-max-concurrent-requests`/`maintenance-max-concurrent-requests`
/// are NOT separately enforced here: `ProviderReservationManager` only
/// implements a two-tier interactive/non-interactive gate (this `capacity`
/// minus this `interactive_reserve`), so background and maintenance
/// priorities currently share one combined non-interactive ceiling. See the
/// doc comment on `Config::embed_tei_background_max_concurrent_requests`.
fn embedding_reservation_config(
    cfg: &Config,
    provider_id: ProviderId,
) -> ProviderReservationConfig {
    ProviderReservationConfig {
        provider_id,
        provider_kind: ProviderKind::Embedding,
        capacity: cfg.embed_tei_max_concurrent as u32,
        interactive_reserve: cfg.embed_tei_interactive_reserved_requests as u32,
        cooldown_after_failures: cfg.embed_tei_cooldown_after_failures as u32,
        cooldown_secs: cfg.embed_tei_cooldown_secs,
    }
}

#[cfg(test)]
#[path = "target_runtime_tests.rs"]
mod tests;
