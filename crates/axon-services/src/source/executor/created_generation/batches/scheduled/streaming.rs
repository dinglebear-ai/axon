//! Bounded, completion-ordered acquisition waves and preparation.
use super::*;
use crate::source::executor::progress::AcquisitionBatchProgress;
use axon_adapters::{AcquisitionProgressSink, StreamedAcquisition};
use futures_util::StreamExt as _;
use std::collections::BTreeMap;
use tokio::sync::mpsc;

pub(super) async fn prepare(
    context: ScheduledGenerationContext<'_, '_>,
    stage: &mut GenerationStageProgress,
    artifact_cleanup: &mut ArtifactCleanupGuard,
    sender: &mut PreparedBatchSender,
    cancel: &CancellationToken,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        !cancel.is_cancelled(),
        "generation scheduler producer canceled"
    );
    let reporter = context.coordinator.acquisition_batch(
        context.changed_total,
        context.changed_total,
        stage.acquired_items,
        stage.acquired_documents,
        true,
    );
    // Two ready acquisitions may queue while one is prepared. The acquisition
    // future owns the last sender so completion always closes this channel.
    let (tx, rx) = mpsc::channel(2);
    let acquire = acquire_waves(
        context.input,
        context.diff,
        tx,
        cancel,
        context.runtime.document_prepare_max_in_flight_bytes,
    );
    let mut state = Preparation {
        context,
        stage,
        artifact_cleanup,
        sender,
        cancel,
    };
    let consume = state.consume(rx, &reporter);
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

struct ChannelStreamSink(mpsc::Sender<StreamedAcquisition>);
#[async_trait::async_trait]
impl axon_adapters::AcquisitionStreamSink for ChannelStreamSink {
    async fn send(&self, item: StreamedAcquisition) -> Result<(), ApiError> {
        self.0.send(item).await.map_err(|_| {
            ApiError::new(
                "source.acquire.stream_closed",
                ErrorStage::Fetching,
                "scheduled acquisition stream closed before provider settlement",
            )
        })
    }
}

async fn acquire_waves(
    input: &SourcePipelineInput<'_>,
    diff: &SourceManifestDiff,
    tx: mpsc::Sender<StreamedAcquisition>,
    cancel: &CancellationToken,
    preparation_bytes: usize,
) -> Result<(), ApiError> {
    if super::super::budget::file_backed(&input.plan) {
        return acquire_files(input, diff, tx, cancel, preparation_bytes).await;
    }
    let size = acquire_batch_size();
    let waves = batch_changed_diff_ramped(diff, first_acquire_batch_size(size), size);
    // One next-wave prefetch only for adapters that opt into it. Both waves
    // share the ready queue and durable provider admission.
    let width = if input.adapter.supports_acquisition_prefetch() {
        2
    } else {
        1
    };
    let mut pending = futures_util::stream::iter(waves)
        .map(|diff| {
            let sink = ChannelStreamSink(tx.clone());
            async move {
                if cancel.is_cancelled() {
                    return Err(ApiError::new(
                        "source.acquire.canceled",
                        ErrorStage::Fetching,
                        "acquisition canceled before wave admission",
                    ));
                }
                input
                    .adapter
                    .acquire_streaming(&input.plan, &diff, None, &sink)
                    .await
            }
        })
        .buffer_unordered(width);
    let mut first_error = None;
    while let Some(result) = pending.next().await {
        if let Err(error) = result {
            cancel.cancel();
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

async fn acquire_files(
    input: &SourcePipelineInput<'_>,
    diff: &SourceManifestDiff,
    tx: mpsc::Sender<StreamedAcquisition>,
    cancel: &CancellationToken,
    preparation_bytes: usize,
) -> Result<(), ApiError> {
    let size = acquire_batch_size();
    let mut budget = super::super::budget::AcquisitionBudget::new(&input.plan, preparation_bytes);
    for (ordinal, batch) in super::super::budget::changed_batches(
        &input.plan,
        diff,
        first_acquire_batch_size(size),
        size,
    )
    .enumerate()
    {
        if cancel.is_cancelled() {
            return Err(ApiError::new(
                "source.acquire.canceled",
                ErrorStage::Fetching,
                "acquisition canceled before wave admission",
            ));
        }
        let items_attempted = (batch.diff.added.len() + batch.diff.modified.len()) as u64;
        let plan = budget.plan(&input.plan);
        let acquisition = input.adapter.acquire(&plan, &batch.diff).await?;
        budget.charge(&acquisition)?;
        tx.send(StreamedAcquisition {
            ordinal,
            is_final: batch.is_final,
            items_attempted,
            acquisition,
        })
        .await
        .map_err(|_| {
            ApiError::new(
                "source.acquire.stream_closed",
                ErrorStage::Fetching,
                "scheduled acquisition stream closed before provider settlement",
            )
        })?;
    }
    Ok(())
}

struct Preparation<'a, 'input> {
    context: ScheduledGenerationContext<'a, 'input>,
    stage: &'a mut GenerationStageProgress,
    artifact_cleanup: &'a mut ArtifactCleanupGuard,
    sender: &'a mut PreparedBatchSender,
    cancel: &'a CancellationToken,
}

impl Preparation<'_, '_> {
    async fn consume(
        &mut self,
        mut rx: mpsc::Receiver<StreamedAcquisition>,
        reporter: &AcquisitionBatchProgress<'_>,
    ) -> anyhow::Result<u64> {
        let index = DiffIndex::new(self.context.diff);
        let mut first_error = None;
        let mut items_done = 0_u64;
        let mut documents_done = 0_u64;
        while let Some(streamed) = rx.recv().await {
            // Retain cleanup ownership even after an earlier preparation error.
            if let Err(error) = self
                .artifact_cleanup
                .track(&streamed.acquisition.artifacts)
                .await
            {
                first_error.get_or_insert_with(|| anyhow::Error::new(error));
                self.cancel.cancel();
            }
            if first_error.is_some() {
                continue;
            }
            let documents = streamed.acquisition.fetched_items.len() as u64;
            items_done = items_done.saturating_add(streamed.items_attempted);
            documents_done = documents_done.saturating_add(documents);
            self.stage.acquired_items = self
                .stage
                .acquired_items
                .saturating_add(streamed.items_attempted);
            self.stage.acquired_documents = self.stage.acquired_documents.saturating_add(documents);
            reporter
                .report(axon_adapters::AcquisitionProgress {
                    items_total: self.context.changed_total,
                    items_done,
                    documents_done,
                })
                .await;
            let acquired = AcquiredChangedBatch {
                batch: ChangedBatch {
                    diff: index.for_items(&streamed.acquisition.manifest.items),
                    // Wave and manifest ordinals are identities, not completion.
                    is_final: items_done == self.context.changed_total,
                },
                acquisition: streamed.acquisition,
                items: streamed.items_attempted,
                documents,
            };
            let context = self.context;
            if let Err(error) = prepare_and_send(
                context.runtime,
                context.input,
                context.emitter,
                context.generation,
                acquired,
                context.archive_requested,
                context.coordinator,
                self.stage,
                self.artifact_cleanup,
                self.sender,
                self.cancel,
            )
            .await
            {
                first_error = Some(error);
                self.cancel.cancel();
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        anyhow::ensure!(
            items_done == self.context.changed_total,
            "acquisition settled {items_done} of {} changed items",
            self.context.changed_total
        );
        Ok(documents_done)
    }
}

struct DiffIndex<'a> {
    template: SourceManifestDiff,
    added: BTreeMap<&'a SourceItemKey, &'a ManifestItem>,
    modified: BTreeMap<&'a SourceItemKey, &'a ManifestItem>,
}

impl<'a> DiffIndex<'a> {
    fn new(diff: &'a SourceManifestDiff) -> Self {
        Self {
            template: SourceManifestDiff {
                header: diff.header.clone(),
                source_id: diff.source_id.clone(),
                previous_generation: diff.previous_generation.clone(),
                next_generation: diff.next_generation.clone(),
                added: Vec::new(),
                modified: Vec::new(),
                removed: Vec::new(),
                unchanged: Vec::new(),
                skipped: Vec::new(),
                failed: Vec::new(),
                counts: DiffCounts {
                    added: 0,
                    modified: 0,
                    removed: 0,
                    unchanged: 0,
                    skipped: 0,
                    failed: 0,
                },
            },
            added: diff
                .added
                .iter()
                .map(|item| (&item.source_item_key, item))
                .collect(),
            modified: diff
                .modified
                .iter()
                .map(|item| (&item.source_item_key, item))
                .collect(),
        }
    }

    fn for_items(&self, items: &[ManifestItem]) -> SourceManifestDiff {
        let mut diff = self.template.clone();
        for item in items {
            if let Some(original) = self.added.get(&item.source_item_key) {
                diff.added.push((*original).clone());
            } else if let Some(original) = self.modified.get(&item.source_item_key) {
                diff.modified.push((*original).clone());
            }
        }
        diff.counts.added = diff.added.len() as u64;
        diff.counts.modified = diff.modified.len() as u64;
        diff
    }
}

#[cfg(test)]
#[path = "streaming_tests.rs"]
mod tests;
