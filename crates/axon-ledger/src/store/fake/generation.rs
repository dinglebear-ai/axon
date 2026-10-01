//! Generation lifecycle (create/complete/fail/publish) for [`FakeLedgerStore`].
//!
//! Split out of `fake.rs` to keep it under the monolith file-size cap — pure
//! state-machine logic over the same `FakeLedgerState`, no new concepts.

use std::sync::Arc;

use axon_api::source::*;
use tokio::sync::Mutex;

use super::{
    FakeLedgerMode, FakeLedgerState, record_graph_prune_cleanup_debt,
    record_ledger_prune_cleanup_debt, record_removed_item_cleanup_debt,
};
use crate::store::Result;
use crate::store::util::{generation_missing_error, source_missing_error, timestamp};
use crate::validation::{
    ensure_generation_publishable, ensure_generation_writable, generation_already_published_error,
    manifest_missing_error,
};

pub(super) async fn create_generation(
    state: &Arc<Mutex<FakeLedgerState>>,
    source_id: SourceId,
) -> Result<SourceGeneration> {
    let mut state = state.lock().await;
    if !state.sources.contains_key(&source_id) {
        return Err(source_missing_error(&source_id));
    }
    let mut sequence = state
        .generation_counters
        .get(&source_id)
        .copied()
        .unwrap_or(0)
        + 1;
    while state.generations.contains_key(&(
        source_id.clone(),
        SourceGenerationId::new(format!("gen_{sequence}")),
    )) {
        sequence += 1;
    }
    state
        .generation_counters
        .insert(source_id.clone(), sequence);
    let generation = SourceGenerationId::new(format!("gen_{sequence}"));
    let generation = SourceGeneration {
        source_id: source_id.clone(),
        generation: generation.clone(),
        status: LifecycleStatus::Running,
        publish_state: PublishState::Writing,
        created_at: timestamp(),
        published_at: None,
        item_counts: ItemCounts {
            added: 0,
            modified: 0,
            removed: 0,
            unchanged: 0,
            failed: 0,
        },
        document_counts: DocumentCounts {
            skipped: 0,
            discovered: 0,
            prepared: 0,
            embedded: 0,
            published: 0,
            failed: 0,
        },
        cleanup_debt: Vec::new(),
        previous_generation: state.committed.get(&source_id).cloned(),
    };
    state.generations.insert(
        (source_id, generation.generation.clone()),
        generation.clone(),
    );
    Ok(generation)
}

pub(super) async fn committed_generation(
    state: &Arc<Mutex<FakeLedgerState>>,
    source_id: SourceId,
) -> Result<Option<SourceGenerationId>> {
    Ok(state.lock().await.committed.get(&source_id).cloned())
}

pub(super) async fn complete_generation(
    state: &Arc<Mutex<FakeLedgerState>>,
    generation: SourceGeneration,
) -> Result<SourceGeneration> {
    ensure_generation_publishable(&generation)?;
    let mut state = state.lock().await;
    if !state.sources.contains_key(&generation.source_id) {
        return Err(source_missing_error(&generation.source_id));
    }
    let key = (generation.source_id.clone(), generation.generation.clone());
    let Some(stored) = state.generations.get(&key).cloned() else {
        return Err(generation_missing_error(
            &generation.source_id,
            &generation.generation,
        ));
    };
    ensure_generation_writable(&stored)?;
    if !state
        .manifests
        .contains_key(&(generation.source_id.clone(), generation.generation.clone()))
    {
        return Err(manifest_missing_error(&generation));
    }
    if stored.previous_generation != generation.previous_generation {
        return Err(ApiError::new(
            "source.ledger.generation_baseline_changed",
            ErrorStage::Publishing,
            format!(
                "generation {} was based on {:?}, but stored generation is based on {:?}",
                generation.generation.0, generation.previous_generation, stored.previous_generation
            ),
        )
        .with_source_id(generation.source_id.0));
    }

    let mut completed = generation;
    completed.publish_state = PublishState::Writing;
    completed.published_at = None;
    completed.cleanup_debt = Vec::new();
    completed.created_at = stored.created_at;
    state.generations.insert(key, completed.clone());
    Ok(completed)
}

pub(super) async fn fail_generation(
    state: &Arc<Mutex<FakeLedgerState>>,
    generation: SourceGeneration,
) -> Result<SourceGeneration> {
    let mut state = state.lock().await;
    let key = (generation.source_id.clone(), generation.generation.clone());
    let Some(stored) = state.generations.get(&key).cloned() else {
        return Err(generation_missing_error(
            &generation.source_id,
            &generation.generation,
        ));
    };
    if stored.published_at.is_some() || stored.publish_state != PublishState::Writing {
        return Err(generation_already_published_error(&stored));
    }
    let mut failed = generation;
    failed.created_at = stored.created_at;
    failed.published_at = None;
    failed.publish_state = PublishState::Writing;
    failed.status = LifecycleStatus::Failed;
    state.generations.insert(key, failed.clone());
    Ok(failed)
}

pub(super) async fn publish_generation(
    state: &Arc<Mutex<FakeLedgerState>>,
    mode: FakeLedgerMode,
    request: PublishGenerationRequest,
) -> Result<SourceGeneration> {
    if mode == FakeLedgerMode::PublishFailure {
        return Err(ApiError::new(
            "source.ledger.publish_failed",
            ErrorStage::Publishing,
            "fake ledger failed to publish generation",
        )
        .with_source_id(request.source_id.0));
    }
    let mut state = state.lock().await;
    let Some(generation) = state
        .generations
        .get(&(request.source_id.clone(), request.generation.clone()))
        .cloned()
    else {
        return Err(generation_missing_error(
            &request.source_id,
            &request.generation,
        ));
    };
    ensure_generation_publishable(&generation)?;
    if !state
        .manifests
        .contains_key(&(generation.source_id.clone(), generation.generation.clone()))
    {
        return Err(manifest_missing_error(&generation));
    }
    let committed = state.committed.get(&generation.source_id).cloned();
    if committed != request.expected_previous_generation
        || generation.previous_generation != request.expected_previous_generation
    {
        return Err(ApiError::new(
            "source.ledger.generation_baseline_changed",
            ErrorStage::Publishing,
            format!(
                "generation {} was based on {:?}, but committed generation is {:?}",
                generation.generation.0, generation.previous_generation, committed
            ),
        )
        .with_source_id(generation.source_id.0));
    }
    if let Some(previous) = committed.as_ref() {
        super::document::carry_document_statuses_locked(
            &mut state,
            &request.source_id,
            previous,
            &request.generation,
            request.retained_statuses.clone(),
            &timestamp(),
        )?;
    } else if !request.retained_statuses.is_empty() {
        return Err(ApiError::new(
            "source.ledger.status_provenance_changed",
            ErrorStage::Publishing,
            "initial publication cannot carry retained statuses",
        ));
    }
    let mut new_debt = record_removed_item_cleanup_debt(&mut state, &generation);
    new_debt.extend(record_graph_prune_cleanup_debt(&mut state, &generation));
    new_debt.extend(record_ledger_prune_cleanup_debt(&mut state, &generation));
    for debt in &mut new_debt {
        debt.job_id = request.job_id;
        debt.origin_attempt = request.attempt;
        if let Some(stored) = state.cleanup_debt.get_mut(&debt.debt_id) {
            stored.job_id = request.job_id;
            stored.origin_attempt = request.attempt;
        }
    }
    let cleanup_debt = new_debt
        .iter()
        .map(|debt| debt.debt_id.clone())
        .collect::<Vec<_>>();
    let mut published = generation.clone();
    published.publish_state = if cleanup_debt.is_empty() {
        PublishState::Committed
    } else {
        PublishState::CleanupPending
    };
    published.published_at = Some(timestamp());
    published.cleanup_debt = cleanup_debt;
    state.generations.insert(
        (published.source_id.clone(), published.generation.clone()),
        published.clone(),
    );
    state
        .committed
        .insert(published.source_id.clone(), published.generation.clone());
    Ok(published)
}

pub(super) async fn recover(
    state: &Arc<Mutex<FakeLedgerState>>,
    current: SourceGeneration,
    lease: LeaseGuard,
    collection: String,
) -> Result<u64> {
    let mut state = state.lock().await;
    let stored_lease = state.leases.get(&lease.lease_id).cloned();
    let valid = match stored_lease.as_ref() {
        Some(stored) => {
            stored.owner_id == lease.owner_id
                && stored.lease_key == format!("source:{}", current.source_id.0)
                && crate::store::util::timestamp_after(&stored.expires_at, &timestamp())?
        }
        None => false,
    };
    if !valid || collection.trim().is_empty() {
        return Err(ApiError::new(
            "source.ledger.recovery_lease_required",
            ErrorStage::Cleaning,
            "abandoned generation recovery requires this source's live lease and explicit vector collection; reacquire the source lease before retrying",
        ));
    }
    if !state
        .generations
        .get(&(current.source_id.clone(), current.generation.clone()))
        .is_some_and(|g| {
            g.status == LifecycleStatus::Running
                && g.published_at.is_none()
                && g.publish_state == PublishState::Writing
        })
    {
        return Err(ApiError::new(
            "source.ledger.recovery_generation_changed",
            ErrorStage::Cleaning,
            "active recovery generation is no longer running and unpublished; inspect its state before retrying",
        ));
    }
    let acquired = stored_lease.unwrap().acquired_at;
    let stored_created_at = state.generations
        [&(current.source_id.clone(), current.generation.clone())]
        .created_at
        .clone();
    let mut candidates = Vec::new();
    for g in state.generations.values() {
        if crate::store::util::timestamp_after(&stored_created_at, &g.created_at)?
            && crate::store::util::timestamp_after(&acquired, &g.created_at)?
            && (g.source_id == current.source_id
                && g.generation != current.generation
                && !state.cleanup_debt.values().any(|d| {
                    d.kind == CleanupDebtKind::VectorDelete
                        && d.source_id == g.source_id
                        && d.generation.as_ref() == Some(&g.generation)
                })
                && g.published_at.is_none()
                && g.publish_state == PublishState::Writing
                && matches!(
                    g.status,
                    LifecycleStatus::Running | LifecycleStatus::Completed | LifecycleStatus::Failed
                ))
            && state
                .manifests
                .get(&(g.source_id.clone(), g.generation.clone()))
                .and_then(|m| {
                    m.metadata
                        .get(crate::GENERATION_VECTOR_COLLECTION_METADATA_KEY)
                })
                .and_then(|v| v.as_str())
                .is_some_and(|v| !v.trim().is_empty())
        {
            candidates.push(g.clone());
        }
    }
    candidates.sort_by(|a, b| a.created_at.0.cmp(&b.created_at.0));
    candidates.truncate(64);
    let mut recovered = 0;
    for mut old in candidates {
        let Some(write_collection) = state
            .manifests
            .get(&(old.source_id.clone(), old.generation.clone()))
            .and_then(|m| {
                m.metadata
                    .get(crate::GENERATION_VECTOR_COLLECTION_METADATA_KEY)
            })
            .and_then(|v| v.as_str())
            .filter(|v| !v.trim().is_empty())
            .map(str::to_string)
        else {
            continue;
        };
        old.status = LifecycleStatus::Failed;
        let mut debt =
            crate::cleanup_debt::generation_vector_delete_debt(&old.source_id, &old.generation);
        debt.vector_collection = Some(write_collection);
        debt.job_id = lease.job_id.unwrap_or(JobId::new(uuid::Uuid::nil()));
        old.cleanup_debt.push(debt.debt_id.clone());
        state
            .generations
            .insert((old.source_id.clone(), old.generation.clone()), old);
        state
            .cleanup_debt
            .entry(debt.debt_id.clone())
            .or_insert(debt);
        recovered += 1;
    }
    Ok(recovered)
}
