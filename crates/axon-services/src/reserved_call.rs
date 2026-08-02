//! The single production choke point for provider operations.
//!
//! Provider traits remain transport- and scheduler-agnostic. Production source
//! execution passes a runtime and durable job/attempt/stage identity here; this
//! module selects the provider, waits for scheduler capacity where applicable,
//! and owns every raw provider handle.

use std::sync::Arc;

use axon_api::source::*;
use axon_core::boundary::{ArtifactBytesWriteRequest, ArtifactStore};
use axon_error::ErrorStage;
use axon_jobs::scheduler::{
    ProviderScheduler, ReservationRequest, ReservedCallError, SchedulerError, call_reserved,
};
use axon_ledger::store::LedgerStore;

use crate::context::TargetLocalSourceRuntime;

#[derive(Debug, Clone)]
pub struct ProviderCallContext {
    pub job_id: JobId,
    pub attempt: u32,
    pub stage_id: Option<StageId>,
    pub priority: JobPriority,
    pub operation_id: String,
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
        }
    }

    pub fn for_phase(
        job_id: JobId,
        attempt: u32,
        phase: PipelinePhase,
        priority: JobPriority,
        operation_id: impl Into<String>,
    ) -> Self {
        Self::new(
            job_id,
            attempt,
            Some(StageId::for_job_stage(job_id, phase.as_str(), 0)),
            priority,
            operation_id,
        )
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
        return runtime.embedding_provider.embed(batch).await;
    };
    let provider = Arc::clone(&runtime.embedding_provider);
    map_reserved(
        call_reserved::<EmbeddingLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move { provider.embed(batch).await },
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
        return runtime.vector_store.upsert(batch).await;
    };
    let store = Arc::clone(&runtime.vector_store);
    map_reserved(
        call_reserved::<VectorLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move { store.upsert(batch).await },
        )
        .await,
        ErrorStage::Upserting,
        "vector",
    )
}

pub async fn mark_generation_committed(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    collection: String,
    source_id: SourceId,
    generation: SourceGenerationId,
) -> Result<VectorStoreWriteResult, ApiError> {
    let Some(scheduler) = runtime.vector_scheduler.as_deref() else {
        return runtime
            .vector_store
            .mark_generation_committed(collection, source_id, generation)
            .await;
    };
    let store = Arc::clone(&runtime.vector_store);
    map_reserved(
        call_reserved::<VectorLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move {
                store
                    .mark_generation_committed(collection, source_id, generation)
                    .await
            },
        )
        .await,
        ErrorStage::Publishing,
        "vector",
    )
}

pub async fn mark_unchanged_items_committed(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    collection: String,
    source_id: SourceId,
    previous_generation: SourceGenerationId,
    committed_generation: SourceGenerationId,
    source_item_keys: Vec<SourceItemKey>,
) -> Result<VectorStoreWriteResult, ApiError> {
    let Some(scheduler) = runtime.vector_scheduler.as_deref() else {
        return runtime
            .vector_store
            .mark_unchanged_items_committed(
                collection,
                source_id,
                previous_generation,
                committed_generation,
                source_item_keys,
            )
            .await;
    };
    let store = Arc::clone(&runtime.vector_store);
    map_reserved(
        call_reserved::<VectorLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move {
                store
                    .mark_unchanged_items_committed(
                        collection,
                        source_id,
                        previous_generation,
                        committed_generation,
                        source_item_keys,
                    )
                    .await
            },
        )
        .await,
        ErrorStage::Publishing,
        "vector",
    )
}

pub async fn delete_vectors(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    selector: VectorDeleteSelector,
) -> Result<VectorStoreDeleteResult, ApiError> {
    let Some(scheduler) = runtime.vector_scheduler.as_deref() else {
        return runtime.vector_store.delete(selector).await;
    };
    let store = Arc::clone(&runtime.vector_store);
    map_reserved(
        call_reserved::<VectorLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move { store.delete(selector).await },
        )
        .await,
        ErrorStage::Cleaning,
        "vector",
    )
}

pub async fn retire_generation(
    runtime: &TargetLocalSourceRuntime,
    context: ProviderCallContext,
    collection: String,
    source_id: SourceId,
    generation: SourceGenerationId,
    retired_epoch: SourceGenerationId,
) -> Result<VectorStoreWriteResult, ApiError> {
    let Some(scheduler) = runtime.vector_scheduler.as_deref() else {
        return runtime
            .vector_store
            .retire_generation(collection, source_id, generation, retired_epoch)
            .await;
    };
    let store = Arc::clone(&runtime.vector_store);
    map_reserved(
        call_reserved::<VectorLane, _, ApiError, _, _>(
            scheduler,
            context.request(1),
            move |_lease| async move {
                store
                    .retire_generation(collection, source_id, generation, retired_epoch)
                    .await
            },
        )
        .await,
        ErrorStage::Publishing,
        "vector",
    )
}

pub async fn put_artifact(
    runtime: &TargetLocalSourceRuntime,
    request: ArtifactWriteRequest,
) -> Result<ArtifactHandle, ApiError> {
    runtime.artifact_store.put(request).await
}

pub async fn put_artifact_bytes(
    runtime: &TargetLocalSourceRuntime,
    request: ArtifactBytesWriteRequest,
) -> Result<ArtifactHandle, ApiError> {
    runtime.artifact_store.put_bytes(request).await
}

fn map_reserved<T>(
    result: Result<T, ReservedCallError<ApiError>>,
    stage: ErrorStage,
    provider_id: &str,
) -> Result<T, ApiError> {
    match result {
        Ok(value) => Ok(value),
        Err(ReservedCallError::Provider(error)) => Err(error),
        Err(ReservedCallError::Scheduler(error)) => Err(scheduler_error(error, stage, provider_id)),
    }
}

fn scheduler_error(error: SchedulerError, stage: ErrorStage, provider_id: &str) -> ApiError {
    let code = match error {
        SchedulerError::RequestTooLarge => "provider.scheduler.request_too_large",
        SchedulerError::QueueFull => "provider.scheduler.queue_full",
        SchedulerError::WaitTimeout => "provider.scheduler.wait_timeout",
        SchedulerError::StaleFence => "provider.scheduler.stale_fence",
        SchedulerError::Queued => "provider.scheduler.queued",
        SchedulerError::Database(_) => "provider.scheduler.database",
    };
    ApiError::new(code, stage, error.to_string()).with_provider_id(provider_id)
}
