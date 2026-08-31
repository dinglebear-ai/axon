use super::*;
use std::future::Future;
use tokio_util::sync::CancellationToken;

use crate::source::executor::generation_work::{
    PreparedBatchSender, PreparedBatchSideEffects, prepared_work_channel,
};
use crate::source::executor::progress::PipelineProgress;

#[cfg(not(test))]
const COUNTERPART_CANCEL_GRACE: std::time::Duration = std::time::Duration::from_secs(10);
#[cfg(test)]
const COUNTERPART_CANCEL_GRACE: std::time::Duration = std::time::Duration::from_millis(50);

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
    let producer = Box::pin(produce(
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
    ));
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
    let batches = batch_changed_diff_ramped(diff, first_batch_size, acquire_batch_size)
        .enumerate()
        .map(|(index, diff)| ChangedBatch {
            diff,
            is_final: index + 1 == batch_count,
        });
    for batch in batches {
        anyhow::ensure!(
            !cancel.is_cancelled(),
            "generation scheduler producer canceled"
        );
        stream_prepare_batch(
            runtime,
            input,
            emitter,
            generation,
            batch,
            archive_requested,
            changed_total,
            coordinator,
            stage,
            artifact_cleanup,
            &mut sender,
            cancel,
        )
        .await?;
    }
    Ok(())
}

struct ChannelStreamSink(tokio::sync::mpsc::Sender<axon_adapters::StreamedAcquisition>);

#[async_trait::async_trait]
impl axon_adapters::AcquisitionStreamSink for ChannelStreamSink {
    async fn send(
        &self,
        acquisition: axon_adapters::StreamedAcquisition,
    ) -> std::result::Result<(), axon_api::source::ApiError> {
        self.0.send(acquisition).await.map_err(|_| {
            axon_api::source::ApiError::new(
                "source.acquire.stream_closed",
                axon_api::source::ErrorStage::Fetching,
                "scheduled acquisition stream closed before provider settlement",
            )
        })
    }
}

#[allow(clippy::too_many_arguments)]
async fn stream_prepare_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    generation: &SourceGenerationId,
    batch: ChangedBatch,
    archive_requested: bool,
    changed_total: u64,
    coordinator: &ProgressCoordinator,
    stage: &mut GenerationStageProgress,
    artifact_cleanup: &mut ArtifactCleanupGuard,
    sender: &mut PreparedBatchSender,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    let item_count = batch
        .diff
        .added
        .len()
        .saturating_add(batch.diff.modified.len()) as u64;
    let reporter = coordinator.acquisition_batch(
        changed_total,
        item_count,
        stage.acquired_items,
        stage.acquired_documents,
        true,
    );
    // Two ready acquisitions may queue while one is prepared. This preserves
    // overlap without allowing the adapter to outrun scheduler backpressure.
    let (tx, mut rx) = tokio::sync::mpsc::channel(2);
    // The acquisition future must own the last sender. Keeping `sink` in this
    // outer scope leaves the channel open after the adapter returns, so the
    // consumer waits forever for another item and the job is eventually
    // recovered as stale.
    let acquire = async {
        let sink = ChannelStreamSink(tx);
        input
            .adapter
            .acquire_streaming(&input.plan, &batch.diff, Some(&reporter), &sink)
            .await
    };
    let consume = async {
        let mut first_error = None;
        let mut batch_documents = 0_u64;
        while let Some(streamed) = rx.recv().await {
            if first_error.is_some() {
                continue;
            }
            let documents = streamed.acquisition.fetched_items.len() as u64;
            batch_documents = batch_documents.saturating_add(documents);
            stage.acquired_items = stage
                .acquired_items
                .saturating_add(streamed.items_attempted);
            stage.acquired_documents = stage.acquired_documents.saturating_add(documents);
            let item_key = streamed
                .acquisition
                .manifest
                .items
                .first()
                .map(|item| item.source_item_key.clone());
            let mut streamed_diff = batch.diff.clone();
            streamed_diff
                .added
                .retain(|item| Some(&item.source_item_key) == item_key.as_ref());
            streamed_diff
                .modified
                .retain(|item| Some(&item.source_item_key) == item_key.as_ref());
            streamed_diff.removed.clear();
            streamed_diff.unchanged.clear();
            streamed_diff.skipped.clear();
            streamed_diff.failed.clear();
            let acquired = AcquiredChangedBatch {
                batch: ChangedBatch {
                    diff: streamed_diff,
                    is_final: batch.is_final && streamed.is_final,
                },
                acquisition: streamed.acquisition,
                items: streamed.items_attempted,
                documents,
            };
            if let Err(error) = prepare_and_send(
                runtime,
                input,
                emitter,
                generation,
                acquired,
                archive_requested,
                coordinator,
                stage,
                artifact_cleanup,
                sender,
                cancel,
            )
            .await
            {
                // Keep draining so the adapter settles all already-admitted
                // provider futures; report the first preparation failure.
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(batch_documents), Err)
    };
    let (acquired, consumed) = tokio::join!(acquire, consume);
    if let Ok(documents) = consumed.as_ref() {
        reporter.complete(*documents).await;
    }
    match (consumed, acquired) {
        (Err(primary), Err(secondary)) => Err(primary.context(format!(
            "streamed acquisition also failed while settling: {secondary}"
        ))),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(_), Err(error)) => Err(error.into()),
        (Ok(_), Ok(())) => Ok(()),
    }
}

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
        let prepared = vectorize::prepare_generation_documents(
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
        sender
            .send_final(
                prepared,
                side_effects
                    .take()
                    .unwrap_or_else(PreparedBatchSideEffects::empty),
                is_final,
                cancel,
            )
            .await?;
    }
    Ok(())
}
