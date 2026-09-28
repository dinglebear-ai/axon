//! Graph publication and retirement share the existing source writer lease.
use axon_api::source::*;
use axon_ledger::store::LedgerStore;
use std::{future::Future, sync::Arc};

pub(crate) async fn with_source_lease<T>(
    ledger: Arc<dyn LedgerStore>,
    source: SourceId,
    job: JobId,
    operation: impl Future<Output = Result<T, ApiError>>,
) -> Result<T, ApiError> {
    let owner = format!("graph:{}", uuid::Uuid::new_v4());
    let lease = ledger
        .acquire_lease(LeaseRequest {
            lease_key: format!("source:{}", source.0),
            owner_id: owner.clone(),
            ttl_seconds: 1800,
            job_id: Some(job),
            metadata: MetadataMap::new(),
        })
        .await?
        .ok_or_else(|| {
            ApiError::new(
                "graph.source_busy",
                ErrorStage::Graphing,
                "source publication is busy",
            )
        })?;
    let cancellation = tokio_util::sync::CancellationToken::new();
    let result = super::super::executor::lease_heartbeat::maintain(ledger.clone(), &lease, 1800, Some(cancellation.clone()), async {
        tokio::select! {
            result = operation => result,
            _ = cancellation.cancelled() => Err(ApiError::new("graph.source_lease_lost", ErrorStage::Graphing, "source publication lease was lost")),
        }
    }).await;
    let release = ledger.release_lease(lease.lease_id, owner).await;
    match (result, release) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) | (_, Err(error)) => Err(error),
    }
}

fn eligible_status(status: DocumentLifecycleStatus) -> bool {
    matches!(
        status,
        DocumentLifecycleStatus::Prepared
            | DocumentLifecycleStatus::Embedded
            | DocumentLifecycleStatus::Vectorized
            | DocumentLifecycleStatus::Published
            | DocumentLifecycleStatus::Skipped
    )
}

pub(super) async fn filter_skipped(
    ledger: &dyn LedgerStore,
    manifest: &mut SourceManifest,
) -> Result<(), ApiError> {
    let statuses = ledger
        .document_statuses_for_items(
            manifest.source_id.clone(),
            manifest
                .items
                .iter()
                .map(|i| i.source_item_key.clone())
                .collect(),
        )
        .await?;
    let mut by_item = std::collections::BTreeMap::<_, Vec<_>>::new();
    for status in statuses {
        by_item
            .entry(status.source_item_key.clone())
            .or_default()
            .push(status);
    }
    for item in &manifest.items {
        let Some(statuses) = by_item.get(&item.source_item_key) else {
            return Err(ApiError::new(
                "graph.publication_status_unknown",
                ErrorStage::Graphing,
                "indexed item status is unavailable",
            ));
        };
        if statuses
            .iter()
            .any(|status| !eligible_status(status.status))
        {
            return Err(ApiError::new(
                "graph.publication_status_unknown",
                ErrorStage::Graphing,
                "indexed item disposition is unavailable",
            ));
        }
        if statuses
            .iter()
            .any(|status| status.generation.as_ref() != Some(&manifest.generation))
        {
            return Err(ApiError::new(
                "graph.publication_status_stale",
                ErrorStage::Graphing,
                "indexed item status does not match committed generation",
            ));
        }
    }
    manifest.items.retain(|item| {
        by_item[&item.source_item_key]
            .iter()
            .any(|status| status.status != DocumentLifecycleStatus::Skipped)
    });
    Ok(())
}

pub(crate) async fn retire_under_lease<Fut>(
    ledger: Arc<dyn LedgerStore>,
    source: SourceId,
    item: SourceItemKey,
    _retirement_generation: SourceGenerationId,
    job: JobId,
    retire: impl FnOnce(SourceId, SourceItemKey) -> Fut,
) -> Result<GraphDeleteResult, ApiError>
where
    Fut: Future<Output = Result<GraphDeleteResult, ApiError>>,
{
    with_source_lease(ledger.clone(), source.clone(), job, async {
        let generation = ledger
            .committed_generation(source.clone())
            .await?
            .ok_or_else(|| {
                ApiError::new(
                    "graph.retirement_generation_missing",
                    ErrorStage::Cleaning,
                    "committed generation is unavailable",
                )
            })?;
        let manifest = ledger
            .get_manifest(source.clone(), generation.clone())
            .await?
            .ok_or_else(|| {
                ApiError::new(
                    "graph.retirement_manifest_missing",
                    ErrorStage::Cleaning,
                    "committed manifest is unavailable",
                )
            })?;
        if manifest
            .items
            .iter()
            .any(|entry| entry.source_item_key == item)
        {
            let statuses = ledger
                .document_statuses_for_items(source.clone(), vec![item.clone()])
                .await?;
            if statuses.is_empty()
                || statuses.iter().any(|status| {
                    status.generation.as_ref() != Some(&generation)
                        || !eligible_status(status.status)
                })
            {
                return Err(ApiError::new(
                    "graph.retirement_status_unknown",
                    ErrorStage::Cleaning,
                    "current item disposition is unavailable",
                ));
            }
            if statuses
                .iter()
                .any(|status| status.status != DocumentLifecycleStatus::Skipped)
            {
                return Ok(GraphDeleteResult::default());
            }
        }
        retire(source, item).await
    })
    .await
}
