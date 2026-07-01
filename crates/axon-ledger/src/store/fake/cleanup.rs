use axon_api::source::*;

use super::FakeLedgerState;
use crate::store::util::{keyed_manifest_items, manifest_item_changed, timestamp};

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
