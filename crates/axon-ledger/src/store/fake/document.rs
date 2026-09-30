//! Document status updates and publication for the in-memory ledger.

use super::FakeLedgerState;
use crate::store::Result;
use crate::store::util::source_missing_error;
use axon_api::source::*;
use std::sync::Arc;
use tokio::sync::Mutex;

pub(super) async fn update_document_status(
    state: &Arc<Mutex<FakeLedgerState>>,
    status: DocumentStatus,
) -> Result<()> {
    let mut state = state.lock().await;
    if !state.sources.contains_key(&status.source_id) {
        return Err(source_missing_error(&status.source_id));
    }
    if state
        .document_statuses
        .get(&status.document_id)
        .is_some_and(|existing| existing.updated_at.0 > status.updated_at.0)
    {
        return Ok(());
    }
    state
        .document_statuses
        .insert(status.document_id.clone(), status);
    Ok(())
}

pub(super) async fn update_document_statuses(
    state: &Arc<Mutex<FakeLedgerState>>,
    statuses: Vec<DocumentStatus>,
) -> Result<()> {
    let mut state = state.lock().await;
    for status in &statuses {
        if !state.sources.contains_key(&status.source_id) {
            return Err(source_missing_error(&status.source_id));
        }
    }
    state.document_status_update_batches.push(
        statuses
            .iter()
            .map(|status| status.document_id.clone())
            .collect(),
    );
    for status in statuses {
        if state
            .document_statuses
            .get(&status.document_id)
            .is_none_or(|existing| existing.updated_at.0 <= status.updated_at.0)
        {
            state
                .document_statuses
                .insert(status.document_id.clone(), status);
        }
    }
    Ok(())
}

pub(super) async fn publish_document_statuses(
    state: &Arc<Mutex<FakeLedgerState>>,
    source_id: SourceId,
    generation: SourceGenerationId,
    updated_at: Timestamp,
) -> Result<u64> {
    let mut state = state.lock().await;
    if !state.sources.contains_key(&source_id) {
        return Err(source_missing_error(&source_id));
    }
    let mut updated = 0u64;
    for status in state.document_statuses.values_mut() {
        if status.source_id == source_id
            && status.generation.as_ref() == Some(&generation)
            && matches!(
                status.status,
                DocumentLifecycleStatus::Prepared
                    | DocumentLifecycleStatus::Embedded
                    | DocumentLifecycleStatus::Vectorized
                    | DocumentLifecycleStatus::Published
            )
        {
            status.status = DocumentLifecycleStatus::Published;
            status.updated_at = updated_at.clone();
            updated = updated.saturating_add(1);
        }
    }
    Ok(updated)
}

#[path = "document_carry.rs"]
mod carry;
pub(super) use carry::{
    carry_document_statuses, carry_document_statuses_locked, document_statuses_for_items,
};

impl super::FakeLedgerStore {
    pub async fn document_status(&self, document_id: &DocumentId) -> Option<DocumentStatus> {
        self.state
            .lock()
            .await
            .document_statuses
            .get(document_id)
            .cloned()
    }

    pub async fn document_status_update_batches(&self) -> Vec<Vec<DocumentId>> {
        self.state
            .lock()
            .await
            .document_status_update_batches
            .clone()
    }
}
