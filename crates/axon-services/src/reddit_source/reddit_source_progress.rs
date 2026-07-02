use async_trait::async_trait;
use axon_api::source::*;
use axon_embedding::provider::EmbeddingProvider;
use axon_vectors::store::VectorStore;

#[async_trait]
pub(super) trait RedditSourceProgress: Send + Sync {
    async fn record_phase(
        &self,
        phase: PipelinePhase,
        status: LifecycleStatus,
        counts: Option<StageCounts>,
        error: Option<SourceError>,
        provider_reservations: Vec<ProviderReservationSnapshot>,
    ) -> anyhow::Result<()>;
}

pub(super) async fn ensure_providers_ready(
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
) -> Result<(), ApiError> {
    ensure_provider_capability_ready(embedding_provider.capabilities().await?)?;
    let vector_capability = vector_store.capabilities().await?;
    ensure_provider_capability_ready(vector_capability.clone())?;
    ensure_vector_generation_publish_supported(vector_capability)?;
    Ok(())
}

pub(super) fn phase_for_api_error(error: &ApiError) -> PipelinePhase {
    match error.stage {
        ErrorStage::Embedding => PipelinePhase::Embedding,
        ErrorStage::Upserting => PipelinePhase::Vectorizing,
        ErrorStage::Publishing => PipelinePhase::Publishing,
        ErrorStage::Cleaning => PipelinePhase::Cleaning,
        ErrorStage::Discovering => PipelinePhase::Discovering,
        ErrorStage::Diffing => PipelinePhase::Diffing,
        ErrorStage::Preparing => PipelinePhase::Preparing,
        _ => PipelinePhase::Planning,
    }
}

pub(super) async fn record_progress(
    progress: Option<&dyn RedditSourceProgress>,
    phase: PipelinePhase,
    counts: Option<StageCounts>,
) -> anyhow::Result<()> {
    if let Some(progress) = progress {
        progress
            .record_phase(phase, LifecycleStatus::Running, counts, None, Vec::new())
            .await?;
    }
    Ok(())
}

pub(super) async fn record_progress_with_reservations(
    progress: Option<&dyn RedditSourceProgress>,
    phase: PipelinePhase,
    counts: Option<StageCounts>,
    provider_reservations: Vec<ProviderReservationSnapshot>,
) -> anyhow::Result<()> {
    if let Some(progress) = progress {
        progress
            .record_phase(
                phase,
                LifecycleStatus::Running,
                counts,
                None,
                provider_reservations,
            )
            .await?;
    }
    Ok(())
}

pub(super) async fn record_progress_error(
    progress: Option<&dyn RedditSourceProgress>,
    phase: PipelinePhase,
    error: &ApiError,
) -> anyhow::Result<()> {
    if let Some(progress) = progress {
        progress
            .record_phase(
                phase,
                LifecycleStatus::Failed,
                None,
                Some(source_error_from_api_error(error)),
                Vec::new(),
            )
            .await?;
    }
    Ok(())
}

pub(super) async fn progress_error_context(
    progress: Option<&dyn RedditSourceProgress>,
    phase: PipelinePhase,
    error: &ApiError,
) -> Option<String> {
    record_progress_error(progress, phase, error)
        .await
        .err()
        .map(|progress_err| format!("also failed to record reddit source progress: {progress_err}"))
}

fn ensure_provider_capability_ready(capability: ProviderCapability) -> Result<(), ApiError> {
    if matches!(
        capability.health,
        HealthStatus::Healthy | HealthStatus::Degraded
    ) {
        return Ok(());
    }
    if let Some(error) = capability.last_error {
        return Err(error);
    }
    let stage = match capability.provider_kind {
        ProviderKind::Embedding => ErrorStage::Embedding,
        ProviderKind::Vector => ErrorStage::Upserting,
        _ => ErrorStage::Planning,
    };
    let mut error = ApiError::new(
        "provider.not_ready",
        stage,
        format!(
            "provider {} is not ready: {:?}",
            capability.provider_id.0, capability.health
        ),
    )
    .with_provider_id(capability.provider_id.0);
    error.retryable = true;
    Err(error)
}

fn ensure_vector_generation_publish_supported(
    capability: ProviderCapability,
) -> Result<(), ApiError> {
    let provider_id = capability.provider_id.0.clone();
    let Some(vector_store) = capability.vector_store.as_ref() else {
        return Err(ApiError::new(
            "provider.vector.capability_missing",
            ErrorStage::Planning,
            "vector provider did not report vector store capabilities",
        )
        .with_provider_id(provider_id));
    };
    if vector_store.generation_publish {
        return Ok(());
    }
    Err(ApiError::new(
        "provider.vector.generation_publish_unsupported",
        ErrorStage::Publishing,
        "vector provider does not support source generation publish markers",
    )
    .with_provider_id(provider_id))
}

pub(super) fn source_error_from_api_error(error: &ApiError) -> SourceError {
    SourceError {
        code: error.code.0.clone(),
        severity: Severity::Failed,
        message: error.message.clone(),
        source_item_key: error
            .details
            .get("path_hint")
            .map(|hint| SourceItemKey::new(hint.clone())),
        retryable: error.retryable,
        provider_id: error
            .provider_id
            .as_ref()
            .map(|id| ProviderId::new(id.clone())),
        cause: Some(error.to_string()),
    }
}
