use std::collections::BTreeSet;
use std::sync::Arc;

use axon_api::source::*;
use tokio::sync::Mutex;

use super::FakeLedgerState;
use crate::store::Result;
use crate::store::util::{keyed_manifest_items, manifest_item_changed, timestamp};

const ARTIFACT_METADATA_KEY: &str = "_axon_artifacts";
const CACHE_KEY_METADATA_KEY: &str = "_axon_document_cache_key";

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

/// Record (idempotently) stale-item debt for every item that was removed or
/// modified between the previous committed generation and `generation`.
///
/// This includes the vector delete plus any artifact/cache cleanup identities
/// recorded in the previous manifest metadata. Returns the full set of debt
/// rows relevant to this publish — freshly inserted or already present at the
/// same natural key — so the caller can fold it into
/// `SourceGeneration.cleanup_debt` without re-scanning the whole store.
pub(super) fn record_removed_item_cleanup_debt(
    state: &mut FakeLedgerState,
    generation: &SourceGeneration,
) -> Vec<CleanupDebt> {
    let Some(previous_generation) = generation.previous_generation.as_ref() else {
        return Vec::new();
    };
    let previous_manifest = state
        .manifests
        .get(&(generation.source_id.clone(), previous_generation.clone()))
        .cloned();
    let previous_items = previous_manifest
        .as_ref()
        .map(|manifest| manifest.items.clone())
        .unwrap_or_default();
    let next_by_key = state
        .manifests
        .get(&(generation.source_id.clone(), generation.generation.clone()))
        .map(|manifest| keyed_manifest_items(manifest.items.clone()))
        .unwrap_or_default();
    let mut debts = Vec::new();
    let mut artifact_ids = BTreeSet::new();
    if let Some(previous_manifest) = previous_manifest.as_ref() {
        for debt in artifact_cleanup_debt_for_metadata(
            &generation.source_id,
            previous_generation,
            &previous_manifest.metadata,
            "manifest",
            &mut artifact_ids,
        ) {
            insert_or_get_debt(state, &mut debts, debt);
        }
    }
    for item in previous_items {
        if let Some(next) = next_by_key.get(&item.source_item_key)
            && !manifest_item_changed(&item, next)
        {
            continue;
        }
        insert_or_get_debt(
            state,
            &mut debts,
            vector_cleanup_debt(
                &generation.source_id,
                previous_generation,
                &item.source_item_key,
            ),
        );
        for debt in artifact_cleanup_debt_for_metadata(
            &generation.source_id,
            previous_generation,
            &item.metadata,
            &item.source_item_key.0,
            &mut artifact_ids,
        ) {
            insert_or_get_debt(state, &mut debts, debt);
        }
        if let Some(debt) = cache_cleanup_debt_for_metadata(
            &generation.source_id,
            previous_generation,
            &item.metadata,
            &item.source_item_key,
        ) {
            insert_or_get_debt(state, &mut debts, debt);
        }
    }
    debts
}

fn insert_or_get_debt(
    state: &mut FakeLedgerState,
    debts: &mut Vec<CleanupDebt>,
    debt: CleanupDebt,
) {
    let stored = state
        .cleanup_debt
        .entry(debt.debt_id.clone())
        .or_insert(debt);
    debts.push(stored.clone());
}

fn vector_cleanup_debt(
    source_id: &SourceId,
    previous_generation: &SourceGenerationId,
    source_item_key: &SourceItemKey,
) -> CleanupDebt {
    CleanupDebt {
        debt_id: CleanupDebtId::new(format!(
            "debt_{}",
            uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_URL,
                format!(
                    "{}:{}:{}",
                    source_id.0, previous_generation.0, source_item_key.0
                )
                .as_bytes(),
            )
        )),
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        source_id: source_id.clone(),
        generation: Some(previous_generation.clone()),
        kind: CleanupDebtKind::VectorDelete,
        selector: CleanupSelector::SourceItem {
            source_id: source_id.clone(),
            source_item_key: source_item_key.clone(),
            generation: previous_generation.clone(),
        },
        status: LifecycleStatus::Pending,
        created_at: timestamp(),
        attempts: 0,
        last_error: None,
        next_retry_at: None,
        completed_at: None,
    }
}

fn artifact_cleanup_debt_for_metadata(
    source_id: &SourceId,
    previous_generation: &SourceGenerationId,
    metadata: &MetadataMap,
    owner_key: &str,
    seen: &mut BTreeSet<ArtifactId>,
) -> Vec<CleanupDebt> {
    artifact_refs_from_metadata(metadata)
        .into_iter()
        .filter(|artifact| seen.insert(artifact.artifact_id.clone()))
        .map(|artifact| CleanupDebt {
            debt_id: cleanup_debt_id(
                "artifact",
                source_id,
                previous_generation,
                &format!("{owner_key}:{}", artifact.artifact_id.0),
            ),
            job_id: JobId::new(uuid::Uuid::from_u128(0)),
            source_id: source_id.clone(),
            generation: Some(previous_generation.clone()),
            kind: CleanupDebtKind::ArtifactDelete,
            selector: CleanupSelector::Artifact {
                artifact_id: artifact.artifact_id,
            },
            status: LifecycleStatus::Pending,
            created_at: timestamp(),
            attempts: 0,
            last_error: None,
            next_retry_at: None,
            completed_at: None,
        })
        .collect()
}

fn cache_cleanup_debt_for_metadata(
    source_id: &SourceId,
    previous_generation: &SourceGenerationId,
    metadata: &MetadataMap,
    source_item_key: &SourceItemKey,
) -> Option<CleanupDebt> {
    let value = metadata.get(CACHE_KEY_METADATA_KEY)?;
    let key: DocumentCacheKey = serde_json::from_value(value.clone()).ok()?;
    let key_json = serde_json::to_string(&key).ok()?;
    Some(CleanupDebt {
        debt_id: cleanup_debt_id("cache", source_id, previous_generation, &source_item_key.0),
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        source_id: source_id.clone(),
        generation: Some(previous_generation.clone()),
        kind: CleanupDebtKind::CachePrune,
        selector: CleanupSelector::CacheKeys {
            keys: vec![key_json],
        },
        status: LifecycleStatus::Pending,
        created_at: timestamp(),
        attempts: 0,
        last_error: None,
        next_retry_at: None,
        completed_at: None,
    })
}

fn artifact_refs_from_metadata(metadata: &MetadataMap) -> Vec<ArtifactRef> {
    metadata
        .get(ARTIFACT_METADATA_KEY)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

fn cleanup_debt_id(
    prefix: &str,
    source_id: &SourceId,
    generation: &SourceGenerationId,
    identity: &str,
) -> CleanupDebtId {
    CleanupDebtId::new(format!(
        "debt_{}",
        uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_URL,
            format!("{prefix}:{}:{}:{identity}", source_id.0, generation.0).as_bytes(),
        )
    ))
}

/// Record (idempotently) `GraphPrune` debt for every item genuinely absent
/// from the new generation's manifest (not merely modified — a modified item
/// keeps its stable key, so its graph node stays). See the module doc in
/// `crate::sqlite::generation::graph_prune` (the SQLite-side twin of this
/// producer) for why only true removals qualify.
pub(super) fn record_graph_prune_cleanup_debt(
    state: &mut FakeLedgerState,
    generation: &SourceGeneration,
) -> Vec<CleanupDebt> {
    let Some(previous_generation) = generation.previous_generation.as_ref() else {
        return Vec::new();
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
    let mut debts = Vec::new();
    for item in previous_items {
        if next_by_key.contains_key(&item.source_item_key) {
            continue; // still present (unchanged or modified) — graph node stays
        }
        let debt = CleanupDebt {
            debt_id: CleanupDebtId::new(format!(
                "debt_{}",
                uuid::Uuid::new_v5(
                    &uuid::Uuid::NAMESPACE_URL,
                    format!(
                        "graph:{}:{}:{}",
                        generation.source_id.0, previous_generation.0, item.source_item_key.0
                    )
                    .as_bytes(),
                )
            )),
            job_id: JobId::new(uuid::Uuid::from_u128(0)),
            source_id: generation.source_id.clone(),
            generation: Some(previous_generation.clone()),
            kind: CleanupDebtKind::GraphPrune,
            selector: CleanupSelector::GraphNodes {
                stable_keys: vec![item.source_item_key.0.clone()],
            },
            status: LifecycleStatus::Pending,
            created_at: timestamp(),
            attempts: 0,
            last_error: None,
            next_retry_at: None,
            completed_at: None,
        };
        let stored = state
            .cleanup_debt
            .entry(debt.debt_id.clone())
            .or_insert(debt);
        debts.push(stored.clone());
    }
    debts
}

/// Record (idempotently) `LedgerPrune` debt for generations that have aged
/// past the retention window (`LEDGER_GENERATION_RETENTION_COMMITTED`,
/// currently 2 — the just-published generation plus its immediate
/// predecessor are always retained). An older generation is skipped for one
/// more publish cycle while it still has other unresolved (non-`LedgerPrune`)
/// cleanup debt referencing it. See the SQLite-side twin
/// `crate::sqlite::generation::ledger_prune` for the fuller contract note.
pub(super) fn record_ledger_prune_cleanup_debt(
    state: &mut FakeLedgerState,
    generation: &SourceGeneration,
) -> Vec<CleanupDebt> {
    let Some(previous_generation) = generation.previous_generation.as_ref() else {
        return Vec::new();
    };

    let mut cursor = Some(previous_generation.clone());
    for _ in 0..crate::LEDGER_GENERATION_RETENTION_COMMITTED.saturating_sub(1) {
        let Some(current) = cursor else {
            return Vec::new();
        };
        cursor = state
            .generations
            .get(&(generation.source_id.clone(), current))
            .and_then(|candidate| candidate.previous_generation.clone());
    }

    let mut debts = Vec::new();
    while let Some(candidate) = cursor {
        let Some(candidate_generation) = state
            .generations
            .get(&(generation.source_id.clone(), candidate.clone()))
            .cloned()
        else {
            break; // already pruned — nothing older left to consider
        };
        let has_unresolved_non_ledger_debt = state.cleanup_debt.values().any(|debt| {
            debt.source_id == generation.source_id
                && debt.generation.as_ref() == Some(&candidate)
                && debt.completed_at.is_none()
                && debt.kind != CleanupDebtKind::LedgerPrune
        });
        if !has_unresolved_non_ledger_debt {
            let debt = CleanupDebt {
                debt_id: CleanupDebtId::new(format!(
                    "debt_{}",
                    uuid::Uuid::new_v5(
                        &uuid::Uuid::NAMESPACE_URL,
                        format!("ledger:{}:{}", generation.source_id.0, candidate.0).as_bytes(),
                    )
                )),
                job_id: JobId::new(uuid::Uuid::from_u128(0)),
                source_id: generation.source_id.clone(),
                generation: Some(candidate.clone()),
                kind: CleanupDebtKind::LedgerPrune,
                selector: CleanupSelector::LedgerGenerations {
                    source_id: generation.source_id.clone(),
                    up_to_generation: candidate.clone(),
                },
                status: LifecycleStatus::Pending,
                created_at: timestamp(),
                attempts: 0,
                last_error: None,
                next_retry_at: None,
                completed_at: None,
            };
            let stored = state
                .cleanup_debt
                .entry(debt.debt_id.clone())
                .or_insert(debt);
            debts.push(stored.clone());
        }
        cursor = candidate_generation.previous_generation;
    }
    debts
}
