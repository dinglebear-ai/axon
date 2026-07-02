use axon_api::source::*;
use axon_ledger::store::LedgerStore;
use axon_vectors::store::VectorStore;

use super::git_source_adapter::{GitAdapterRun, source_summary, timestamp};
use super::git_source_progress::{GitSourceProgress, progress_error_context};
use super::{GIT_LEASE_TTL_SECONDS, GitSourceIndexInput};

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

pub(super) async fn ensure_lease_before_publish(
    ledger: &dyn LedgerStore,
    input: &GitSourceIndexInput,
    lease: &LeaseGuard,
    generation: SourceGeneration,
) -> anyhow::Result<()> {
    let heartbeat = ledger
        .heartbeat_lease(
            lease.lease_id.clone(),
            input.owner_id.clone(),
            GIT_LEASE_TTL_SECONDS,
        )
        .await?;
    if heartbeat.is_some() {
        return Ok(());
    }
    if let Err(fail_err) = ledger.fail_generation(generation).await {
        return Err(anyhow::anyhow!(
            "git source refresh lost lease before publish and failed to mark generation failed: {fail_err}"
        ));
    }
    Err(anyhow::anyhow!(
        "git source refresh lost lease before publish"
    ))
}

pub(super) fn completed_source_summary(
    input: &GitSourceIndexInput,
    run: &GitAdapterRun,
    item_count: u64,
    diff: &SourceManifestDiff,
    publish_stats: &PublishVectorStats,
) -> SourceSummary {
    let mut summary = source_summary(input, run);
    summary.status = LifecycleStatus::Completed;
    let total_points = publish_stats.total_points_written();
    summary.counts = SourceCounts {
        items_total: item_count,
        items_changed: diff.counts.added + diff.counts.modified + diff.counts.removed,
        documents_total: item_count,
        chunks_total: total_points,
        vector_points_total: total_points,
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

pub(super) async fn complete_generation(
    ledger: &dyn LedgerStore,
    generation: SourceGeneration,
    diff: &SourceManifestDiff,
    discovered_count: u64,
) -> anyhow::Result<SourceGeneration> {
    let completed = SourceGeneration {
        status: LifecycleStatus::Completed,
        publish_state: PublishState::Publishing,
        published_at: None,
        item_counts: ItemCounts {
            added: diff.counts.added,
            modified: diff.counts.modified,
            removed: diff.counts.removed,
            unchanged: diff.counts.unchanged,
            failed: diff.counts.failed,
        },
        document_counts: DocumentCounts {
            discovered: discovered_count,
            prepared: discovered_count,
            embedded: discovered_count,
            published: discovered_count,
            failed: 0,
        },
        ..generation
    };
    Ok(ledger.complete_generation(completed).await?)
}

pub(super) async fn mark_vectors_for_completed_generation(
    vector_store: &dyn VectorStore,
    collection: &CollectionSpec,
    source_id: &SourceId,
    completed: &SourceGeneration,
    diff: &SourceManifestDiff,
    progress: Option<&dyn GitSourceProgress>,
) -> anyhow::Result<PublishVectorStats> {
    let new_points_written = match vector_store
        .mark_generation_committed(
            collection.collection.clone(),
            source_id.clone(),
            completed.generation.clone(),
        )
        .await
    {
        Ok(result) => result.points_written,
        Err(err) => {
            let progress_context =
                progress_error_context(progress, PipelinePhase::Publishing, &err).await;
            let mut err = anyhow::Error::new(err);
            if let Some(context) = progress_context {
                err = err.context(context);
            }
            return Err(err);
        }
    };
    let unchanged_points_written = match mark_unchanged_vectors_committed(
        vector_store,
        collection,
        source_id,
        completed,
        diff,
        completed.generation.clone(),
    )
    .await
    {
        Ok(points_written) => points_written,
        Err(err) => {
            let progress_context =
                progress_error_context(progress, PipelinePhase::Publishing, &err).await;
            let rollback_error =
                rollback_new_generation_vectors(vector_store, collection, source_id, completed)
                    .await
                    .err();
            let mut err = anyhow::Error::new(err);
            if let Some(context) = progress_context {
                err = err.context(context);
            }
            if let Some(rollback_error) = rollback_error {
                err = err.context(format!(
                    "also failed to rollback committed vector generation {} from collection {}: {rollback_error}",
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

pub(super) async fn publish_generation_and_rollback_vectors(
    ledger: &dyn LedgerStore,
    vector_store: &dyn VectorStore,
    collection: &CollectionSpec,
    completed: &SourceGeneration,
) -> anyhow::Result<SourceGeneration> {
    match publish_completed_generation(ledger, completed.clone()).await {
        Ok(published) => Ok(published),
        Err(err) => {
            let mut cleanup_errors = Vec::new();
            if let Err(rollback_err) = rollback_new_generation_vectors(
                vector_store,
                collection,
                &completed.source_id,
                completed,
            )
            .await
            {
                cleanup_errors.push(format!(
                    "also failed to rollback committed vector generation {} from collection {}: {rollback_err}",
                    completed.generation.0, collection.collection
                ));
            }
            if let Err(fail_err) = mark_completed_generation_failed(ledger, completed.clone()).await
            {
                cleanup_errors.push(format!(
                    "also failed to mark source generation failed: {fail_err}"
                ));
            }
            if !cleanup_errors.is_empty() {
                return Err(err.context(cleanup_errors.join("; ")));
            }
            Err(err)
        }
    }
}

pub(super) async fn mark_completed_generation_failed(
    ledger: &dyn LedgerStore,
    completed: SourceGeneration,
) -> anyhow::Result<()> {
    ledger.fail_generation(completed).await?;
    Ok(())
}

pub(super) async fn rollback_new_generation_vectors(
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

async fn publish_completed_generation(
    ledger: &dyn LedgerStore,
    completed: SourceGeneration,
) -> anyhow::Result<SourceGeneration> {
    Ok(ledger
        .publish_generation(PublishGenerationRequest {
            source_id: completed.source_id.clone(),
            generation: completed.generation.clone(),
            expected_previous_generation: completed.previous_generation.clone(),
        })
        .await?)
}

async fn mark_unchanged_vectors_committed(
    vector_store: &dyn VectorStore,
    collection: &CollectionSpec,
    source_id: &SourceId,
    completed: &SourceGeneration,
    diff: &SourceManifestDiff,
    committed_generation: SourceGenerationId,
) -> Result<u64, ApiError> {
    let Some(previous_generation) = completed.previous_generation.clone() else {
        return Ok(0);
    };
    if diff.unchanged.is_empty() {
        return Ok(0);
    }
    vector_store
        .mark_unchanged_items_committed(
            collection.collection.clone(),
            source_id.clone(),
            previous_generation,
            committed_generation,
            unchanged_item_keys(diff),
        )
        .await
        .map(|result| result.points_written)
}

fn unchanged_item_keys(diff: &SourceManifestDiff) -> Vec<SourceItemKey> {
    diff.unchanged
        .iter()
        .map(|item| item.source_item_key.clone())
        .collect()
}
