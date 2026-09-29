//! Fake parity for bulk status provenance and conditional carry.
use super::*;
use std::collections::BTreeSet;
fn conflict() -> ApiError {
    ApiError::new(
        "source.ledger.status_provenance_changed",
        ErrorStage::Publishing,
        "document status provenance changed; refresh must reprepare retained items",
    )
}
pub(in crate::store::fake) async fn document_statuses_for_items(
    state: &Arc<Mutex<FakeLedgerState>>,
    source_id: SourceId,
    keys: Vec<SourceItemKey>,
) -> Result<Vec<DocumentStatus>> {
    let state = state.lock().await;
    let keys = keys.into_iter().collect::<BTreeSet<_>>();
    Ok(state
        .document_statuses
        .values()
        .filter(|s| s.source_id == source_id && keys.contains(&s.source_item_key))
        .cloned()
        .collect())
}
pub(in crate::store::fake) async fn carry_document_statuses(
    state: &Arc<Mutex<FakeLedgerState>>,
    source_id: SourceId,
    expected_generation: SourceGenerationId,
    next_generation: SourceGenerationId,
    expected: Vec<DocumentStatus>,
    updated_at: Timestamp,
) -> Result<u64> {
    if expected.is_empty() {
        return Ok(0);
    }
    let mut state = state.lock().await;
    carry_document_statuses_locked(
        &mut state,
        &source_id,
        &expected_generation,
        &next_generation,
        expected,
        &updated_at,
    )
}

pub(in crate::store::fake) fn carry_document_statuses_locked(
    state: &mut FakeLedgerState,
    source_id: &SourceId,
    expected_generation: &SourceGenerationId,
    next_generation: &SourceGenerationId,
    mut expected: Vec<DocumentStatus>,
    updated_at: &Timestamp,
) -> Result<u64> {
    if expected.is_empty() {
        return Ok(0);
    }
    if state.committed.get(&source_id) != Some(&expected_generation) {
        return Err(conflict());
    }
    let target = state
        .generations
        .get(&(source_id.clone(), next_generation.clone()))
        .ok_or_else(conflict)?;
    if target.previous_generation.as_ref() != Some(&expected_generation)
        || next_generation == expected_generation
    {
        return Err(conflict());
    }
    expected.sort_by(|a, b| a.document_id.cmp(&b.document_id));
    let keys = expected
        .iter()
        .map(|s| s.source_item_key.clone())
        .collect::<BTreeSet<_>>();
    let actual = state
        .document_statuses
        .values()
        .filter(|s| &s.source_id == source_id && keys.contains(&s.source_item_key))
        .cloned()
        .collect::<Vec<_>>();
    if actual != expected
        || expected.iter().any(|s| {
            !matches!(
                s.status,
                DocumentLifecycleStatus::Prepared
                    | DocumentLifecycleStatus::Embedded
                    | DocumentLifecycleStatus::Vectorized
                    | DocumentLifecycleStatus::Published
                    | DocumentLifecycleStatus::Skipped
            ) || &s.source_id != source_id
                || s.generation.as_ref() != Some(&expected_generation)
                || s.updated_at.0 > updated_at.0
        })
        || expected
            .windows(2)
            .any(|w| w[0].document_id == w[1].document_id)
    {
        return Err(conflict());
    }
    let manifest = state
        .manifests
        .get(&(source_id.clone(), next_generation.clone()))
        .ok_or_else(conflict)?;
    let target_keys = manifest
        .items
        .iter()
        .map(|i| &i.source_item_key)
        .collect::<BTreeSet<_>>();
    if keys.iter().any(|key| !target_keys.contains(key)) {
        return Err(conflict());
    }
    let count = expected.len() as u64;
    for mut status in expected {
        status.generation = Some(next_generation.clone());
        status.updated_at = updated_at.clone();
        state
            .document_statuses
            .insert(status.document_id.clone(), status);
    }
    Ok(count)
}
