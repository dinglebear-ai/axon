use axon_api::source::*;
use axon_ledger::store::LedgerStore;
use axon_vectors::store::VectorStore;

use super::{WEB_LEASE_TTL_SECONDS, WebAdapterRun, WebSourceIndexInput, source_summary, timestamp};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PublishVectorStats {
    new_points_written: u64,
    unchanged_points_written: u64,
}

impl PublishVectorStats {
    pub(super) fn total_points_written(self) -> u64 {
        self.new_points_written + self.unchanged_points_written
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct GenerationDocumentCounts {
    pub(super) discovered: u64,
    pub(super) prepared: u64,
    pub(super) embedded: u64,
    pub(super) published: u64,
    pub(super) failed: u64,
}

pub(super) async fn ensure_lease_before_publish(
    ledger: &dyn LedgerStore,
    input: &WebSourceIndexInput,
    lease: &LeaseGuard,
) -> anyhow::Result<()> {
    if ledger
        .heartbeat_lease(
            lease.lease_id.clone(),
            input.owner_id.clone(),
            WEB_LEASE_TTL_SECONDS,
        )
        .await?
        .is_some()
    {
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "web source refresh lost lease before publish"
    ))
}

pub(super) async fn complete_generation(
    ledger: &dyn LedgerStore,
    generation: SourceGeneration,
    diff: &SourceManifestDiff,
    counts: GenerationDocumentCounts,
) -> anyhow::Result<SourceGeneration> {
    Ok(ledger
        .complete_generation(SourceGeneration {
            status: LifecycleStatus::Completed,
            publish_state: PublishState::Publishing,
            item_counts: ItemCounts {
                added: diff.counts.added,
                modified: diff.counts.modified,
                removed: diff.counts.removed,
                unchanged: diff.counts.unchanged,
                failed: diff.counts.failed,
            },
            document_counts: DocumentCounts {
                discovered: counts.discovered,
                prepared: counts.prepared,
                embedded: counts.embedded,
                published: counts.published,
                failed: counts.failed,
            },
            ..generation
        })
        .await?)
}

pub(super) async fn mark_vectors_for_completed_generation(
    vector_store: &dyn VectorStore,
    collection: &CollectionSpec,
    source_id: &SourceId,
    completed: &SourceGeneration,
    diff: &SourceManifestDiff,
    expected_new_points: u64,
) -> anyhow::Result<PublishVectorStats> {
    let new_write = vector_store
        .mark_generation_committed(
            collection.collection.clone(),
            source_id.clone(),
            completed.generation.clone(),
        )
        .await?;
    ensure_full_write("mark_generation_committed", expected_new_points, &new_write)?;
    let new_points_written = new_write.points_written;
    let unchanged_points_written = match mark_unchanged_vectors_committed(
        vector_store,
        collection,
        source_id,
        completed,
        diff,
    )
    .await
    {
        Ok(points_written) => points_written,
        Err(err) => {
            let rollback =
                rollback_new_generation_vectors(vector_store, collection, source_id, completed)
                    .await
                    .err();
            let mut err = anyhow::Error::new(err);
            if let Some(rollback) = rollback {
                err = err.context(format!(
                    "also failed to rollback committed vector generation {} from collection {}: {rollback}",
                    completed.generation.0, collection.collection
                ));
            }
            return Err(err);
        }
    };
    Ok(PublishVectorStats {
        new_points_written,
        unchanged_points_written,
    })
}

pub(super) async fn publish_generation_without_vectors(
    ledger: &dyn LedgerStore,
    completed: &SourceGeneration,
) -> anyhow::Result<SourceGeneration> {
    match ledger
        .publish_generation(PublishGenerationRequest {
            source_id: completed.source_id.clone(),
            generation: completed.generation.clone(),
            expected_previous_generation: completed.previous_generation.clone(),
        })
        .await
    {
        Ok(published) => Ok(published),
        Err(err) => {
            if let Err(fail) = ledger.fail_generation(completed.clone()).await {
                return Err(anyhow::Error::new(err).context(format!(
                    "also failed to mark source generation failed: {fail}"
                )));
            }
            Err(err.into())
        }
    }
}

pub(super) async fn publish_generation_and_rollback_vectors(
    ledger: &dyn LedgerStore,
    vector_store: &dyn VectorStore,
    collection: &CollectionSpec,
    completed: &SourceGeneration,
) -> anyhow::Result<SourceGeneration> {
    match ledger
        .publish_generation(PublishGenerationRequest {
            source_id: completed.source_id.clone(),
            generation: completed.generation.clone(),
            expected_previous_generation: completed.previous_generation.clone(),
        })
        .await
    {
        Ok(published) => Ok(published),
        Err(err) => {
            let mut cleanup_errors = Vec::new();
            if let Err(rollback) = rollback_new_generation_vectors(
                vector_store,
                collection,
                &completed.source_id,
                completed,
            )
            .await
            {
                cleanup_errors.push(format!(
                    "also failed to rollback committed vector generation {} from collection {}: {rollback}",
                    completed.generation.0, collection.collection
                ));
            }
            if let Err(fail) = ledger.fail_generation(completed.clone()).await {
                cleanup_errors.push(format!(
                    "also failed to mark source generation failed: {fail}"
                ));
            }
            if cleanup_errors.is_empty() {
                Err(err.into())
            } else {
                Err(anyhow::Error::new(err).context(cleanup_errors.join("; ")))
            }
        }
    }
}

pub(super) async fn fail_generation_and_rollback_vectors(
    ledger: &dyn LedgerStore,
    vector_store: &dyn VectorStore,
    collection: &CollectionSpec,
    generation: SourceGeneration,
    cause: anyhow::Error,
) -> anyhow::Error {
    let mut cleanup_errors = Vec::new();
    if let Err(rollback) = rollback_new_generation_vectors(
        vector_store,
        collection,
        &generation.source_id,
        &generation,
    )
    .await
    {
        cleanup_errors.push(format!(
            "also failed to rollback vector generation {} from collection {}: {rollback}",
            generation.generation.0, collection.collection
        ));
    }
    if let Err(fail) = ledger.fail_generation(generation).await {
        cleanup_errors.push(format!(
            "also failed to mark source generation failed: {fail}"
        ));
    }
    if cleanup_errors.is_empty() {
        cause
    } else {
        cause.context(cleanup_errors.join("; "))
    }
}

pub(super) async fn fail_generation(
    ledger: &dyn LedgerStore,
    generation: SourceGeneration,
    cause: anyhow::Error,
) -> anyhow::Error {
    match ledger.fail_generation(generation).await {
        Ok(_) => cause,
        Err(fail) => cause.context(format!(
            "also failed to mark source generation failed: {fail}"
        )),
    }
}

pub(super) fn completed_source_summary(
    input: &WebSourceIndexInput,
    run: &WebAdapterRun,
    item_count: u64,
    diff: &SourceManifestDiff,
    points_written: u64,
) -> SourceSummary {
    let mut summary = source_summary(input, run);
    summary.status = LifecycleStatus::Completed;
    summary.counts = SourceCounts {
        items_total: item_count,
        items_changed: diff.counts.added + diff.counts.modified + diff.counts.removed,
        documents_total: item_count,
        chunks_total: points_written,
        vector_points_total: points_written,
        bytes_total: diff
            .added
            .iter()
            .chain(diff.modified.iter())
            .chain(diff.unchanged.iter())
            .map(|item| item.size_bytes.unwrap_or(0))
            .sum(),
    };
    summary.updated_at = timestamp();
    summary
}

fn ensure_full_write(
    operation: &str,
    expected_points: u64,
    write: &VectorStoreWriteResult,
) -> anyhow::Result<()> {
    if write.points_attempted != write.points_written || write.points_written != expected_points {
        return Err(anyhow::anyhow!(
            "{operation} wrote {} of {} attempted points; expected {expected_points}",
            write.points_written,
            write.points_attempted
        ));
    }
    Ok(())
}

async fn mark_unchanged_vectors_committed(
    vector_store: &dyn VectorStore,
    collection: &CollectionSpec,
    source_id: &SourceId,
    completed: &SourceGeneration,
    diff: &SourceManifestDiff,
) -> Result<u64, ApiError> {
    let Some(previous_generation) = completed.previous_generation.clone() else {
        return Ok(0);
    };
    if diff.unchanged.is_empty() {
        return Ok(0);
    }
    let write = vector_store
        .mark_unchanged_items_committed(
            collection.collection.clone(),
            source_id.clone(),
            previous_generation,
            completed.generation.clone(),
            diff.unchanged
                .iter()
                .map(|item| item.source_item_key.clone())
                .collect(),
        )
        .await?;
    ensure_full_write(
        "mark_unchanged_items_committed",
        write.points_attempted,
        &write,
    )
    .map_err(|err| {
        ApiError::new(
            "vector.commit_short_write",
            ErrorStage::Publishing,
            err.to_string(),
        )
    })?;
    Ok(write.points_written)
}

async fn rollback_new_generation_vectors(
    vector_store: &dyn VectorStore,
    collection: &CollectionSpec,
    source_id: &SourceId,
    completed: &SourceGeneration,
) -> Result<(), ApiError> {
    vector_store
        .delete(VectorDeleteSelector::Generation {
            collection: collection.collection.clone(),
            source_id: source_id.clone(),
            generation: completed.generation.clone(),
        })
        .await
        .map(|_| ())
}
