//! ArtifactCandidate staging and post-commit delivery orchestration.

use axon_api::source::*;

use super::super::{SourcePipelineInput, artifact_candidates};
use super::{ArtifactCleanupGuard, IndexCounts, ProgressCoordinator, stage_counts};
use crate::context::TargetLocalSourceRuntime;

pub(super) async fn stage_candidate_delivery(
    runtime: &TargetLocalSourceRuntime,
    job_id: JobId,
    source_id: SourceId,
    generation: SourceGenerationId,
    candidates: &mut Vec<ArtifactCandidate>,
) -> anyhow::Result<Option<String>> {
    let Some(outbox) = &runtime.artifact_candidate_outbox else {
        return Ok(None);
    };
    let staged = outbox
        .stage(job_id, source_id, generation, std::mem::take(candidates))
        .await?;
    Ok(staged.map(|pending| pending.delivery_key))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn finish_candidate_delivery(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    coordinator: &ProgressCoordinator,
    artifact_cleanup: &mut ArtifactCleanupGuard,
    generation: &SourceGenerationId,
    candidates: Vec<ArtifactCandidate>,
    staged_delivery: Option<String>,
    result: &mut anyhow::Result<IndexCounts>,
) -> anyhow::Result<()> {
    if result.is_err() {
        if let (Some(outbox), Some(delivery_key)) =
            (&runtime.artifact_candidate_outbox, staged_delivery)
        {
            outbox.complete(&delivery_key).await?;
        }
        return Ok(());
    }

    coordinator
        .checkpoint(
            PipelinePhase::Publishing,
            stage_counts(Some(1), 1, None, 0, None, 0),
            "published source generation",
        )
        .await;
    artifact_cleanup.disarm();
    let Ok(output) = result else {
        return Ok(());
    };
    if staged_delivery.is_some() {
        artifact_candidates::spawn_outbox_drain(runtime);
        return Ok(());
    }
    let warnings = artifact_candidates::submit_committed_candidates(
        runtime.artifact_candidate_sink.as_ref(),
        input.plan.job_id,
        input.plan.route.source.source_id.clone(),
        generation,
        candidates,
    )
    .await;
    if !warnings.is_empty() {
        output.warnings.extend(warnings);
        super::super::persist_degraded_summary(runtime, output).await;
    }
    Ok(())
}
