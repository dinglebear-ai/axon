use axon_api::source::*;
use axon_ledger::store::LedgerStore;
use axon_vectors::store::VectorStore;

use super::vectorize::VectorizeResult;
use super::{NonWebPipelineInput, SOURCE_LEASE_TTL_SECONDS, timestamp};

pub(super) async fn ensure_lease(
    ledger: &dyn LedgerStore,
    input: &NonWebPipelineInput<'_>,
    lease: &LeaseGuard,
) -> anyhow::Result<()> {
    if ledger
        .heartbeat_lease(
            lease.lease_id.clone(),
            input.owner_id.to_string(),
            SOURCE_LEASE_TTL_SECONDS,
        )
        .await?
        .is_some()
    {
        return Ok(());
    }
    anyhow::bail!("source refresh lost lease before publish")
}

pub(super) async fn complete_generation(
    ledger: &dyn LedgerStore,
    generation: SourceGeneration,
    diff: &SourceManifestDiff,
    discovered: u64,
    vectorized: &VectorizeResult,
) -> anyhow::Result<SourceGeneration> {
    Ok(ledger
        .complete_generation(SourceGeneration {
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
                discovered,
                prepared: vectorized.documents_prepared,
                embedded: if vectorized.points_written > 0 {
                    vectorized.documents_prepared
                } else {
                    0
                },
                published: vectorized.documents_prepared,
                failed: 0,
            },
            ..generation
        })
        .await?)
}

pub(super) async fn publish(
    ledger: &dyn LedgerStore,
    vector_store: &dyn VectorStore,
    collection: &CollectionSpec,
    generation: &SourceGeneration,
    diff: &SourceManifestDiff,
    embed: bool,
) -> anyhow::Result<SourceGeneration> {
    if embed {
        vector_store
            .mark_generation_committed(
                collection.collection.clone(),
                generation.source_id.clone(),
                generation.generation.clone(),
            )
            .await?;
        if let Some(previous) = generation.previous_generation.clone()
            && !diff.unchanged.is_empty()
        {
            vector_store
                .mark_unchanged_items_committed(
                    collection.collection.clone(),
                    generation.source_id.clone(),
                    previous,
                    generation.generation.clone(),
                    diff.unchanged
                        .iter()
                        .map(|item| item.source_item_key.clone())
                        .collect(),
                )
                .await?;
        }
    }
    Ok(ledger
        .publish_generation(PublishGenerationRequest {
            source_id: generation.source_id.clone(),
            generation: generation.generation.clone(),
            expected_previous_generation: generation.previous_generation.clone(),
        })
        .await?)
}

pub(super) fn published_status(status: &DocumentStatus) -> DocumentStatus {
    DocumentStatus {
        status: DocumentLifecycleStatus::Published,
        updated_at: timestamp(),
        ..status.clone()
    }
}
