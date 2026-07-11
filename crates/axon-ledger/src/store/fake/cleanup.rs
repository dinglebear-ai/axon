use std::sync::Arc;

use axon_api::source::*;
use tokio::sync::Mutex;

use super::FakeLedgerState;
use crate::store::Result;
use crate::store::util::{keyed_manifest_items, manifest_item_changed, timestamp};

/// Pending (unresolved) cleanup debt for a source, oldest first. A debt is
/// pending while `completed_at` is unset. Mirrors the SQLite ordering.
pub(in crate::store) async fn list_pending_cleanup_debt(
    state: &Arc<Mutex<FakeLedgerState>>,
    source_id: &SourceId,
) -> Result<Vec<CleanupDebt>> {
    let state = state.lock().await;
    let mut pending: Vec<CleanupDebt> = state
        .cleanup_debt
        .values()
        .filter(|debt| &debt.source_id == source_id && debt.completed_at.is_none())
        .cloned()
        .collect();
    pending.sort_by(|a, b| {
        a.created_at
            .0
            .cmp(&b.created_at.0)
            .then_with(|| a.debt_id.0.cmp(&b.debt_id.0))
    });
    Ok(pending)
}

/// Mark a debt resolved (`Completed` + `completed_at`). Idempotent: unknown or
/// already-resolved ids are a no-op.
pub(in crate::store) async fn resolve_cleanup_debt(
    state: &Arc<Mutex<FakeLedgerState>>,
    debt_id: &CleanupDebtId,
) -> Result<()> {
    let mut state = state.lock().await;
    if let Some(debt) = state.cleanup_debt.get_mut(debt_id)
        && debt.completed_at.is_none()
    {
        debt.status = LifecycleStatus::Completed;
        debt.completed_at = Some(timestamp());
    }
    Ok(())
}

/// Delete ledger rows for one superseded generation: the generation row
/// itself, its manifest, and any document-status rows recorded against it.
/// Idempotent — an unknown `(source_id, generation)` pair deletes nothing and
/// returns `0`. Callers (the prune drain) are responsible for never passing
/// the currently committed generation; this is the ledger-side `LedgerPrune`
/// boundary from `docs/pipeline-unification/runtime/ledger-contract.md`.
pub(in crate::store) async fn delete_generation(
    state: &Arc<Mutex<FakeLedgerState>>,
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> Result<u64> {
    let mut state = state.lock().await;
    let mut deleted = 0u64;
    let key = (source_id.clone(), generation.clone());
    if state.generations.remove(&key).is_some() {
        deleted += 1;
    }
    if state.manifests.remove(&key).is_some() {
        deleted += 1;
    }
    let stale_documents: Vec<DocumentId> = state
        .document_statuses
        .values()
        .filter(|status| {
            &status.source_id == source_id && status.generation.as_ref() == Some(generation)
        })
        .map(|status| status.document_id.clone())
        .collect();
    for document_id in stale_documents {
        if state.document_statuses.remove(&document_id).is_some() {
            deleted += 1;
        }
    }
    Ok(deleted)
}

pub(super) fn record_removed_item_cleanup_debt(
    state: &mut FakeLedgerState,
    generation: &SourceGeneration,
) {
    let Some(previous_generation) = generation.previous_generation.as_ref() else {
        return;
    };
    let previous_items = state
        .manifests
        .get(&(generation.source_id.clone(), previous_generation.clone()))
        .map(|manifest| manifest.items.clone())
        .unwrap_or_default();
    let next_by_key = state
        .manifests
        .get(&(generation.source_id.clone(), generation.generation.clone()))
        .map(|manifest| keyed_manifest_items(manifest.items.clone()))
        .unwrap_or_default();
    for item in previous_items {
        if let Some(next) = next_by_key.get(&item.source_item_key)
            && !manifest_item_changed(&item, next)
        {
            continue;
        }
        let debt = CleanupDebt {
            debt_id: CleanupDebtId::new(format!(
                "debt_{}",
                uuid::Uuid::new_v5(
                    &uuid::Uuid::NAMESPACE_URL,
                    format!(
                        "{}:{}:{}",
                        generation.source_id.0, previous_generation.0, item.source_item_key.0
                    )
                    .as_bytes(),
                )
            )),
            job_id: JobId::new(uuid::Uuid::from_u128(0)),
            source_id: generation.source_id.clone(),
            generation: Some(previous_generation.clone()),
            kind: CleanupDebtKind::VectorDelete,
            selector: CleanupSelector::SourceItem {
                source_id: generation.source_id.clone(),
                source_item_key: item.source_item_key,
                generation: previous_generation.clone(),
            },
            status: LifecycleStatus::Pending,
            created_at: timestamp(),
            attempts: 0,
            last_error: None,
            next_retry_at: None,
            completed_at: None,
        };
        state
            .cleanup_debt
            .entry(debt.debt_id.clone())
            .or_insert(debt);
    }
}
