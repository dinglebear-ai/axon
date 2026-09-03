//! The single production choke point for provider operations.
//!
//! Provider traits remain transport- and scheduler-agnostic. Production source
//! execution passes a runtime and durable job/attempt/stage identity here; this
//! module selects the provider, waits for scheduler capacity where applicable,
//! and owns every raw provider handle.

use std::future::Future;
use std::sync::Arc;

use axon_api::source::*;
use axon_core::boundary::{ArtifactBytesWriteRequest, ArtifactStore};
use axon_error::ErrorStage;
use axon_graph::sqlite::SqliteGraphStore;
use axon_graph::store::GraphStore;
use axon_jobs::scheduler::{
    ProviderScheduler, ReservationRequest, ReservedCallError, SchedulerError, call_reserved,
};
use axon_ledger::store::LedgerStore;
use sqlx::SqlitePool;

use crate::context::TargetLocalSourceRuntime;

mod support;
mod vector;

use support::{map_reserved, record_provider_heartbeat, scheduler_error};
#[cfg(test)]
pub(crate) use vector::test_bulk_load_cleanup_lifecycle;
pub use vector::{
    begin_bulk_load, delete_vectors, drain_bulk_load_cleanups, finish_bulk_load,
    mark_generation_committed, mark_unchanged_items_committed, retire_generation, vector_operation,
    with_bulk_load,
};

/// Drains cancellation-triggered Qdrant restoration before a process runtime exits.
pub struct BulkLoadCleanupDrain;

impl Drop for BulkLoadCleanupDrain {
    fn drop(&mut self) {
        drain_bulk_load_cleanups();
    }
}

#[derive(Debug, Clone)]
pub struct ProviderCallContext {
    pub job_id: JobId,
    pub attempt: u32,
    pub stage_id: Option<StageId>,
    pub priority: JobPriority,
    pub operation_id: String,
    pub phase: Option<PipelinePhase>,
    pub counts: Option<StageCounts>,
}

impl ProviderCallContext {
    pub fn new(
        job_id: JobId,
        attempt: u32,
        stage_id: Option<StageId>,
        priority: JobPriority,
        operation_id: impl Into<String>,
    ) -> Self {
        Self {
            job_id,
            attempt,
            stage_id,
            priority,
            operation_id: operation_id.into(),
            phase: None,
            counts: None,
        }
    }

    pub fn for_phase(
        job_id: JobId,
        attempt: u32,
        phase: PipelinePhase,
        priority: JobPriority,
        operation_id: impl Into<String>,
    ) -> Self {
        let mut context = Self::new(
            job_id,
            attempt,
            Some(StageId::for_job_stage(job_id, phase.as_str(), 0)),
            priority,
            operation_id,
        );
        context.phase = Some(phase);
        context
    }

    #[must_use]
    pub fn with_counts(mut self, counts: StageCounts) -> Self {
        self.counts = Some(counts);
        self
    }

    fn request(&self, units: u32) -> ReservationRequest {
        ReservationRequest {
            job_id: self.job_id,
            stage_id: self.stage_id,
            attempt: self.attempt,
            fence: format!(
                "{}:{}:{}:{}",
                self.job_id.0,
                self.attempt,
                self.stage_id
                    .map(|stage_id| stage_id.0.to_string())
                    .unwrap_or_else(|| "no-stage".to_string()),
                self.operation_id
            ),
            priority: self.priority,
            units,
        }
    }
}

struct EmbeddingLane;
struct VectorLane;
struct ParseLane;
struct GraphLane;
struct ArtifactLane;

pub struct ArtifactCleanupGuard {
    store: Arc<dyn ArtifactStore>,
    ledger: Arc<dyn LedgerStore>,
    source_id: SourceId,
    generation: SourceGenerationId,
    artifacts: Vec<ArtifactRef>,
    armed: bool,
}

impl ArtifactCleanupGuard {
    pub fn new(
        runtime: &TargetLocalSourceRuntime,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Self {
        Self {
            store: Arc::clone(&runtime.artifact_store),
            ledger: Arc::clone(&runtime.ledger),
            source_id,
            generation,
            artifacts: Vec::new(),
            armed: true,
        }
    }

    #[cfg(test)]
    pub fn new_for_test(
        store: Arc<dyn ArtifactStore>,
        ledger: Arc<dyn LedgerStore>,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Self {
        Self {
            store,
            ledger,
            source_id,
            generation,
            artifacts: Vec::new(),
            armed: true,
        }
    }

    pub fn track(&mut self, artifacts: &[ArtifactRef]) {
        for artifact in artifacts {
            if self
                .artifacts
                .iter()
                .all(|tracked| tracked.artifact_id != artifact.artifact_id)
            {
                self.artifacts.push(artifact.clone());
            }
        }
    }

    pub fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ArtifactCleanupGuard {
    fn drop(&mut self) {
        if !self.armed || self.artifacts.is_empty() {
            return;
        }
        let store = Arc::clone(&self.store);
        let ledger = Arc::clone(&self.ledger);
        let source_id = self.source_id.clone();
        let generation = self.generation.clone();
        let artifacts = std::mem::take(&mut self.artifacts);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                match ledger.committed_generation(source_id).await {
                    Ok(Some(committed)) if committed == generation => return,
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "could not verify publication before artifact cleanup; preserving artifacts"
                        );
                        return;
                    }
                }
                for artifact in artifacts {
                    let handle = ArtifactHandle {
                        artifact_id: artifact.artifact_id.clone(),
                        artifact_kind: artifact.artifact_kind,
                        uri: Some(artifact.uri.clone()),
                    };
                    if let Err(error) = store.delete(handle).await {
                        tracing::warn!(
                            artifact_id = %artifact.artifact_id.0,
                            error = %error,
                            "failed to clean artifact from uncommitted source generation"
                        );
                    }
                }
            });
        }
    }
}

pub async fn ensure_source_providers_ready(
    runtime: &TargetLocalSourceRuntime,
) -> Result<(), ApiError> {
    let embedding = runtime.embedding_provider.capabilities().await?;
    let vector = runtime.vector_store.capabilities().await?;
    for capability in [&embedding, &vector] {
        if !matches!(
            capability.health,
            HealthStatus::Healthy | HealthStatus::Degraded
        ) {
            return Err(capability.last_error.clone().unwrap_or_else(|| {
                ApiError::new(
                    "provider.not_ready",
                    ErrorStage::Planning,
                    format!("provider {} is not ready", capability.provider_id.0),
                )
            }));
        }
    }
    if !vector
        .vector_store
        .as_ref()
        .is_some_and(|capability| capability.generation_publish)
    {
        return Err(ApiError::new(
            "provider.generation_publish_unsupported",
            ErrorStage::Planning,
            "vector provider does not support source generation publication",
        ));
    }
    Ok(())
}

pub async fn embed(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    batch: EmbeddingBatch,
) -> Result<EmbeddingResult, ApiError> {
    let Some(scheduler) = runtime.embedding_scheduler.as_deref() else {
        record_provider_heartbeat(runtime, &context, None).await;
        return runtime.embedding_provider.embed(batch).await;
    };
    let provider = Arc::clone(&runtime.embedding_provider);
    let request = context.request(1);
    map_reserved(
        call_reserved::<EmbeddingLane, _, ApiError, _, _>(
            scheduler,
            request,
            move |lease| async move {
                let snapshot = lease.snapshot(context.priority, 1);
                record_provider_heartbeat(runtime, &context, Some(snapshot)).await;
                provider.embed(batch).await
            },
        )
        .await,
        ErrorStage::Embedding,
        "embedding",
    )
}

pub async fn ensure_collection(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    spec: CollectionSpec,
) -> Result<(), ApiError> {
    let Some(scheduler) = runtime.vector_scheduler.as_deref() else {
        return runtime.vector_store.ensure_collection(spec).await;
    };
    let store = Arc::clone(&runtime.vector_store);
    map_reserved(
        call_reserved::<VectorLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move { store.ensure_collection(spec).await },
        )
        .await,
        ErrorStage::Upserting,
        "vector",
    )
}

pub async fn upsert(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    batch: VectorPointBatch,
) -> Result<VectorStoreWriteResult, ApiError> {
    let Some(scheduler) = runtime.vector_scheduler.as_deref() else {
        record_provider_heartbeat(runtime, &context, None).await;
        return runtime.vector_store.upsert(batch).await;
    };
    let store = Arc::clone(&runtime.vector_store);
    let request = context.request(1);
    map_reserved(
        call_reserved::<VectorLane, _, ApiError, _, _>(
            scheduler,
            request,
            move |lease| async move {
                let snapshot = lease.snapshot(context.priority, 1);
                record_provider_heartbeat(runtime, &context, Some(snapshot)).await;
                store.upsert(batch).await
            },
        )
        .await,
        ErrorStage::Upserting,
        "vector",
    )
}

pub async fn search_vectors(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    request: VectorSearchRequest,
) -> Result<VectorSearchResult, ApiError> {
    let Some(scheduler) = runtime.vector_scheduler.as_deref() else {
        return runtime.vector_store.search(request).await;
    };
    let store = Arc::clone(&runtime.vector_store);
    map_reserved(
        call_reserved::<VectorLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move { store.search(request).await },
        )
        .await,
        ErrorStage::Retrieving,
        "vector",
    )
}

pub async fn parse_operation<T, F, Fut>(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    operation: F,
) -> anyhow::Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let Some(scheduler) = runtime.parse_scheduler.as_deref() else {
        return operation().await;
    };
    match call_reserved::<ParseLane, _, anyhow::Error, _, _>(
        scheduler,
        context.request(1),
        move |_lease| operation(),
    )
    .await
    {
        Ok(value) => Ok(value),
        Err(ReservedCallError::Provider(error)) => Err(error),
        Err(ReservedCallError::Scheduler(error)) => Err(anyhow::Error::new(scheduler_error(
            error,
            ErrorStage::ParsingContent,
            "parser",
        ))),
    }
}

pub async fn graph_operation<T, F, Fut>(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    operation: F,
) -> Result<T, ApiError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    let Some(scheduler) = runtime.graph_scheduler.as_deref() else {
        return Ok(operation().await);
    };
    map_reserved(
        call_reserved::<GraphLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move { Ok(operation().await) },
        )
        .await,
        ErrorStage::Graphing,
        "graph",
    )
}

pub async fn upsert_graph_candidates(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    pool: SqlitePool,
    candidates: Vec<GraphCandidate>,
) -> Result<GraphWriteResult, ApiError> {
    graph_operation(runtime, context, move || async move {
        let store = SqliteGraphStore::from_pool(pool);
        store.upsert_candidate_iter(candidates).await
    })
    .await?
}

#[cfg(test)]
pub async fn upsert_graph_candidates_for_test(
    pool: SqlitePool,
    candidates: Vec<GraphCandidate>,
) -> Result<GraphWriteResult, ApiError> {
    let store = SqliteGraphStore::from_pool(pool);
    store.upsert_candidate_iter(candidates).await
}

pub async fn put_artifact(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    request: ArtifactWriteRequest,
) -> Result<ArtifactHandle, ApiError> {
    let Some(scheduler) = runtime.artifact_scheduler.as_deref() else {
        return runtime.artifact_store.put(request).await;
    };
    let store = Arc::clone(&runtime.artifact_store);
    map_reserved(
        call_reserved::<ArtifactLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move { store.put(request).await },
        )
        .await,
        ErrorStage::Publishing,
        "artifact",
    )
}

pub async fn put_artifact_bytes(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    request: ArtifactBytesWriteRequest,
) -> Result<ArtifactHandle, ApiError> {
    let Some(scheduler) = runtime.artifact_scheduler.as_deref() else {
        return runtime.artifact_store.put_bytes(request).await;
    };
    let store = Arc::clone(&runtime.artifact_store);
    map_reserved(
        call_reserved::<ArtifactLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move { store.put_bytes(request).await },
        )
        .await,
        ErrorStage::Publishing,
        "artifact",
    )
}
