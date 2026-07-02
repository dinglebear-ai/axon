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

pub(crate) mod commit;
pub mod convert;
mod http;
mod search;
mod store_impl;

use axon_api::source::*;

// Re-export the request-shape conversion helpers exercised by the crate's
// contract tests and any transport that needs the typed builders.
pub use convert::{
    QdrantCollectionSettings, qdrant_collection_request, qdrant_filter,
    qdrant_payload_index_requests, qdrant_upsert_points,
};

#[allow(dead_code)]
pub const MODULE_NAME: &str = "qdrant";

/// Qdrant-backed [`VectorStore`](crate::store::VectorStore).
///
/// The `url` is stored verbatim and parsed (with credentials stripped) per
/// request; it is never surfaced in error details.
#[derive(Debug, Clone)]
pub struct QdrantVectorStore {
    url: String,
    provider_id: ProviderId,
}

impl QdrantVectorStore {
    /// Build a store for the Qdrant instance at `url`.
    pub fn new(url: impl Into<String>, provider_id: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            provider_id: ProviderId::new(provider_id),
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
}

/// Build the capability snapshot for this store.
///
/// Reports live health by probing the Qdrant root endpoint; declares dense +
/// sparse + hybrid + generation-publish support.
pub(crate) async fn capability_snapshot(store: &QdrantVectorStore) -> ProviderCapability {
    let (health, last_error) = probe_health(store).await;
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
        cooldown_until: None,
        last_error,
        reservation_policy: ReservationPolicy {
            supports_reservations: false,
            queue_policy: QueuePolicy::Fifo,
            interactive_reserve: 0,
            cooldown_after_failures: 1,
            cooldown_secs: 30,
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
