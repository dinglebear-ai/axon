use super::*;
use std::future::Future;
use tokio_util::sync::CancellationToken;

use crate::source::executor::generation_work::{
    PreparedBatchSender, PreparedBatchSideEffects, prepared_work_channel_with_byte_budget,
};
use crate::source::executor::progress::PipelineProgress;

#[cfg(not(test))]
const COUNTERPART_CANCEL_GRACE: std::time::Duration = std::time::Duration::from_secs(10);
#[cfg(test)]
const COUNTERPART_CANCEL_GRACE: std::time::Duration = std::time::Duration::from_millis(50);

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
        return Ok(());
    }
    process_inner(context, state).await
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
                let consumed = tokio::time::timeout(COUNTERPART_CANCEL_GRACE, &mut consumer)
                    .await
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("consumer cancellation did not settle within {} ms", COUNTERPART_CANCEL_GRACE.as_millis())));
                return resolve_scheduler_results("producer", produced, "consumer", consumed);
            }
            let consumed = consumer.await;
            resolve_scheduler_results("producer", produced, "consumer", consumed)
        }
        consumed = &mut consumer => {
            if consumed.is_err() {
                cancel.cancel();
                let produced = tokio::time::timeout(COUNTERPART_CANCEL_GRACE, &mut producer)
                    .await
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("producer cancellation did not settle within {} ms", COUNTERPART_CANCEL_GRACE.as_millis())));
                return resolve_scheduler_results("consumer", consumed, "producer", produced);
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
        (Err(primary), Err(secondary)) if is_cancellation_error(&primary) && !is_cancellation_error(&secondary) => {
            Err(secondary.context(format!("generation scheduler {first_name} stopped: {primary:#}")))
        }
        (Err(primary), Err(secondary)) => Err(primary.context(format!(
            "generation scheduler {counterpart_name} also failed after {first_name} failure: {secondary:#}"
        ))),
    }
}

fn is_cancellation_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        if let Some(error) = cause.downcast_ref::<axon_api::source::ApiError>() {
            return matches!(
                error.code.0.as_str(),
                "source.acquire.canceled" | "source.acquire.stream_closed"
            );
        }
        matches!(
            cause.to_string().as_str(),
            "generation scheduler producer canceled"
                | "generation scheduler canceled before vectorization"
                | "prepared work send canceled"
                | "prepared work receiver closed"
        )
    })
}

async fn produce(
    context: ScheduledGenerationContext<'_, '_>,
    stage: &mut GenerationStageProgress,
    artifact_cleanup: &mut ArtifactCleanupGuard,
    mut sender: PreparedBatchSender,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    streaming::prepare(context, stage, artifact_cleanup, &mut sender, cancel).await
}

#[path = "scheduled/streaming.rs"]
mod streaming;

#[cfg(test)]
#[path = "scheduled_tests.rs"]
mod tests;

#[allow(clippy::too_many_arguments)]
async fn prepare_and_send(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    generation: &SourceGenerationId,
    acquired: AcquiredChangedBatch,
    archive_requested: bool,
    coordinator: &ProgressCoordinator,
    stage: &mut GenerationStageProgress,
    artifact_cleanup: &mut ArtifactCleanupGuard,
    sender: &mut PreparedBatchSender,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
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
    let mut batches =
        vectorize::generation_document_batches(components.documents, runtime.document_batch_size)
            .peekable();
    let mut side_effects = Some(components.side_effects);
    if batches.peek().is_none() {
        return sender
            .send_final(
                Vec::new(),
                side_effects.take().expect("side effects available"),
                components.is_final,
                cancel,
            )
            .await;
    }
    while let Some(documents) = batches.next() {
        let is_final = components.is_final && batches.peek().is_none();
        let (prepared, skipped) = vectorize::prepare_generation_documents(
            runtime,
            input,
            documents,
            &components.enrichment_graph,
            generation,
            emitter,
            coordinator,
            &mut stage.pipeline,
            is_final,
        )
        .await?;
        let mut effects = side_effects
            .take()
            .unwrap_or_else(PreparedBatchSideEffects::empty);
        effects.skipped_statuses = skipped.document_statuses;
        sender
            .send_final(prepared, effects, is_final, cancel)
            .await?;
    }
    Ok(())
}
