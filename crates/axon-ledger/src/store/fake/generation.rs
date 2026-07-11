//! Generation lifecycle (create/complete/fail/publish) for [`FakeLedgerStore`].
//!
//! Split out of `fake.rs` to keep it under the monolith file-size cap — pure
//! state-machine logic over the same `FakeLedgerState`, no new concepts.

use std::sync::Arc;

use axon_api::source::*;
use tokio::sync::Mutex;

use super::{FakeLedgerMode, FakeLedgerState, record_removed_item_cleanup_debt};
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
    record_removed_item_cleanup_debt(&mut state, &generation);
    let cleanup_debt = state
        .cleanup_debt
        .values()
        .filter(|debt| {
            debt.source_id == generation.source_id
                && debt.generation == generation.previous_generation
                && matches!(debt.kind, CleanupDebtKind::VectorDelete)
        })
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
