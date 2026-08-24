use axon_api::source::*;
use axon_error::ErrorStage;

use super::FakeJobWatchStore;
use crate::boundary::{JobStore, Result};
use crate::state_machine::validate_stage_plan;

pub(super) async fn admit(
    store: &FakeJobWatchStore,
    admission: ProjectionBatchAdmission,
) -> Result<ProjectionBatchAdmissionResult> {
    for item in &admission.items {
        validate_stage_plan(&item.request.stage_plan)?;
    }
    validate_fingerprints(store, &admission).await?;
    let mut results = Vec::with_capacity(admission.items.len());
    for (index, item) in admission.items.into_iter().enumerate() {
        let reused = store
            .state
            .lock()
            .await
            .projection_fingerprints
            .contains_key(&item.storage_key);
        let mut request = item.request;
        request.idempotency_key = Some(item.storage_key.clone());
        let descriptor = store.create(request).await?;
        store
            .state
            .lock()
            .await
            .projection_fingerprints
            .insert(item.storage_key, item.fingerprint);
        results.push(ProjectionAdmissionResultItem {
            index,
            operation: item.operation,
            descriptor,
            reused,
        });
    }
    store.state.lock().await.projection_batches.insert(
        admission.batch_id,
        (admission.principal_id, results.clone()),
    );
    Ok(ProjectionBatchAdmissionResult {
        batch_id: admission.batch_id,
        items: results,
    })
}

async fn validate_fingerprints(
    store: &FakeJobWatchStore,
    admission: &ProjectionBatchAdmission,
) -> Result<()> {
    let state = store.state.lock().await;
    for item in &admission.items {
        if let Some(stored) = state.projection_fingerprints.get(&item.storage_key)
            && stored != &item.fingerprint
        {
            return Err(ApiError::new(
                "projection.idempotency_collision",
                ErrorStage::Storage,
                "idempotency key was already used for a different request",
            ));
        }
    }
    Ok(())
}

pub(super) async fn lookup(
    store: &FakeJobWatchStore,
    lookup: ProjectionBatchLookup,
) -> Result<Option<ProjectionBatchAdmissionResult>> {
    Ok(store
        .state
        .lock()
        .await
        .projection_batches
        .get(&lookup.batch_id)
        .filter(|(principal, _)| principal == &lookup.principal_id)
        .map(|(_, items)| ProjectionBatchAdmissionResult {
            batch_id: lookup.batch_id,
            items: items.clone(),
        }))
}
