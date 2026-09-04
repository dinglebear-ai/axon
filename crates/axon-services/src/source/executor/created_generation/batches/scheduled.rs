use super::*;
use std::future::Future;
use tokio_util::sync::CancellationToken;

use crate::source::executor::created_generation::setup::ensure_generation_collection;
use crate::source::executor::generation_work::{
    PreparedBatchSender, PreparedBatchSideEffects, prepared_work_channel_with_byte_budget,
};
use crate::source::executor::progress::PipelineProgress;

pub(super) fn enabled() -> bool {
    std::env::var("AXON_EMBED_SCHEDULER_ENABLED")
        .ok()
        .and_then(|value| value.parse::<bool>().ok())
        .unwrap_or(false)
}

#[derive(Clone, Copy)]
pub(super) struct ScheduledGenerationContext<'a, 'input> {
    pub(super) runtime: &'a TargetLocalSourceRuntime,
    pub(super) input: &'a SourcePipelineInput<'input>,
    pub(super) emitter: &'a SourceEventEmitter,
    pub(super) generation: &'a SourceGenerationId,
    pub(super) collection: &'a CollectionSpec,
    pub(super) diff: &'a SourceManifestDiff,
    pub(super) archive_requested: bool,
    pub(super) changed_total: u64,
    pub(super) coordinator: &'a ProgressCoordinator,
}

pub(super) struct ScheduledGenerationState<'a> {
    pub(super) stage: &'a mut GenerationStageProgress,
    pub(super) accumulated: &'a mut GenerationAccumulator,
    pub(super) artifact_cleanup: &'a mut ArtifactCleanupGuard,
}

// LEARNED: forwarding a dozen positional arguments through each scheduler
// layer made otherwise local changes touch every call site.
// PATTERN: group immutable generation inputs separately from mutable progress
// state, so concurrency boundaries make their borrowing and ownership visible.
pub(super) async fn process(
    context: ScheduledGenerationContext<'_, '_>,
    state: ScheduledGenerationState<'_>,
) -> anyhow::Result<()> {
    if context.changed_total == 0 {
        return ensure_generation_collection(context.runtime, context.input, context.collection)
            .await;
    }
    ensure_generation_collection(context.runtime, context.input, context.collection).await?;
    super::with_bulk_load(
        context.runtime,
        context.input,
        context.collection,
        "restoring Qdrant indexing after the failed scheduled pipeline also failed",
        process_inner(context, state),
    )
    .await
}

async fn process_inner(
    context: ScheduledGenerationContext<'_, '_>,
    state: ScheduledGenerationState<'_>,
) -> anyhow::Result<()> {
    let (sender, receiver) = prepared_work_channel_with_byte_budget(
        context.runtime.embed_pool_max_inputs,
        context.runtime.embed_prepared_byte_budget,
    )?;
    tracing::info!(
        chunk_capacity = context.runtime.embed_pool_max_inputs.saturating_mul(3),
        queue_capacity = 2,
        byte_capacity_kib = context.runtime.embed_prepared_byte_budget.div_ceil(1024),
        "enabled bounded generation embedding scheduler"
    );
    let cancel = CancellationToken::new();
    // Heap-pin both deep pipeline futures before joining them. Keeping either
    // concrete future inline makes the combined debug/test poll frame exceed
    // the default test-thread stack for real scheduled generation paths.
    let producer = Box::pin(produce(
        context,
        state.stage,
        state.artifact_cleanup,
        sender,
        &cancel,
    ));
    let mut scheduler_progress = PipelineProgress::default();
    let consumer = Box::pin(super::super::scheduler::run_generation_scheduler(
        context.runtime,
        context.input,
        context.emitter,
        context.coordinator,
        context.collection.clone(),
        receiver,
        state.accumulated,
        &mut scheduler_progress,
        &cancel,
    ));
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

async fn produce(
    context: ScheduledGenerationContext<'_, '_>,
    stage: &mut GenerationStageProgress,
    artifact_cleanup: &mut ArtifactCleanupGuard,
    mut sender: PreparedBatchSender,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    let acquire_batch_size = acquire_batch_size();
    let first_batch_size = first_acquire_batch_size(acquire_batch_size);
    let changed = usize::try_from(context.changed_total).unwrap_or(usize::MAX);
    let batch_count = if changed <= first_batch_size {
        usize::from(changed > 0)
    } else {
        1 + (changed - first_batch_size).div_ceil(acquire_batch_size)
    };
    let mut batches = batch_changed_diff_ramped(context.diff, first_batch_size, acquire_batch_size)
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
        context.input,
        first,
        context.changed_total,
        stage.acquired_items,
        stage.acquired_documents,
        context.coordinator,
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
            let prepared = prepare(context, acquired, stage, artifact_cleanup).await?;
            send_prepared(&mut sender, prepared, cancel).await?;
            break;
        };
        let next_acquisition = acquire_changed_batch(
            context.input,
            next_batch,
            context.changed_total,
            stage.acquired_items,
            stage.acquired_documents,
            context.coordinator,
            !context.input.adapter.supports_acquisition_prefetch(),
        );
        anyhow::ensure!(
            !cancel.is_cancelled(),
            "generation scheduler producer canceled"
        );
        let (sent, prefetched) = process_and_acquire_next(
            context.input.adapter,
            async {
                let prepared = prepare(context, acquired, stage, artifact_cleanup).await?;
                send_prepared(&mut sender, prepared, cancel).await
            },
            next_acquisition,
        )
        .await;
        if let Some(Ok(prefetched)) = prefetched.as_ref() {
            artifact_cleanup
                .track(&prefetched.acquisition.artifacts)
                .await?;
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

async fn prepare(
    context: ScheduledGenerationContext<'_, '_>,
    acquired: AcquiredChangedBatch,
    stage: &mut GenerationStageProgress,
    artifact_cleanup: &mut ArtifactCleanupGuard,
) -> anyhow::Result<SchedulerPreparedBatch> {
    let components = prepare_acquired_components(
        context.runtime,
        context.input,
        context.emitter,
        context.generation,
        acquired,
        context.archive_requested,
        context.coordinator,
        stage,
        artifact_cleanup,
    )
    .await?;
    let prepared = vectorize::prepare_generation_documents(
        context.runtime,
        context.input,
        components.documents,
        &components.enrichment_graph,
        context.generation,
        context.emitter,
        context.coordinator,
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
