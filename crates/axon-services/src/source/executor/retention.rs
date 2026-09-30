//! Committed inventory and document-status provenance for refreshes.
use axon_api::source::*;
use axon_ledger::store::LedgerStore;
use std::collections::{BTreeMap, BTreeSet};

#[path = "retention/processing_identity.rs"]
mod identity;
pub(super) use identity::processing_identity;

pub(super) async fn merge_inventory(
    ledger: &dyn LedgerStore,
    plan: &SourcePlan,
    manifest: &mut SourceManifest,
) -> anyhow::Result<BTreeSet<SourceItemKey>> {
    let unvisited = BTreeSet::new();
    if manifest.inventory_completeness() == InventoryCompleteness::Complete {
        return Ok(unvisited);
    }
    let Some(previous) = ledger
        .committed_generation(manifest.source_id.clone())
        .await?
    else {
        return Ok(unvisited);
    };
    let prior = ledger
        .get_manifest(manifest.source_id.clone(), previous)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("committed source inventory is missing; complete refresh required")
        })?;
    let git_repo = plan.route.source.source_kind == SourceKind::Git
        && matches!(plan.route.scope, SourceScope::Repo | SourceScope::Directory);
    let existing_paths = if git_repo {
        let wanted = prior
            .items
            .iter()
            .filter_map(|item| item.display_path.clone())
            .collect();
        Some(axon_adapters::git::retained_repository_paths(plan.clone(), wanted).await?)
    } else {
        None
    };
    retain_unvisited(plan, manifest, prior, existing_paths.as_ref())
}

fn retain_unvisited(
    plan: &SourcePlan,
    manifest: &mut SourceManifest,
    prior: SourceManifest,
    existing_paths: Option<&BTreeSet<String>>,
) -> anyhow::Result<BTreeSet<SourceItemKey>> {
    let mut unvisited = BTreeSet::new();
    let visited: BTreeSet<_> = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.clone())
        .collect();
    let git_repo = plan.route.source.source_kind == SourceKind::Git
        && matches!(plan.route.scope, SourceScope::Repo | SourceScope::Directory);
    for item in prior.items {
        let allowed = !git_repo
            || item
                .display_path
                .as_deref()
                .is_some_and(|path| existing_paths.is_some_and(|paths| paths.contains(path)));
        if allowed && !visited.contains(&item.source_item_key) {
            unvisited.insert(item.source_item_key.clone());
            manifest.items.push(item);
        }
    }
    Ok(unvisited)
}

pub(super) fn eligible(status: &DocumentStatus, generation: &SourceGenerationId) -> bool {
    status.generation.as_ref() == Some(generation)
        && matches!(
            status.status,
            DocumentLifecycleStatus::Prepared
                | DocumentLifecycleStatus::Embedded
                | DocumentLifecycleStatus::Vectorized
                | DocumentLifecycleStatus::Published
                | DocumentLifecycleStatus::Skipped
        )
}

async fn grouped_statuses(
    ledger: &dyn LedgerStore,
    diff: &SourceManifestDiff,
) -> anyhow::Result<BTreeMap<SourceItemKey, Vec<DocumentStatus>>> {
    let keys = diff
        .unchanged
        .iter()
        .map(|item| item.source_item_key.clone())
        .collect();
    let mut grouped = BTreeMap::<_, Vec<_>>::new();
    for status in ledger
        .document_statuses_for_items(diff.source_id.clone(), keys)
        .await?
    {
        grouped
            .entry(status.source_item_key.clone())
            .or_default()
            .push(status);
    }
    Ok(grouped)
}

/// Latest status rows can belong to a failed attempt. Never infer retained output
/// from their counts, or from an aggregate source summary.
pub(super) async fn validate_retained(
    ledger: &dyn LedgerStore,
    diff: &mut SourceManifestDiff,
    unvisited: &BTreeSet<SourceItemKey>,
) -> anyhow::Result<()> {
    let Some(previous) = diff.previous_generation.as_ref() else {
        return Ok(());
    };
    let grouped = grouped_statuses(ledger, diff).await?;
    let mut retained = Vec::new();
    for item in std::mem::take(&mut diff.unchanged) {
        let valid = grouped
            .get(&item.source_item_key)
            .is_some_and(|rows| !rows.is_empty() && rows.iter().all(|row| eligible(row, previous)));
        if valid {
            retained.push(item);
        } else {
            anyhow::ensure!(
                !unvisited.contains(&item.source_item_key),
                "partial refresh cannot verify retained output; complete refresh required"
            );
            diff.modified.push(item);
        }
    }
    diff.unchanged = retained;
    diff.counts.unchanged = diff.unchanged.len() as u64;
    diff.counts.modified = diff.modified.len() as u64;
    Ok(())
}

pub(super) async fn retained_statuses(
    ledger: &dyn LedgerStore,
    diff: &SourceManifestDiff,
) -> anyhow::Result<Vec<DocumentStatus>> {
    let Some(previous) = diff.previous_generation.as_ref() else {
        return Ok(Vec::new());
    };
    let mut grouped = grouped_statuses(ledger, diff).await?;
    let mut statuses = Vec::new();
    for item in &diff.unchanged {
        let rows = grouped.remove(&item.source_item_key).unwrap_or_default();
        anyhow::ensure!(
            !rows.is_empty() && rows.iter().all(|row| eligible(row, previous)),
            "retained document provenance changed before publication; retry refresh"
        );
        statuses.extend(rows);
    }
    Ok(statuses)
}

pub(super) async fn carry_and_count(
    ledger: &dyn LedgerStore,
    manifest: &SourceManifest,
    diff: &SourceManifestDiff,
    vectorized: &super::vectorize::VectorizeResult,
) -> anyhow::Result<(SourceCounts, Vec<DocumentStatus>)> {
    let retained = retained_statuses(ledger, diff).await?;
    let counts = source_counts(manifest, diff, &retained, vectorized);
    Ok((counts, retained))
}

pub(super) fn source_counts(
    manifest: &SourceManifest,
    diff: &SourceManifestDiff,
    retained: &[DocumentStatus],
    vectorized: &super::vectorize::VectorizeResult,
) -> SourceCounts {
    let mut counts = SourceCounts {
        items_total: manifest.items.len() as u64,
        items_changed: diff.counts.added + diff.counts.modified + diff.counts.removed,
        documents_total: vectorized.documents_prepared,
        documents_skipped: vectorized.documents_skipped,
        chunks_total: vectorized.chunks_prepared,
        vector_points_total: vectorized.points_written,
        bytes_total: manifest
            .items
            .iter()
            .map(|item| item.size_bytes.unwrap_or(0))
            .fold(0u64, u64::saturating_add),
    };
    for status in retained {
        if status.status == DocumentLifecycleStatus::Skipped {
            counts.documents_skipped = counts.documents_skipped.saturating_add(1);
        } else {
            counts.documents_total = counts.documents_total.saturating_add(1);
        }
        counts.chunks_total = counts
            .chunks_total
            .saturating_add(u64::from(status.chunk_count));
        counts.vector_points_total = counts
            .vector_points_total
            .saturating_add(u64::from(status.vector_point_count));
    }
    counts
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod tests;
