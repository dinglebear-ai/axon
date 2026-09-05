use super::*;
use futures_util::{StreamExt, stream};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub(in crate::source::executor) struct PreparedPoolVectorizer {
    pub(super) ready: Option<pipeline::BuiltVectorBatch>,
    pub(super) cumulative: HashMap<DocumentId, DocumentStatus>,
}

#[derive(Debug)]
pub(in crate::source::executor) enum PushOutcome {
    NoPublication,
    StatusesOnly(VectorizeResult),
    Published(VectorizeResult),
}

impl PreparedPoolVectorizer {
    /// Embed a bounded group of sequence-tagged outer pools concurrently, then
    /// build and publish them in input order. TEI's shared request/input gates
    /// remain the authoritative admission boundary; this removes the former
    /// complete-drain barrier between outer pools without relaxing ordering or
    /// retaining more than `pools.len()` pool payloads.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::source::executor) async fn push_many(
        &mut self,
        runtime: &TargetLocalSourceRuntime,
        input: &SourcePipelineInput<'_>,
        collection: CollectionSpec,
        emitter: &SourceEventEmitter,
        coordinator: &ProgressCoordinator,
        pools: Vec<Vec<PreparedDocument>>,
        is_final_group: bool,
        progress: &mut PipelineProgress,
        cancel: &CancellationToken,
    ) -> anyhow::Result<Vec<PushOutcome>> {
        let mut outcomes = Vec::new();
        let mut embedding_work = Vec::new();
        for (sequence, prepared) in pools.into_iter().enumerate() {
            anyhow::ensure!(
                !cancel.is_cancelled(),
                "generation scheduler canceled before vectorization"
            );
            let chunks = prepared
                .iter()
                .map(|document| document.chunks.len() as u64)
                .sum();
            if !input.plan.request.embed || chunks == 0 {
                let result = statuses_only(prepared, DocumentLifecycleStatus::Prepared);
                self.checkpoint(runtime, &result).await?;
                outcomes.push(PushOutcome::StatusesOnly(result));
                continue;
            }
            coordinator
                .report(
                    emitter,
                    PipelinePhase::Batching,
                    progress.batched(chunks),
                    "batching prepared chunks",
                )
                .await;
            let counts = pipeline::begin_embedding(emitter, coordinator, progress).await;
            embedding_work.push((sequence, prepared, counts));
        }

        let final_sequence = embedding_work
            .last()
            .map(|(sequence, _, _)| *sequence)
            .unwrap_or_default();
        let embedded = stream::iter(embedding_work)
            .map(|(sequence, prepared, counts)| async move {
                let result = pipeline::call_embedding(
                    runtime,
                    input,
                    &prepared,
                    PipelinePhase::Embedding,
                    counts,
                )
                .await;
                (sequence, prepared, result)
            })
            .buffered(3);
        tokio::pin!(embedded);
        let mut next = embedded.next().await;
        while let Some((sequence, prepared, embeddings)) = next {
            let mut embeddings = match embeddings {
                Ok(embeddings) => embeddings,
                Err(error) => {
                    if let Some(current) = self.ready.take() {
                        let durable = publish_built_batch(
                            runtime,
                            input,
                            current,
                            emitter,
                            coordinator,
                            progress,
                        )
                        .await?;
                        self.checkpoint(runtime, &durable).await?;
                    }
                    return Err(error);
                }
            };
            pipeline::finish_embedding(coordinator, progress, &embeddings).await;
            let built = pipeline::build_vector_batch(
                prepared,
                collection.clone(),
                &mut embeddings,
                emitter,
                coordinator,
                progress,
                is_final_group && sequence == final_sequence,
            )
            .await?;
            if let Some(current) = self.ready.replace(built) {
                // Keep polling the ordered embedding stream while Qdrant
                // publishes the previously built batch. This removes the
                // TEI-then-Qdrant phase barrier without changing ordering.
                let (published, following) = tokio::join!(
                    publish_built_batch(runtime, input, current, emitter, coordinator, progress),
                    embedded.next(),
                );
                let result = published?;
                self.checkpoint(runtime, &result).await?;
                outcomes.push(PushOutcome::Published(result));
                next = following;
            } else {
                outcomes.push(PushOutcome::NoPublication);
                next = embedded.next().await;
            }
        }
        Ok(outcomes)
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::source::executor) async fn finish(
        &mut self,
        runtime: &TargetLocalSourceRuntime,
        input: &SourcePipelineInput<'_>,
        emitter: &SourceEventEmitter,
        coordinator: &ProgressCoordinator,
        progress: &mut PipelineProgress,
    ) -> anyhow::Result<Option<VectorizeResult>> {
        let Some(ready) = self.ready.take() else {
            return Ok(None);
        };
        let result =
            publish_built_batch(runtime, input, ready, emitter, coordinator, progress).await?;
        self.checkpoint(runtime, &result).await?;
        Ok(Some(result))
    }

    async fn checkpoint(
        &mut self,
        runtime: &TargetLocalSourceRuntime,
        result: &VectorizeResult,
    ) -> anyhow::Result<()> {
        let started = Instant::now();
        let touched = merge_and_collect_touched(&mut self.cumulative, &result.document_statuses);
        write_document_statuses(
            runtime.ledger.as_ref(),
            &touched,
            runtime.document_status_batch_size,
        )
        .await?;
        tracing::info!(
            documents = touched.len(),
            checkpoint_ms = started.elapsed().as_millis() as u64,
            "vector publication checkpoint completed"
        );
        Ok(())
    }
}

fn merge_and_collect_touched(
    cumulative: &mut HashMap<DocumentId, DocumentStatus>,
    statuses: &[DocumentStatus],
) -> Vec<DocumentStatus> {
    merge_cumulative_statuses(cumulative, statuses);
    statuses
        .iter()
        .filter_map(|status| cumulative.get(&status.document_id).cloned())
        .collect()
}

fn merge_cumulative_statuses(
    cumulative: &mut HashMap<DocumentId, DocumentStatus>,
    statuses: &[DocumentStatus],
) {
    for status in statuses {
        if let Some(existing) = cumulative.get_mut(&status.document_id) {
            existing.chunk_count = existing.chunk_count.saturating_add(status.chunk_count);
            existing.vector_point_count = existing
                .vector_point_count
                .saturating_add(status.vector_point_count);
            existing.updated_at = status.updated_at.clone();
            existing.status = status.status;
        } else {
            cumulative.insert(status.document_id.clone(), status.clone());
        }
    }
}

#[cfg(test)]
#[path = "prepared_pool_tests.rs"]
mod tests;
