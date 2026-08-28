use super::*;
use tokio_util::sync::CancellationToken;

use crate::source::executor::generation_work::{
    PreparedBatchSender, PreparedBatchSideEffects, prepared_work_channel,
};
use crate::source::executor::progress::PipelineProgress;

pub(super) fn enabled() -> bool {
    std::env::var("AXON_EMBED_SCHEDULER_ENABLED")
        .ok()
        .and_then(|value| value.parse::<bool>().ok())
        .unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn process(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    generation: &SourceGenerationId,
    collection: &CollectionSpec,
    diff: &SourceManifestDiff,
    archive_requested: bool,
    changed_total: u64,
    coordinator: &ProgressCoordinator,
    stage: &mut GenerationStageProgress,
    accumulated: &mut GenerationAccumulator,
    artifact_cleanup: &mut ArtifactCleanupGuard,
) -> anyhow::Result<()> {
    let (sender, receiver) = prepared_work_channel(runtime.embed_pool_max_inputs)?;
    tracing::info!(
        chunk_capacity = runtime.embed_pool_max_inputs.saturating_mul(3),
        queue_capacity = 2,
        byte_capacity_kib = 1_048_576_u64,
        "enabled bounded generation embedding scheduler"
    );
    let cancel = CancellationToken::new();
    let producer = produce(
        runtime,
        input,
        emitter,
        generation,
        diff,
        archive_requested,
        changed_total,
        coordinator,
        stage,
        artifact_cleanup,
        sender,
        &cancel,
    );
    let mut scheduler_progress = PipelineProgress::default();
    let consumer = super::super::scheduler::run_generation_scheduler(
        runtime,
        input,
        emitter,
        coordinator,
        generation,
        collection.clone(),
        receiver,
        accumulated,
        &mut scheduler_progress,
        &cancel,
    );
    let (produced, consumed) = tokio::join!(producer, consumer);
    match (produced, consumed) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) | (Ok(()), Err(primary)) => Err(primary),
        (Err(primary), Err(secondary)) => Err(primary.context(format!(
            "generation scheduler counterpart also failed: {secondary:#}"
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
async fn produce(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    generation: &SourceGenerationId,
    diff: &SourceManifestDiff,
    archive_requested: bool,
    changed_total: u64,
    coordinator: &ProgressCoordinator,
    stage: &mut GenerationStageProgress,
    artifact_cleanup: &mut ArtifactCleanupGuard,
    sender: PreparedBatchSender,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    let acquire_batch_size = acquire_batch_size();
    let first_batch_size = first_acquire_batch_size(acquire_batch_size);
    let changed = usize::try_from(changed_total).unwrap_or(usize::MAX);
    let batch_count = if changed <= first_batch_size {
        usize::from(changed > 0)
    } else {
        1 + (changed - first_batch_size).div_ceil(acquire_batch_size)
    };
    let mut batches = batch_changed_diff_ramped(diff, first_batch_size, acquire_batch_size)
        .enumerate()
        .map(|(index, diff)| ChangedBatch {
            diff,
            is_final: index + 1 == batch_count,
        });
    let Some(first) = batches.next() else {
        return Ok(());
    };
    let mut acquired = acquire_changed_batch(
        input,
        first,
        changed_total,
        stage.acquired_items,
        stage.acquired_documents,
        coordinator,
        true,
    )
    .await?;
    loop {
        stage.acquired_items = stage.acquired_items.saturating_add(acquired.items);
        stage.acquired_documents = stage.acquired_documents.saturating_add(acquired.documents);
        let Some(next_batch) = batches.next() else {
            let prepared = prepare(
                runtime,
                input,
                emitter,
                generation,
                acquired,
                archive_requested,
                coordinator,
                stage,
                artifact_cleanup,
            )
            .await?;
            send_prepared(&sender, prepared, cancel).await?;
            break;
        };
        let next_acquisition = acquire_changed_batch(
            input,
            next_batch,
            changed_total,
            stage.acquired_items,
            stage.acquired_documents,
            coordinator,
            !input.adapter.supports_acquisition_prefetch(),
        );
        let (prepared, prefetched) = process_and_acquire_next(
            input.adapter,
            prepare(
                runtime,
                input,
                emitter,
                generation,
                acquired,
                archive_requested,
                coordinator,
                stage,
                artifact_cleanup,
            ),
            next_acquisition,
        )
        .await;
        if let Some(Ok(prefetched)) = prefetched.as_ref() {
            artifact_cleanup.track(&prefetched.acquisition.artifacts);
        }
        let (prepared, next_acquired) = resolve_prepared_step(prepared, prefetched)?;
        send_prepared(&sender, prepared, cancel).await?;
        acquired = next_acquired;
    }
    Ok(())
}

fn resolve_prepared_step(
    prepared: anyhow::Result<SchedulerPreparedBatch>,
    prefetched: Option<anyhow::Result<AcquiredChangedBatch>>,
) -> anyhow::Result<(SchedulerPreparedBatch, AcquiredChangedBatch)> {
    match (prepared, prefetched) {
        (Ok(prepared), Some(Ok(prefetched))) => Ok((prepared, prefetched)),
        (Err(primary), Some(Err(secondary))) => Err(primary.context(format!(
            "overlapped next-batch acquisition also failed: {secondary:#}"
        ))),
        (Err(primary), Some(Ok(_)) | None) => Err(primary),
        (Ok(_), Some(Err(error))) => Err(error),
        (Ok(_), None) => anyhow::bail!("next acquisition was not attempted after preparation"),
    }
}

async fn send_prepared(
    sender: &PreparedBatchSender,
    prepared: SchedulerPreparedBatch,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    if prepared.is_final {
        sender
            .send_final(prepared.prepared, prepared.side_effects, true, cancel)
            .await
    } else {
        sender
            .send(prepared.prepared, prepared.side_effects, cancel)
            .await
    }
}

struct SchedulerPreparedBatch {
    prepared: Vec<PreparedDocument>,
    side_effects: PreparedBatchSideEffects,
    is_final: bool,
}

#[allow(clippy::too_many_arguments)]
async fn prepare(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    generation: &SourceGenerationId,
    acquired: AcquiredChangedBatch,
    archive_requested: bool,
    coordinator: &ProgressCoordinator,
    stage: &mut GenerationStageProgress,
    artifact_cleanup: &mut ArtifactCleanupGuard,
) -> anyhow::Result<SchedulerPreparedBatch> {
    let components = prepare_acquired_components(
        runtime,
        input,
        emitter,
        generation,
        acquired,
        archive_requested,
        coordinator,
        stage,
        artifact_cleanup,
    )
    .await?;
    let prepared = vectorize::prepare_generation_documents(
        runtime,
        input,
        components.documents,
        &components.enrichment_graph,
        generation,
        emitter,
        coordinator,
        &mut stage.pipeline,
        components.is_final,
    )
    .await?;
    Ok(SchedulerPreparedBatch {
        prepared,
        side_effects: components.side_effects,
        is_final: components.is_final,
    })
}
