use super::*;
use std::future::Future;
use tokio_util::sync::CancellationToken;

use crate::source::executor::created_generation::setup::ensure_generation_collection;
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
    if changed_total == 0 {
        return ensure_generation_collection(runtime, input, collection).await;
    }
    ensure_generation_collection(runtime, input, collection).await?;
    super::with_bulk_load(
        runtime,
        input,
        collection,
        "restoring Qdrant indexing after the failed scheduled pipeline also failed",
        process_inner(
            runtime,
            input,
            emitter,
            generation,
            collection,
            diff,
            archive_requested,
            changed_total,
            coordinator,
            stage,
            accumulated,
            artifact_cleanup,
        ),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn process_inner(
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
        collection.clone(),
        receiver,
        accumulated,
        &mut scheduler_progress,
        &cancel,
    );
    join_cancel_on_error(producer, consumer, &cancel).await
}

async fn join_cancel_on_error<Producer, Consumer>(
    producer: Producer,
    consumer: Consumer,
    cancel: &CancellationToken,
) -> anyhow::Result<()>
where
    Producer: Future<Output = anyhow::Result<()>>,
    Consumer: Future<Output = anyhow::Result<()>>,
{
    tokio::pin!(producer);
    tokio::pin!(consumer);
    tokio::select! {
        produced = &mut producer => {
            if produced.is_err() {
                cancel.cancel();
            }
            let consumed = consumer.await;
            resolve_scheduler_results("producer", produced, "consumer", consumed)
        }
        consumed = &mut consumer => {
            if consumed.is_err() {
                cancel.cancel();
            }
            let produced = producer.await;
            resolve_scheduler_results("consumer", consumed, "producer", produced)
        }
    }
}

fn resolve_scheduler_results(
    first_name: &str,
    first: anyhow::Result<()>,
    counterpart_name: &str,
    counterpart: anyhow::Result<()>,
) -> anyhow::Result<()> {
    match (first, counterpart) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) | (Ok(()), Err(primary)) => Err(primary),
        (Err(primary), Err(secondary)) => Err(anyhow::anyhow!(
            "{primary:#}; generation scheduler {counterpart_name} also failed after {first_name} failure: {secondary:#}"
        )),
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
    mut sender: PreparedBatchSender,
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
        anyhow::bail!("scheduled generation had changed items but produced no batches");
    };
    anyhow::ensure!(
        !cancel.is_cancelled(),
        "generation scheduler producer canceled"
    );
    // Once acquisition starts, let it settle so any returned artifacts can be
    // registered with the cleanup guard. Cancellation prevents new admission
    // and channel sends; it must not drop a mutation-bearing provider future.
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
            anyhow::ensure!(
                !cancel.is_cancelled(),
                "generation scheduler producer canceled"
            );
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
            send_prepared(&mut sender, prepared, cancel).await?;
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
        anyhow::ensure!(
            !cancel.is_cancelled(),
            "generation scheduler producer canceled"
        );
        let (sent, prefetched) = process_and_acquire_next(
            input.adapter,
            async {
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
                send_prepared(&mut sender, prepared, cancel).await
            },
            next_acquisition,
        )
        .await;
        if let Some(Ok(prefetched)) = prefetched.as_ref() {
            artifact_cleanup.track(&prefetched.acquisition.artifacts);
        }
        acquired = resolve_sent_step(sent, prefetched)?;
    }
    Ok(())
}

fn resolve_sent_step(
    sent: anyhow::Result<()>,
    prefetched: Option<anyhow::Result<AcquiredChangedBatch>>,
) -> anyhow::Result<AcquiredChangedBatch> {
    match (sent, prefetched) {
        (Ok(()), Some(Ok(prefetched))) => Ok(prefetched),
        (Err(primary), Some(Err(secondary))) => Err(primary.context(format!(
            "overlapped next-batch acquisition also failed: {secondary:#}"
        ))),
        (Err(primary), Some(Ok(_)) | None) => Err(primary),
        (Ok(()), Some(Err(error))) => Err(error),
        (Ok(()), None) => anyhow::bail!("next acquisition was not attempted after preparation"),
    }
}

async fn send_prepared(
    sender: &mut PreparedBatchSender,
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

#[cfg(test)]
#[path = "scheduled_tests.rs"]
mod tests;

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
