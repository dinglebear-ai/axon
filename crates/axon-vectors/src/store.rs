//! Vector store boundary and deterministic fake.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axon_api::source::*;
use serde_json::json;
use tokio::sync::Mutex;

use crate::collection::{
    check_collection_drift, normalize_collection_spec, validate_collection_spec,
};
use crate::filter::{matches_delete_selector, selector_collection, validate_delete_selector};
use crate::payload::generation_payload_i64;
use crate::store_helpers::{delete_result, payload_string, stage_header};
use crate::validation::validate_upsert_batch;

#[path = "store/arc_forward.rs"]
mod arc_forward;
#[path = "store/fake_capability.rs"]
mod fake_capability;
#[path = "store/fake_commit.rs"]
mod fake_commit;
#[path = "store/fake_search.rs"]
mod fake_search;
#[path = "store/fake_state.rs"]
mod fake_state;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn ensure_collection(&self, spec: CollectionSpec) -> Result<()>;
    async fn begin_bulk_load(&self, _collection: &str) -> Result<()> {
        Ok(())
    }
    async fn finish_bulk_load(&self, _collection: &str) -> Result<()> {
        Ok(())
    }
    async fn upsert(&self, batch: VectorPointBatch) -> Result<VectorStoreWriteResult>;
    /// Count points already using a generation before staging any new points.
    /// Stores used for source indexing must override this to protect imports
    /// whose vectors predate the source ledger.
    async fn count_generation_points(
        &self,
        _collection: String,
        _source_id: SourceId,
        _generation: SourceGenerationId,
    ) -> Result<u64> {
        Err(ApiError::new(
            "vector.generation_count_unsupported",
            ErrorStage::Retrieving,
            "vector store cannot check whether a source generation is occupied",
        ))
    }
    async fn mark_generation_committed(
        &self,
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<VectorStoreWriteResult>;
    /// Carry unchanged items into a new committed generation without mutating
    /// the previous committed generation's points. Implementations should copy
    /// or otherwise stage new-generation visibility so old committed searches
    /// remain valid until ledger publish is durable.
    async fn mark_unchanged_items_committed(
        &self,
        collection: String,
        source_id: SourceId,
        previous_generation: SourceGenerationId,
        committed_generation: SourceGenerationId,
        source_item_keys: Vec<SourceItemKey>,
    ) -> Result<VectorStoreWriteResult>;
    async fn retire_generation(
        &self,
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
        retired_epoch: SourceGenerationId,
    ) -> Result<VectorStoreWriteResult> {
        let _ = (collection, source_id, generation, retired_epoch);
        Err(ApiError::new(
            "vector.retirement_unsupported",
            ErrorStage::Publishing,
            "vector store does not support epoch retirement",
        ))
    }
    async fn delete(&self, selector: VectorDeleteSelector) -> Result<VectorStoreDeleteResult>;
    async fn search(&self, request: VectorSearchRequest) -> Result<VectorSearchResult>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}

#[derive(Debug, Clone)]
pub struct FakeVectorStore {
    provider_id: ProviderId,
    health: HealthStatus,
    mode: FakeVectorMode,
    cooldown_until_override: Option<Timestamp>,
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
    CommitFailure,
    PartialCommitFailure,
    DeleteFailure,
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
            cooldown_until_override: None,
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

    /// Override `capabilities().cooldown_until`, taking precedence over the
    /// fixed timestamp [`FakeVectorMode::RateLimited`] otherwise reports.
    /// Lets tests simulate a live, "now"-relative cooldown window instead of
    /// a mode-derived fixed instant.
    pub fn with_cooldown_until(mut self, cooldown_until: Timestamp) -> Self {
        self.cooldown_until_override = Some(cooldown_until);
        self
    }

    pub async fn calls(&self) -> Vec<&'static str> {
        self.state.lock().await.calls.clone()
    }

    pub async fn collection_spec(&self, collection: &str) -> Option<CollectionSpec> {
        self.state.lock().await.collections.get(collection).cloned()
    }

    pub async fn points(&self, collection: &str) -> Vec<VectorPoint> {
        self.state
            .lock()
            .await
            .points
            .get(collection)
            .map(|points| points.values().cloned().collect())
            .unwrap_or_default()
    }

    fn mode_error(&self) -> Option<ApiError> {
        self.mode_error_for(ErrorStage::Upserting)
    }

    fn mode_error_for(&self, stage: ErrorStage) -> Option<ApiError> {
        match self.mode {
            FakeVectorMode::Success
            | FakeVectorMode::PartialFailure
            | FakeVectorMode::SlowWrite
            | FakeVectorMode::CommitFailure
            | FakeVectorMode::PartialCommitFailure
            | FakeVectorMode::DeleteFailure => None,
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
            | FakeVectorMode::SlowWrite
            | FakeVectorMode::CommitFailure
            | FakeVectorMode::PartialCommitFailure
            | FakeVectorMode::DeleteFailure => FakeProviderModeState::Success,
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
            ErrorStage::Upserting,
            "vector store",
        );
        if self.mode == FakeVectorMode::Unavailable {
            state.health = HealthStatus::Unavailable;
            state.last_error = self.mode_error();
        }
        if self.health != HealthStatus::Healthy {
            state.health = self.health;
        }
        if let Some(cooldown_until) = self.cooldown_until_override.clone() {
            state.cooldown_until = Some(cooldown_until);
        }
        state
    }
}

#[async_trait]
impl VectorStore for FakeVectorStore {
    async fn count_generation_points(
        &self,
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<u64> {
        let mut state = self.state.lock().await;
        state.calls.push("count_generation_points");
        state.collection_spec(&collection, ErrorStage::Retrieving)?;
        Ok(state.points.get(&collection).map_or(0, |points| {
            points
                .values()
                .filter(|point| {
                    payload_string(&point.payload, "source_id").as_deref() == Some(&source_id.0)
                        && payload_generation_matches(
                            &point.payload,
                            "source_generation",
                            &generation,
                        )
                })
                .count() as u64
        }))
    }
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

    async fn begin_bulk_load(&self, _collection: &str) -> Result<()> {
        self.state.lock().await.calls.push("begin_bulk_load");
        Ok(())
    }

    async fn finish_bulk_load(&self, _collection: &str) -> Result<()> {
        self.state.lock().await.calls.push("finish_bulk_load");
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
                ErrorStage::Upserting,
                format!("collection {} has not been ensured", batch.collection),
            )
        })?;
        let batch_sparse = validate_upsert_batch(spec, &batch, ErrorStage::Upserting)?;
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
                ErrorStage::Upserting,
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

    async fn mark_generation_committed(
        &self,
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<VectorStoreWriteResult> {
        let mut state = self.state.lock().await;
        state.calls.push("mark_generation_committed");
        if self.mode == FakeVectorMode::CommitFailure {
            return Err(ApiError::new(
                "provider.commit_failed",
                ErrorStage::Publishing,
                "vector store failed to mark generation committed",
            )
            .with_provider_id(&self.provider_id.0));
        }
        if let Some(err) = self.mode_error_for(ErrorStage::Publishing) {
            return Err(err);
        }
        state.collection_spec(&collection, ErrorStage::Publishing)?;
        let points = state.points.entry(collection.clone()).or_default();
        let mut points_written = 0;
        for point in points.values_mut() {
            let point_source = payload_string(&point.payload, "source_id");
            if point_source.as_deref() == Some(source_id.0.as_str())
                && payload_generation_matches(&point.payload, "source_generation", &generation)
            {
                point.payload.insert(
                    "committed_generation".to_string(),
                    json!(generation_payload_i64(&generation, "committed_generation")?),
                );
                point
                    .payload
                    .insert("document_status".to_string(), json!("published"));
                points_written += 1;
            }
        }
        Ok(VectorStoreWriteResult {
            header: stage_header(PipelinePhase::Publishing),
            collection,
            points_attempted: points_written,
            points_written,
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
        if self.mode == FakeVectorMode::DeleteFailure {
            return Err(ApiError::new(
                "provider.delete_failed",
                ErrorStage::Cleaning,
                "fake vector store failed to delete points",
            )
            .with_provider_id(&self.provider_id.0));
        }
        if let Some(err) = self.mode_error_for(ErrorStage::Cleaning) {
            return Err(err);
        }
        validate_delete_selector(&selector)?;
        let collection = selector_collection(&selector).to_string();
        state.collection_spec(&collection, ErrorStage::Cleaning)?;
        let points = state.points.entry(collection.clone()).or_default();
        let before = points.len();
        points.retain(|_, point| !matches_delete_selector(point, &selector));
        Ok(delete_result(
            collection,
            before.saturating_sub(points.len()) as u64,
        ))
    }

    async fn mark_unchanged_items_committed(
        &self,
        collection: String,
        source_id: SourceId,
        previous_generation: SourceGenerationId,
        committed_generation: SourceGenerationId,
        source_item_keys: Vec<SourceItemKey>,
    ) -> Result<VectorStoreWriteResult> {
        self.mark_unchanged_items_committed_inner(
            collection,
            source_id,
            previous_generation,
            committed_generation,
            source_item_keys,
        )
        .await
    }

    async fn retire_generation(
        &self,
        collection: String,
        source_id: SourceId,
        generation: SourceGenerationId,
        retired_epoch: SourceGenerationId,
    ) -> Result<VectorStoreWriteResult> {
        self.retire_generation_inner(collection, source_id, generation, retired_epoch)
            .await
    }

    async fn search(&self, request: VectorSearchRequest) -> Result<VectorSearchResult> {
        self.search_inner(request).await
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        self.capabilities_inner().await
    }
}

fn payload_generation_matches(
    payload: &MetadataMap,
    field: &str,
    generation: &SourceGenerationId,
) -> bool {
    let Ok(expected) = generation_payload_i64(generation, field) else {
        return false;
    };
    payload.get(field).and_then(serde_json::Value::as_i64) == Some(expected)
}

impl FakeVectorStore {
    pub async fn reset(&self) -> Result<()> {
        *self.state.lock().await = FakeVectorState::default();
        Ok(())
    }
}
