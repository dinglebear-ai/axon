//! Live Qdrant vector store over the REST API.
//!
//! [`QdrantVectorStore`] implements [`crate::store::VectorStore`] against a
//! Qdrant instance addressed by the URL passed to [`QdrantVectorStore::new`].
//! Submodules split the concern:
//! - [`http`] — reqwest transport with credential redaction and retries.
//! - [`convert`] — request-shape conversion (`qdrant_client`-typed validators
//!   plus the REST JSON bodies actually sent).
//! - [`store_impl`] — the `VectorStore` trait implementation.
//! - [`search`] — `/points/query` named-dense and dense+bm42 RRF hybrid.
//! - [`commit`] — generation-aware publish (`mark_*_committed`).
//! - [`read`] — raw-payload read/query primitives ported from legacy
//!   `axon-vector` (facet, scroll, retrieve-by-url, canonical/prefix purge).

pub(crate) mod commit;
pub mod convert;
mod http;
mod read;
mod search;
mod store_impl;

use axon_api::source::*;
use axon_observe::reservation::{ProviderReservationConfig, ProviderReservationManager};

// Re-export the request-shape conversion helpers exercised by the crate's
// contract tests and any transport that needs the typed builders.
pub use convert::{
    QdrantCollectionSettings, qdrant_collection_request, qdrant_filter,
    qdrant_payload_index_requests, qdrant_upsert_points,
};
// Read/query primitives — see `read.rs`. Methods themselves are inherent
// `impl QdrantVectorStore` blocks defined inside the submodule; only the new
// public types and the free-standing render helper need re-exporting here.
pub use read::{
    QdrantRetrieveByUrlResult, QdrantScrolledPoint, QdrantUrlVariantError, ScrollPage,
    render_full_doc_from_points,
};

#[allow(dead_code)]
pub const MODULE_NAME: &str = "qdrant";

/// Self-tracked health/cooldown capacity, independent of any scheduler-side
/// reservation pool a caller may layer on top (mirrors
/// `axon_embedding::tei::TeiEmbeddingProvider`'s `health` field). Sized
/// generously — it exists purely to fold live write/delete/search outcomes
/// into `capabilities()`, not to gate concurrency.
const HEALTH_TRACKER_CAPACITY: u32 = 1_000_000;
const HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES: u32 = 1;
const HEALTH_TRACKER_COOLDOWN_SECS: u64 = 30;

/// Qdrant-backed [`VectorStore`](crate::store::VectorStore).
///
/// The `url` is stored verbatim and parsed (with credentials stripped) per
/// request; it is never surfaced in error details.
#[derive(Debug, Clone)]
pub struct QdrantVectorStore {
    url: String,
    provider_id: ProviderId,
    health: ProviderReservationManager,
}

impl QdrantVectorStore {
    /// Build a store for the Qdrant instance at `url`.
    pub fn new(url: impl Into<String>, provider_id: impl Into<String>) -> Self {
        let provider_id = ProviderId::new(provider_id);
        let health = ProviderReservationManager::new(ProviderReservationConfig {
            provider_id: provider_id.clone(),
            provider_kind: ProviderKind::Vector,
            capacity: HEALTH_TRACKER_CAPACITY,
            interactive_reserve: 0,
            cooldown_after_failures: HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES,
            cooldown_secs: HEALTH_TRACKER_COOLDOWN_SECS,
        });
        Self {
            url: url.into(),
            provider_id,
            health,
        }
    }

    /// The configured Qdrant URL (may embed credentials — do not log).
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The provider id used in capability snapshots and error attribution.
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    /// Fold a fallible operation's outcome into the live health tracker,
    /// returning the result unchanged. Every [`crate::store::VectorStore`]
    /// trait method routes its result through this so `capabilities()`
    /// reflects real write/delete/search failures instead of only the
    /// separate root-liveness probe in [`capability_snapshot`].
    pub(crate) async fn track<T>(&self, result: Result<T, ApiError>) -> Result<T, ApiError> {
        match &result {
            Ok(_) => self.health.record_success().await,
            Err(err) => {
                self.health
                    .record_failure(err.code.0.clone(), err.retryable)
                    .await;
            }
        }
        result
    }
}

/// Build the capability snapshot for this store.
///
/// Reports live health from two sources folded together: a root-liveness
/// probe (unreachable server → `Unavailable`) and the store's own
/// `record_success`/`record_failure` tracker fed by every
/// [`VectorStore`](crate::store::VectorStore) call via
/// [`QdrantVectorStore::track`] (repeated write/delete/search failures →
/// `Cooling`, with a live `cooldown_until`). The tracker wins when it reports
/// `Cooling` or `Unavailable` — those reflect *our own* scheduling decision
/// even if a fresh probe happens to succeed. Declares dense + sparse + hybrid
/// + generation-publish support.
pub(crate) async fn capability_snapshot(store: &QdrantVectorStore) -> ProviderCapability {
    let (probed_health, probe_error) = probe_health(store).await;
    let tracked_health = store.health.health().await;
    let cooldown_until = store.health.cooldown_until().await;
    let (health, last_error) = if matches!(
        tracked_health,
        HealthStatus::Cooling | HealthStatus::Unavailable
    ) {
        let last_error = store
            .health
            .cooling_snapshot()
            .await
            .map(|cooling| {
                ApiError::new(
                    "provider.cooling",
                    axon_error::ErrorStage::Observing,
                    cooling.reason,
                )
                .with_provider_id(store.provider_id().0.clone())
            })
            .or(probe_error);
        (tracked_health, last_error)
    } else {
        (probed_health, probe_error)
    };
    ProviderCapability {
        provider_id: store.provider_id().clone(),
        provider_kind: ProviderKind::Vector,
        implementation: "qdrant".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        health,
        limits: ProviderLimits::default(),
        features: vec![
            "dense".to_string(),
            "sparse".to_string(),
            "hybrid".to_string(),
            "payload_filters".to_string(),
            "payload_indexes".to_string(),
            "generation_publish".to_string(),
        ],
        cooldown_until,
        last_error,
        reservation_policy: ReservationPolicy {
            supports_reservations: false,
            queue_policy: QueuePolicy::Fifo,
            interactive_reserve: 0,
            cooldown_after_failures: HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES,
            cooldown_secs: HEALTH_TRACKER_COOLDOWN_SECS,
            retry_backoff_ms: None,
        },
        reservation_state: ReservationStateSnapshot {
            queued: 0,
            active: 0,
            available_units: 0,
            oldest_queued_ms: None,
            priority_breakdown: Default::default(),
            states: Vec::new(),
        },
        cost_class: ProviderCostClass::Internal,
        degraded_modes: Vec::new(),
        fake_overrides_supported: false,
        embedding: None,
        llm: None,
        vector_store: Some(VectorStoreCapability {
            dense: true,
            sparse: true,
            hybrid: true,
            payload_filters: true,
            payload_indexes: Vec::new(),
            delete_by_filter: true,
            generation_publish: true,
            collection_aliases: true,
            consistency: VectorConsistency::Strong,
        }),
        fetch: None,
        render: None,
        credential: None,
    }
}

/// Probe the Qdrant root for liveness. Any transport/status failure downgrades
/// health to `Unavailable` and carries a redaction-safe last error.
async fn probe_health(store: &QdrantVectorStore) -> (HealthStatus, Option<ApiError>) {
    let http = match store.http() {
        Ok(http) => http,
        Err(err) => return (HealthStatus::Unavailable, Some(err)),
    };
    // `GET /` returns a small JSON envelope (`{"title":...,"version":...}`).
    let url = format!("{}/", http.endpoint().root());
    match http
        .get_json(axon_error::ErrorStage::Observing, &url, "qdrant_health")
        .await
    {
        Ok(_) => (HealthStatus::Healthy, None),
        Err(err) => (HealthStatus::Unavailable, Some(err)),
    }
}
