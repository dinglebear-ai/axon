//! Provider reservation and liveness heartbeat helpers for source vectorization.
//!
//! Reservation acquisition remains fatal because provider capacity is required
//! for the stage. Heartbeat persistence is observational and degrades to a
//! structured warning so it cannot fail otherwise-successful provider work.

use axon_api::source::{JobHeartbeat, LifecycleStatus, PipelinePhase, StageCounts};
use axon_embedding::reservation::{ProviderReservation, ProviderReservationContext};

use super::{NonWebPipelineInput, TargetLocalSourceRuntime, timestamp};

pub(super) async fn embedding(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
) -> anyhow::Result<ProviderReservation> {
    Ok(runtime
        .embedding_reservations
        .reserve_with_context(ProviderReservationContext {
            job_id: input.plan.job_id,
            stage_id: None,
            provider_id: Some(runtime.embedding_provider_id.clone()),
            priority: input.execution.priority,
            units: 1,
            ttl_seconds: Some(300),
        })
        .await?)
}

pub(super) async fn vector(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
) -> anyhow::Result<ProviderReservation> {
    Ok(runtime
        .vector_reservations
        .reserve_with_context(ProviderReservationContext {
            job_id: input.plan.job_id,
            stage_id: None,
            provider_id: Some(runtime.vector_provider_id.clone()),
            priority: input.execution.priority,
            units: 1,
            ttl_seconds: Some(300),
        })
        .await?)
}

pub(super) async fn heartbeat(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
    phase: PipelinePhase,
    counts: StageCounts,
    reservation: &ProviderReservation,
) {
    let heartbeat = JobHeartbeat {
        job_id: input.plan.job_id,
        attempt: input.execution.attempt,
        worker_id: Some("source-pipeline".to_string()),
        phase,
        status: LifecycleStatus::Running,
        stage_id: None,
        heartbeat_at: timestamp(),
        sequence: 0,
        last_progress_at: Some(timestamp()),
        last_event_sequence: None,
        counts: Some(counts),
        provider_reservations: vec![reservation.snapshot()],
    };
    if let Err(error) = runtime.jobs.heartbeat(heartbeat).await {
        tracing::warn!(
            job_id = %input.plan.job_id.0,
            phase = ?phase,
            error = %error,
            "failed to persist provider reservation heartbeat"
        );
    }
}
