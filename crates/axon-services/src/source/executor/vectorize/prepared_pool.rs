use super::*;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub(in crate::source::executor) struct PreparedPoolVectorizer {
    pub(super) ready: Option<pipeline::BuiltVectorBatch>,
    pub(super) cumulative: HashMap<DocumentId, DocumentStatus>,
}

impl PreparedPoolVectorizer {
    pub(in crate::source::executor) fn has_pending_publication(&self) -> bool {
        self.ready.is_some()
    }

    /// Build the first pool, then overlap every subsequent pool's embedding
    /// with publication of its predecessor. A returned result is durable; a
    /// `None` result remains owned by this vectorizer until the next push or
    /// `finish`.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::source::executor) async fn push(
        &mut self,
        runtime: &TargetLocalSourceRuntime,
        input: &SourcePipelineInput<'_>,
        collection: CollectionSpec,
        emitter: &SourceEventEmitter,
        coordinator: &ProgressCoordinator,
        prepared: Vec<PreparedDocument>,
        progress: &mut PipelineProgress,
        cancel: &CancellationToken,
    ) -> anyhow::Result<Option<VectorizeResult>> {
        if cancel.is_cancelled() {
            anyhow::bail!("generation scheduler canceled before vectorization");
        }
        let chunks = prepared
            .iter()
            .map(|document| document.chunks.len() as u64)
            .sum();
        coordinator
            .report(
                emitter,
                PipelinePhase::Batching,
                progress.batched(chunks),
                "batching prepared chunks",
            )
            .await;
        if !input.plan.request.embed || chunks == 0 {
            let result = statuses_only(prepared, DocumentLifecycleStatus::Prepared);
            self.checkpoint(runtime, &result).await?;
            return Ok(Some(result));
        }

        let Some(current) = self.ready.take() else {
            self.ready = Some(
                embed_and_build_batch(
                    runtime,
                    input,
                    prepared,
                    collection,
                    emitter,
                    coordinator,
                    progress,
                    false,
                )
                .await?,
            );
            return Ok(None);
        };
        let mut durable = VectorizeResult::default();
        let next = publish_and_build_next(
            runtime,
            input,
            current,
            prepared,
            collection,
            emitter,
            coordinator,
            progress,
            &mut durable,
            false,
        )
        .await;
        // `publish_and_build_next` absorbs a successful current upsert before
        // it surfaces a speculative next-embedding failure. Persist that
        // durable publication regardless of the speculative result so Qdrant
        // and the document-status ledger cannot diverge on this error path.
        self.checkpoint(runtime, &durable).await?;
        self.ready = Some(next?);
        Ok(Some(durable))
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
        merge_cumulative_statuses(&mut self.cumulative, &result.document_statuses);
        let mut statuses = self.cumulative.values().cloned().collect::<Vec<_>>();
        statuses.sort_by(|left, right| left.document_id.0.cmp(&right.document_id.0));
        write_document_statuses(runtime.ledger.as_ref(), &statuses).await
    }
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
