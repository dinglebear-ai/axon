//! Vector store boundary and deterministic fake.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axon_api::source::*;
use tokio::sync::Mutex;

use crate::collection::{
    check_collection_drift, normalize_collection_spec, validate_collection_spec,
};
use crate::filter::{
    matches_delete_selector, matches_search_filters, selector_collection, validate_delete_selector,
    validate_search_filters,
};
use crate::validation::validate_upsert_batch;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn ensure_collection(&self, spec: CollectionSpec) -> Result<()>;
    async fn upsert(&self, batch: VectorPointBatch) -> Result<VectorStoreWriteResult>;
    async fn delete(&self, selector: VectorDeleteSelector) -> Result<VectorStoreDeleteResult>;
    async fn search(&self, request: VectorSearchRequest) -> Result<VectorSearchResult>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}

#[derive(Debug, Clone)]
pub struct FakeVectorStore {
    provider_id: ProviderId,
    health: HealthStatus,
    mode: FakeVectorMode,
    state: Arc<Mutex<FakeVectorState>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeVectorMode {
    Success,
    Unavailable,
    Timeout,
    RateLimited,
    Fatal,
    PartialFailure,
    SlowWrite,
}

#[derive(Debug, Default)]
struct FakeVectorState {
    collections: BTreeMap<String, CollectionSpec>,
    points: BTreeMap<String, BTreeMap<VectorPointId, VectorPoint>>,
    calls: Vec<&'static str>,
}

impl FakeVectorStore {
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: ProviderId::new(provider_id),
            health: HealthStatus::Healthy,
            mode: FakeVectorMode::Success,
            state: Arc::new(Mutex::new(FakeVectorState::default())),
        }
    }

    pub fn with_health(mut self, health: HealthStatus) -> Self {
        self.health = health;
        self
    }

    pub fn with_mode(mut self, mode: FakeVectorMode) -> Self {
        self.mode = mode;
        self
    }

    pub async fn calls(&self) -> Vec<&'static str> {
        self.state.lock().await.calls.clone()
    }

    pub async fn collection_spec(&self, collection: &str) -> Option<CollectionSpec> {
        self.state.lock().await.collections.get(collection).cloned()
    }

    fn mode_error(&self) -> Option<ApiError> {
        self.mode_error_for(axon_error::ErrorStage::Upserting)
    }

    fn mode_error_for(&self, stage: axon_error::ErrorStage) -> Option<ApiError> {
        match self.mode {
            FakeVectorMode::Success
            | FakeVectorMode::PartialFailure
            | FakeVectorMode::SlowWrite => None,
            FakeVectorMode::Unavailable => Some(
                ApiError::new("provider.unavailable", stage, "vector store unavailable")
                    .with_provider_id(&self.provider_id.0),
            ),
            FakeVectorMode::Timeout => fake_provider_mode_error(
                FakeProviderModeState::Timeout,
                &self.provider_id.0,
                stage,
                "vector store",
            ),
            FakeVectorMode::RateLimited => fake_provider_mode_error(
                FakeProviderModeState::RateLimited,
                &self.provider_id.0,
                stage,
                "vector store",
            ),
            FakeVectorMode::Fatal => fake_provider_mode_error(
                FakeProviderModeState::Fatal,
                &self.provider_id.0,
                stage,
                "vector store",
            ),
        }
    }

    fn mode_state(&self) -> FakeProviderModeState {
        match self.mode {
            FakeVectorMode::Success
            | FakeVectorMode::PartialFailure
            | FakeVectorMode::SlowWrite => FakeProviderModeState::Success,
            FakeVectorMode::Unavailable => FakeProviderModeState::Fatal,
            FakeVectorMode::Timeout => FakeProviderModeState::Timeout,
            FakeVectorMode::RateLimited => FakeProviderModeState::RateLimited,
            FakeVectorMode::Fatal => FakeProviderModeState::Fatal,
        }
    }

    fn capability_state(&self) -> FakeProviderCapabilityState {
        let mut state = fake_provider_capability_state(
            self.mode_state(),
            &self.provider_id.0,
            axon_error::ErrorStage::Upserting,
            "vector store",
        );
        if self.mode == FakeVectorMode::Unavailable {
            state.health = HealthStatus::Unavailable;
            state.last_error = self.mode_error();
        }
        if self.health != HealthStatus::Healthy {
            state.health = self.health;
        }
        state
    }
}

#[async_trait]
impl VectorStore for FakeVectorStore {
    async fn ensure_collection(&self, spec: CollectionSpec) -> Result<()> {
        let mut state = self.state.lock().await;
        state.calls.push("ensure_collection");
        if let Some(err) = self.mode_error() {
            return Err(err);
        }
        let spec = normalize_collection_spec(spec);
        validate_collection_spec(&spec)?;
        if let Some(existing) = state.collections.get(&spec.collection) {
            check_collection_drift(existing, &spec)?;
        } else {
            state.collections.insert(spec.collection.clone(), spec);
        }
        Ok(())
    }

    async fn upsert(&self, batch: VectorPointBatch) -> Result<VectorStoreWriteResult> {
        let mut state = self.state.lock().await;
        state.calls.push("upsert");
        if let Some(err) = self.mode_error() {
            return Err(err);
        }
        let mut batch = batch;
        let slow_write = self.mode == FakeVectorMode::SlowWrite;
        let spec = state.collections.get(&batch.collection).ok_or_else(|| {
            ApiError::new(
                "vector.collection_not_found",
                axon_error::ErrorStage::Upserting,
                format!("collection {} has not been ensured", batch.collection),
            )
        })?;
        let batch_sparse = validate_upsert_batch(spec, &batch, axon_error::ErrorStage::Upserting)?;
        for point in &mut batch.points {
            if point.sparse_vector.is_none()
                && let Some(sparse) = batch_sparse.get(&point.chunk_id.0)
            {
                point.sparse_vector = Some(sparse.clone());
            }
        }
        let collection = state.points.entry(batch.collection.clone()).or_default();
        let points_attempted = batch.points.len() as u64;
        let partial_failure = self.mode == FakeVectorMode::PartialFailure;
        let mut points_written = 0;
        for point in batch.points {
            collection.insert(point.point_id.clone(), point);
            points_written += 1;
            if partial_failure {
                break;
            }
        }
        drop(state);
        if slow_write {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if partial_failure {
            return Err(ApiError::new(
                "provider.partial_failure",
                axon_error::ErrorStage::Upserting,
                format!("fake vector store wrote {points_written} of {points_attempted} points"),
            )
            .with_provider_id(&self.provider_id.0));
        }
        Ok(VectorStoreWriteResult {
            header: stage_header(PipelinePhase::Upserting),
            collection: batch.collection,
            points_attempted,
            points_written,
            payload_indexes_created: batch
                .payload_indexes
                .into_iter()
                .map(|index| index.field_name)
                .collect(),
            usage: ProviderUsage {
                input_tokens: None,
                output_tokens: None,
                requests: 1,
                duration_ms: 0,
            },
        })
    }

    async fn delete(&self, selector: VectorDeleteSelector) -> Result<VectorStoreDeleteResult> {
        let mut state = self.state.lock().await;
        state.calls.push("delete");
        if let Some(err) = self.mode_error_for(axon_error::ErrorStage::Cleaning) {
            return Err(err);
        }
        validate_delete_selector(&selector)?;
        let collection = selector_collection(&selector).to_string();
        state.collection_spec(&collection, axon_error::ErrorStage::Cleaning)?;
        let points = state.points.entry(collection.clone()).or_default();
        let before = points.len();
        points.retain(|_, point| !matches_delete_selector(point, &selector));
        Ok(delete_result(
            collection,
            before.saturating_sub(points.len()) as u64,
        ))
    }

    async fn search(&self, request: VectorSearchRequest) -> Result<VectorSearchResult> {
        let mut state = self.state.lock().await;
        state.calls.push("search");
        if let Some(err) = self.mode_error_for(axon_error::ErrorStage::Retrieving) {
            return Err(err);
        }
        let query_vector = request.dense_vector.as_deref().ok_or_else(|| {
            ApiError::new(
                "vector.missing_query_vector",
                axon_error::ErrorStage::Retrieving,
                "fake vector store search requires a dense query vector",
            )
        })?;
        let query_sparse = request.sparse_vector.as_ref();
        let spec =
            state.collection_spec(&request.collection, axon_error::ErrorStage::Retrieving)?;
        if query_vector.len() as u32 != spec.dense.dimensions {
            return Err(ApiError::new(
                "vector.dimension_mismatch",
                axon_error::ErrorStage::Retrieving,
                format!(
                    "query vector dimensions {} do not match collection dimensions {}",
                    query_vector.len(),
                    spec.dense.dimensions
                ),
            ));
        }
        if (query_sparse.is_some() || request.hybrid == Some(true)) && spec.sparse.is_none() {
            return Err(ApiError::new(
                "vector.sparse_not_configured",
                axon_error::ErrorStage::Retrieving,
                format!(
                    "collection {} does not declare a sparse vector namespace",
                    request.collection
                ),
            ));
        }
        validate_search_filters(&request)?;
        let limit = request.limit as usize;
        let mut scored = state
            .points
            .get(&request.collection)
            .into_iter()
            .flat_map(|points| points.values())
            .filter(|point| matches_search_filters(point, &request))
            .map(|point| {
                (
                    point,
                    dot_score(query_vector, &point.vector)
                        + sparse_dot_score(query_sparse, point.sparse_vector.as_ref()),
                )
            })
            .collect::<Vec<_>>();
        scored.sort_by(|(left_point, left_score), (right_point, right_score)| {
            right_score
                .total_cmp(left_score)
                .then(left_point.point_id.0.cmp(&right_point.point_id.0))
        });
        scored.truncate(limit);
        let results = scored
            .into_iter()
            .map(|(point, score)| VectorSearchMatch {
                point_id: point.point_id.clone(),
                score,
                chunk_id: Some(point.chunk_id.clone()),
                document_id: payload_string(&point.payload, "document_id").map(DocumentId::new),
                source_id: payload_string(&point.payload, "source_id").map(SourceId::new),
                source_item_key: None,
                text: payload_string(&point.payload, "chunk_text"),
                payload: point.payload.clone(),
            })
            .collect();
        Ok(VectorSearchResult {
            collection: request.collection,
            results,
            limit: request.limit,
            next_cursor: None,
            warnings: Vec::new(),
            metadata: MetadataMap::new(),
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        let state = self.capability_state();
        let store_state = self.state.lock().await;
        let sparse_configured = store_state
            .collections
            .values()
            .any(|spec| spec.sparse.is_some());
        let payload_indexes = store_state
            .collections
            .values()
            .next()
            .map(|spec| {
                spec.payload_indexes
                    .iter()
                    .map(|index| index.field_name.clone())
                    .collect()
            })
            .unwrap_or_default();
        drop(store_state);
        Ok(ProviderCapability {
            provider_id: self.provider_id.clone(),
            provider_kind: ProviderKind::Vector,
            implementation: "fake".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            health: state.health,
            limits: ProviderLimits {
                max_concurrency: Some(2),
                interactive_reserved_concurrency: Some(1),
                background_max_concurrency: Some(1),
                ..ProviderLimits::default()
            },
            features: vec!["dense".to_string(), "delete_by_chunk".to_string()],
            cooldown_until: state.cooldown_until,
            last_error: state.last_error,
            reservation_policy: ReservationPolicy {
                supports_reservations: true,
                queue_policy: QueuePolicy::Priority,
                interactive_reserve: 1,
                cooldown_after_failures: 1,
                cooldown_secs: 30,
                retry_backoff_ms: Some(100),
            },
            reservation_state: ReservationStateSnapshot {
                queued: 0,
                active: 0,
                available_units: 2,
                oldest_queued_ms: None,
                priority_breakdown: Default::default(),
                states: vec![ReservationState::Granted],
            },
            cost_class: ProviderCostClass::Internal,
            degraded_modes: Vec::new(),
            fake_overrides_supported: true,
            embedding: None,
            llm: None,
            vector_store: Some(VectorStoreCapability {
                dense: true,
                sparse: sparse_configured,
                hybrid: sparse_configured,
                payload_filters: true,
                payload_indexes,
                delete_by_filter: true,
                collection_aliases: false,
                consistency: VectorConsistency::Strong,
            }),
            fetch: None,
            render: None,
            credential: None,
        })
    }
}

impl FakeVectorStore {
    pub async fn reset(&self) -> Result<()> {
        *self.state.lock().await = FakeVectorState::default();
        Ok(())
    }
}

impl FakeVectorState {
    fn collection_spec(
        &self,
        collection: &str,
        stage: axon_error::ErrorStage,
    ) -> Result<&CollectionSpec> {
        self.collections.get(collection).ok_or_else(|| {
            ApiError::new(
                "vector.collection_not_found",
                stage,
                format!("collection {collection} has not been ensured"),
            )
        })
    }
}

fn delete_result(collection: String, points_deleted: u64) -> VectorStoreDeleteResult {
    VectorStoreDeleteResult {
        collection,
        points_matched: points_deleted,
        points_deleted,
        dry_run: false,
        warnings: Vec::new(),
        metadata: MetadataMap::new(),
    }
}

fn dot_score(left: &[f32], right: &[f32]) -> f64 {
    left.iter()
        .zip(right.iter())
        .map(|(left, right)| f64::from(*left) * f64::from(*right))
        .sum()
}

fn sparse_dot_score(query: Option<&SparseVector>, point: Option<&SparseVector>) -> f64 {
    let (Some(query), Some(point)) = (query, point) else {
        return 0.0;
    };
    query
        .indices
        .iter()
        .zip(query.values.iter())
        .map(|(query_index, query_value)| {
            point
                .indices
                .iter()
                .position(|point_index| point_index == query_index)
                .and_then(|position| point.values.get(position))
                .map(|point_value| f64::from(*query_value) * f64::from(*point_value))
                .unwrap_or(0.0)
        })
        .sum()
}

fn payload_string(payload: &MetadataMap, field: &str) -> Option<String> {
    payload.get(field)?.as_str().map(ToString::to_string)
}

fn stage_header(phase: PipelinePhase) -> StageResultHeader {
    let timestamp = Timestamp("2026-07-01T00:00:00Z".to_string());
    StageResultHeader {
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        stage_id: StageId::new(uuid::Uuid::from_u128(0)),
        phase,
        status: LifecycleStatus::Completed,
        started_at: timestamp.clone(),
        completed_at: Some(timestamp),
        counts: StageCounts {
            items_total: None,
            items_done: 0,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    }
}
