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

mod bulk_load;
mod collection_spec;
pub use bulk_load::drain_bulk_load_transition_workers;
pub(crate) mod commit;
pub mod convert;
mod grpc;
mod http;
mod migration;
mod read;
mod search;
mod store_cache;
mod store_impl;
mod store_trait;
mod upsert;
pub use migration::{VectorMigrationReceipt, migrate_unnamed_collection};

/// Retrieve all stored chunks for a URL through the vector-domain boundary.
pub async fn retrieve_by_url(
    store: &QdrantVectorStore,
    collection: &str,
    target: &str,
    max_points: Option<usize>,
) -> crate::store::Result<QdrantRetrieveByUrlResult> {
    store.retrieve_by_url(collection, target, max_points).await
}

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, OnceLock, Weak};

use axon_api::source::*;
use axon_observe::reservation::{ProviderReservationConfig, ProviderReservationManager};
use qdrant_client::Qdrant;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore};

// Re-export the request-shape conversion helpers exercised by the crate's
// contract tests and any transport that needs the typed builders.
pub use convert::{
    QdrantCollectionSettings, qdrant_collection_request, qdrant_collection_request_with_settings,
    qdrant_filter, qdrant_payload_index_requests, qdrant_upsert_points,
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
const DEFAULT_POINT_BUFFER: usize = 1024;
const DEFAULT_WRITE_PARALLELISM: usize = 2;
const HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES: u32 = 1;
const HEALTH_TRACKER_COOLDOWN_SECS: u64 = 30;

/// Process-wide generation for detected collection specs. Reset may drop and
/// recreate Qdrant through raw HTTP without access to every live store clone;
/// advancing this epoch makes all per-instance entries stale immediately.
static COLLECTION_SPEC_CACHE_EPOCHS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ParallelismKey {
    endpoint: String,
}

#[derive(Debug)]
struct QdrantParallelismGates {
    write_slots: Arc<Semaphore>,
    payload_index_slots: Arc<Semaphore>,
}

static PARALLELISM_GATES: LazyLock<Mutex<HashMap<ParallelismKey, Weak<QdrantParallelismGates>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn shared_parallelism_gates(
    url: &str,
    write_parallelism: usize,
    payload_index_parallelism: usize,
    current: Option<&Arc<QdrantParallelismGates>>,
) -> Arc<QdrantParallelismGates> {
    let key = ParallelismKey {
        endpoint: http::QdrantEndpoint::parse(url).root().to_string(),
    };
    let mut registry = PARALLELISM_GATES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    registry.retain(|_, gates| gates.strong_count() > 0);
    if let Some(gates) = registry.get(&key).and_then(Weak::upgrade) {
        // A sole live owner may safely replace its gate while applying initial
        // configuration. Once another store shares the endpoint, retain the
        // established capacity until every owner drains; this prevents a
        // reload with different knobs from multiplying in-flight operations.
        let sole_current_owner = current.is_some_and(|current| Arc::ptr_eq(current, &gates))
            // One strong reference belongs to the store and one to this
            // temporary upgrade from the registry's weak reference.
            && Arc::strong_count(&gates) == 2;
        if !sole_current_owner {
            return gates;
        }
    }
    let gates = Arc::new(QdrantParallelismGates {
        write_slots: Arc::new(Semaphore::new(write_parallelism.max(1))),
        payload_index_slots: Arc::new(Semaphore::new(payload_index_parallelism.max(1))),
    });
    registry.insert(key, Arc::downgrade(&gates));
    gates
}

/// Qdrant-backed [`VectorStore`](crate::store::VectorStore).
///
/// The `url` is stored verbatim and parsed (with credentials stripped) per
/// request; it is never surfaced in error details.
#[derive(Clone)]
pub struct QdrantVectorStore {
    url: String,
    provider_id: ProviderId,
    point_buffer: usize,
    write_parallelism: usize,
    payload_index_parallelism: usize,
    collection_settings: QdrantCollectionSettings,
    bulk_load_enabled: bool,
    bulk_indexing_threshold: u64,
    normal_indexing_threshold: u64,
    async_writes: bool,
    write_transport: QdrantWriteTransport,
    grpc_client: Option<Arc<Qdrant>>,
    parallelism_gates: Arc<QdrantParallelismGates>,
    health: ProviderReservationManager,
    collection_specs: Arc<RwLock<HashMap<String, (u64, CollectionSpec)>>>,
}

impl QdrantVectorStore {
    /// Build a store for the Qdrant instance at `url`.
    pub fn new(url: impl Into<String>, provider_id: impl Into<String>) -> Self {
        Self::new_with_point_buffer(url, provider_id, DEFAULT_POINT_BUFFER)
    }

    /// Build a store using the configured point-buffer limit for each REST
    /// upsert. Keeping the limit on the store makes the runtime configuration
    /// the sole source of truth instead of duplicating it in the writer.
    pub fn new_with_point_buffer(
        url: impl Into<String>,
        provider_id: impl Into<String>,
        point_buffer: usize,
    ) -> Self {
        let url = url.into();
        let provider_id = ProviderId::new(provider_id);
        let health = ProviderReservationManager::new(ProviderReservationConfig {
            provider_id: provider_id.clone(),
            provider_kind: ProviderKind::Vector,
            capacity: HEALTH_TRACKER_CAPACITY,
            interactive_reserve: 0,
            cooldown_after_failures: HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES,
            cooldown_secs: HEALTH_TRACKER_COOLDOWN_SECS,
        });
        let parallelism_gates = shared_parallelism_gates(
            &url,
            DEFAULT_WRITE_PARALLELISM,
            DEFAULT_WRITE_PARALLELISM,
            None,
        );
        Self {
            url,
            provider_id,
            point_buffer: point_buffer.max(1),
            write_parallelism: DEFAULT_WRITE_PARALLELISM,
            payload_index_parallelism: DEFAULT_WRITE_PARALLELISM,
            collection_settings: QdrantCollectionSettings::default(),
            bulk_load_enabled: false,
            bulk_indexing_threshold: 10_485_760,
            normal_indexing_threshold: 20_000,
            async_writes: false,
            write_transport: QdrantWriteTransport::Rest,
            grpc_client: None,
            parallelism_gates,
            health,
            collection_specs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn point_buffer(&self) -> usize {
        self.point_buffer
    }

    pub fn write_parallelism(&self) -> usize {
        self.write_parallelism
    }

    pub fn payload_index_parallelism(&self) -> usize {
        self.payload_index_parallelism
    }

    pub fn collection_settings(&self) -> QdrantCollectionSettings {
        self.collection_settings
    }

    pub fn write_transport(&self) -> QdrantWriteTransport {
        self.write_transport
    }

    pub(super) async fn write_permit(
        &self,
        stage: ErrorStage,
    ) -> Result<OwnedSemaphorePermit, ApiError> {
        Arc::clone(&self.parallelism_gates.write_slots)
            .acquire_owned()
            .await
            .map_err(|_| {
                ApiError::new(
                    "vector.qdrant.write_admission_closed",
                    stage,
                    "Qdrant write admission gate is closed",
                )
                .with_provider_id(self.provider_id.0.clone())
            })
    }

    pub(super) fn write_slots(&self) -> Arc<Semaphore> {
        Arc::clone(&self.parallelism_gates.write_slots)
    }

    pub(super) fn payload_index_slots(&self) -> Arc<Semaphore> {
        Arc::clone(&self.parallelism_gates.payload_index_slots)
    }

    /// The configured Qdrant URL (may embed credentials — do not log).
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The provider id used in capability snapshots and error attribution.
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    /// Invalidate every live store instance's detected collection specs.
    /// Destructive collection reset must call this after a successful drop.
    pub fn invalidate_collection_spec_cache(url: &str, collection: &str) {
        let mut epochs = collection_spec_cache_epochs()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let epoch = epochs
            .entry(collection_cache_key(url, collection))
            .or_default();
        *epoch = epoch.wrapping_add(1);
    }

    pub(super) fn collection_spec_cache_epoch(&self, collection: &str) -> u64 {
        collection_spec_cache_epochs()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&collection_cache_key(&self.url, collection))
            .copied()
            .unwrap_or(0)
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

/// Apply the runtime point-buffer setting without exposing another provider
/// construction/method operation to orchestration-layer callers.
pub fn configure_point_buffer(store: &mut QdrantVectorStore, point_buffer: usize) {
    store.point_buffer = point_buffer.max(1);
}

pub fn configure_parallelism(
    store: &mut QdrantVectorStore,
    write_parallelism: usize,
    payload_index_parallelism: usize,
) {
    store.write_parallelism = write_parallelism.max(1);
    store.payload_index_parallelism = payload_index_parallelism.max(1);
    store.parallelism_gates = shared_parallelism_gates(
        &store.url,
        store.write_parallelism,
        store.payload_index_parallelism,
        Some(&store.parallelism_gates),
    );
}

pub fn configure_collection_settings(
    store: &mut QdrantVectorStore,
    settings: QdrantCollectionSettings,
) {
    store.collection_settings = settings;
}

pub fn configure_bulk_load(
    store: &mut QdrantVectorStore,
    enabled: bool,
    bulk_indexing_threshold: u64,
    normal_indexing_threshold: u64,
) {
    store.bulk_load_enabled = enabled;
    store.bulk_indexing_threshold = bulk_indexing_threshold;
    store.normal_indexing_threshold = normal_indexing_threshold;
}

pub fn configure_async_writes(store: &mut QdrantVectorStore, enabled: bool) {
    store.async_writes = enabled;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdrantWriteTransport {
    Rest,
    Grpc,
}

pub fn configure_write_transport(
    store: &mut QdrantVectorStore,
    transport: &str,
    grpc_url: Option<&str>,
) -> Result<(), ApiError> {
    match transport.trim().to_ascii_lowercase().as_str() {
        "rest" => {
            configure_rest_transport(store);
            Ok(())
        }
        "grpc" => {
            let url = grpc_url
                .filter(|url| !url.trim().is_empty())
                .ok_or_else(|| {
                    ApiError::new(
                        "vector.qdrant.grpc_url_missing",
                        ErrorStage::Upserting,
                        "QDRANT_GRPC_URL is required when Qdrant transport is grpc",
                    )
                })?;
            configure_grpc_transport(store, url)
        }
        _ => Err(ApiError::new(
            "vector.qdrant.transport_config",
            ErrorStage::Upserting,
            "Qdrant write transport must be rest or grpc",
        )),
    }
}

pub fn configure_grpc_transport(store: &mut QdrantVectorStore, url: &str) -> Result<(), ApiError> {
    let (grpc_url, api_key) = grpc_connection_parts(&store.url, url);
    let grpc_endpoint = http::QdrantEndpoint::parse(url);
    if !grpc_endpoint.transport_is_safe_for_credentials(api_key.is_some()) {
        return Err(ApiError::new(
            "vector.qdrant.insecure_credentials",
            ErrorStage::Authorizing,
            "Qdrant credentials require HTTPS for non-loopback gRPC endpoints",
        ));
    }
    let client = Qdrant::from_url(&grpc_url)
        .api_key(api_key)
        .skip_compatibility_check()
        .build()
        .map_err(|error| {
            ApiError::new(
                "vector.qdrant.grpc_config",
                ErrorStage::Upserting,
                format!("failed to configure Qdrant gRPC transport: {error}"),
            )
        })?;
    store.grpc_client = Some(Arc::new(client));
    store.write_transport = QdrantWriteTransport::Grpc;
    Ok(())
}

fn grpc_connection_parts(rest_url: &str, grpc_url: &str) -> (String, Option<String>) {
    let rest = http::QdrantEndpoint::parse(rest_url);
    let grpc = http::QdrantEndpoint::parse(grpc_url);
    let api_key = grpc
        .api_key()
        .or_else(|| rest.api_key())
        .map(ToOwned::to_owned);
    (grpc.grpc_origin(), api_key)
}

pub fn configure_rest_transport(store: &mut QdrantVectorStore) {
    store.write_transport = QdrantWriteTransport::Rest;
    store.grpc_client = None;
}

impl std::fmt::Debug for QdrantVectorStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QdrantVectorStore")
            .field("provider_id", &self.provider_id)
            .field("point_buffer", &self.point_buffer)
            .field("write_parallelism", &self.write_parallelism)
            .field("write_transport", &self.write_transport)
            .field("bulk_load_enabled", &self.bulk_load_enabled)
            .field("async_writes", &self.async_writes)
            .finish_non_exhaustive()
    }
}

fn collection_spec_cache_epochs() -> &'static Mutex<HashMap<String, u64>> {
    COLLECTION_SPEC_CACHE_EPOCHS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn collection_cache_key(url: &str, collection: &str) -> String {
    format!("{}\0{collection}", url.trim_end_matches('/'))
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
                ApiError::new("provider.cooling", ErrorStage::Observing, cooling.reason)
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
        .get_json(ErrorStage::Observing, &url, "qdrant_health")
        .await
    {
        Ok(_) => (HealthStatus::Healthy, None),
        Err(err) => (HealthStatus::Unavailable, Some(err)),
    }
}
