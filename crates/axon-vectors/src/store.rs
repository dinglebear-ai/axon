//! Vector store boundary and deterministic fake.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use tokio::sync::Mutex;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn ensure_collection(&self, spec: CollectionSpec) -> Result<()>;
    async fn upsert(&self, batch: VectorPointBatch) -> Result<VectorStoreWriteResult>;
    async fn delete(&self, selector: VectorDeleteSelector) -> Result<VectorStoreDeleteResult>;
    async fn search(&self, request: VectorSearchRequest) -> Result<VectorSearchResult>;
    async fn reset(&self) -> Result<()>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}

#[derive(Debug, Clone)]
pub struct FakeVectorStore {
    provider_id: ProviderId,
    state: Arc<Mutex<FakeVectorState>>,
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
            state: Arc::new(Mutex::new(FakeVectorState::default())),
        }
    }

    pub async fn calls(&self) -> Vec<&'static str> {
        self.state.lock().await.calls.clone()
    }
}

#[async_trait]
impl VectorStore for FakeVectorStore {
    async fn ensure_collection(&self, spec: CollectionSpec) -> Result<()> {
        let mut state = self.state.lock().await;
        state.calls.push("ensure_collection");
        state.collections.insert(spec.collection.clone(), spec);
        Ok(())
    }

    async fn upsert(&self, batch: VectorPointBatch) -> Result<VectorStoreWriteResult> {
        let mut state = self.state.lock().await;
        state.calls.push("upsert");
        let collection = state.points.entry(batch.collection.clone()).or_default();
        let points_attempted = batch.points.len() as u64;
        for point in batch.points {
            collection.insert(point.point_id.clone(), point);
        }
        Ok(VectorStoreWriteResult {
            header: stage_header(PipelinePhase::Upserting),
            collection: batch.collection,
            points_attempted,
            points_written: points_attempted,
            payload_indexes_created: Vec::new(),
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
        let (collection_name, deleted) = match selector {
            VectorDeleteSelector::Chunks {
                collection,
                chunk_ids,
            } => {
                let Some(points) = state.points.get_mut(&collection) else {
                    return Ok(delete_result(collection, 0));
                };
                let before = points.len();
                points.retain(|_, point| !chunk_ids.contains(&point.chunk_id));
                (collection, before.saturating_sub(points.len()) as u64)
            }
            VectorDeleteSelector::Points {
                collection,
                point_ids,
            } => {
                let Some(points) = state.points.get_mut(&collection) else {
                    return Ok(delete_result(collection, 0));
                };
                let mut deleted = 0;
                for point_id in point_ids {
                    if points.remove(&point_id).is_some() {
                        deleted += 1;
                    }
                }
                (collection, deleted)
            }
            other => {
                return Err(ApiError::new(
                    "vector.selector_unsupported",
                    axon_error::ErrorStage::Upserting,
                    format!("fake vector store does not support selector {other:?}"),
                ));
            }
        };
        Ok(delete_result(collection_name, deleted))
    }

    async fn search(&self, request: VectorSearchRequest) -> Result<VectorSearchResult> {
        if !request.filters.is_empty()
            || request.generation.is_some()
            || !request.graph_refs.is_empty()
        {
            return Err(ApiError::new(
                "vector.filter_unsupported",
                axon_error::ErrorStage::Retrieving,
                "fake vector store does not implement filtered search",
            ));
        }
        let mut state = self.state.lock().await;
        state.calls.push("search");
        let query_vector = request.dense_vector.clone().unwrap_or_default();
        let mut results = state
            .points
            .get(&request.collection)
            .into_iter()
            .flat_map(|points| points.values())
            .map(|point| VectorSearchMatch {
                point_id: point.point_id.clone(),
                score: dot_score(&query_vector, &point.vector),
                chunk_id: Some(point.chunk_id.clone()),
                document_id: None,
                source_id: None,
                source_item_key: None,
                text: None,
                payload: point.payload.clone(),
            })
            .collect::<Vec<_>>();
        results.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then(a.point_id.0.cmp(&b.point_id.0))
        });
        results.truncate(request.limit as usize);
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
        Ok(ProviderCapability {
            provider_id: self.provider_id.clone(),
            provider_kind: ProviderKind::Vector,
            implementation: "fake".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            health: HealthStatus::Healthy,
            limits: ProviderLimits {
                max_concurrency: Some(2),
                interactive_reserved_concurrency: Some(1),
                background_max_concurrency: Some(1),
                ..ProviderLimits::default()
            },
            features: vec!["dense".to_string(), "delete_by_chunk".to_string()],
            cooldown_until: None,
            last_error: None,
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
                sparse: false,
                hybrid: false,
                payload_filters: false,
                payload_indexes: Vec::new(),
                delete_by_filter: false,
                collection_aliases: false,
                consistency: VectorConsistency::Strong,
            }),
            fetch: None,
            render: None,
            credential: None,
        })
    }

    async fn reset(&self) -> Result<()> {
        *self.state.lock().await = FakeVectorState::default();
        Ok(())
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

fn stage_header(phase: PipelinePhase) -> StageResultHeader {
    StageResultHeader {
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        stage_id: StageId::new(uuid::Uuid::from_u128(0)),
        phase,
        status: LifecycleStatus::Completed,
        started_at: Timestamp("2026-07-01T00:00:00Z".to_string()),
        completed_at: Some(Timestamp("2026-07-01T00:00:00Z".to_string())),
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
