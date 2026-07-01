use std::sync::Arc;

use axon_api::source::*;
use tokio::sync::Mutex;

use super::FakeLedgerState;
use crate::store::Result;
use crate::store::util::{add_seconds, lease_missing_error, timestamp, timestamp_after};

pub(super) async fn acquire_lease(
    state: &Arc<Mutex<FakeLedgerState>>,
    request: LeaseRequest,
) -> Result<Option<LeaseGuard>> {
    let now = timestamp();
    let mut state = state.lock().await;
    if let Some(existing_id) = state.lease_ids_by_key.get(&request.lease_key).cloned() {
        let existing = state.leases.get(&existing_id).cloned();
        match existing {
            Some(existing) if timestamp_after(&existing.expires_at, &now)? => {
                if existing.owner_id == request.owner_id {
                    let guard = LeaseGuard {
                        expires_at: add_seconds(&now, request.ttl_seconds),
                        heartbeat_at: now.clone(),
                        acquired_at: existing.acquired_at,
                        job_id: request.job_id,
                        metadata: request.metadata,
                        ..existing
                    };
                    state.leases.insert(existing_id, guard.clone());
                    return Ok(Some(guard));
                }
                return Ok(None);
            }
            Some(_) | None => {
                state.leases.remove(&existing_id);
                state.lease_ids_by_key.remove(&request.lease_key);
            }
        }
    }

    let guard = LeaseGuard {
        lease_id: LeaseId::new(format!("lease_{}", uuid::Uuid::new_v4())),
        lease_key: request.lease_key,
        owner_id: request.owner_id,
        expires_at: add_seconds(&now, request.ttl_seconds),
        heartbeat_at: now.clone(),
        acquired_at: now,
        job_id: request.job_id,
        metadata: request.metadata,
    };
    state
        .lease_ids_by_key
        .insert(guard.lease_key.clone(), guard.lease_id.clone());
    state.leases.insert(guard.lease_id.clone(), guard.clone());
    Ok(Some(guard))
}

pub(super) async fn release_lease(
    state: &Arc<Mutex<FakeLedgerState>>,
    lease_id: LeaseId,
    owner_id: String,
) -> Result<()> {
    let mut state = state.lock().await;
    let Some(guard) = state.leases.get(&lease_id).cloned() else {
        return Err(lease_missing_error(&lease_id));
    };
    if guard.owner_id != owner_id {
        return Err(ApiError::new(
            "source.ledger.lease_owner_mismatch",
            ErrorStage::Leasing,
            "lease owner does not match release owner",
        ));
    }
    state.leases.remove(&lease_id);
    state.lease_ids_by_key.remove(&guard.lease_key);
    Ok(())
}

pub(super) async fn heartbeat_lease(
    state: &Arc<Mutex<FakeLedgerState>>,
    lease_id: LeaseId,
    owner_id: String,
    ttl_seconds: u64,
) -> Result<Option<LeaseGuard>> {
    let now = timestamp();
    let mut state = state.lock().await;
    let Some(existing) = state.leases.get(&lease_id).cloned() else {
        return Ok(None);
    };
    if existing.owner_id != owner_id || !timestamp_after(&existing.expires_at, &now)? {
        return Ok(None);
    }
    let guard = LeaseGuard {
        heartbeat_at: now.clone(),
        expires_at: add_seconds(&now, ttl_seconds),
        ..existing
    };
    state.leases.insert(lease_id, guard.clone());
    Ok(Some(guard))
}
