//! Reservation lease lifecycle for the SQLite provider scheduler: the active
//! lease handle, the queued/active drop guards, and the `call_reserved`
//! wrapper that executes one provider operation under a granted reservation.
//! Split from `scheduler.rs` to stay under the monolith line cap.

use std::future::Future;
#[cfg(test)]
use std::sync::{
    Mutex as StdMutex,
    atomic::{AtomicUsize, Ordering},
};

use axon_api::source::{
    JobPriority, ProviderId, ProviderReservationSnapshot, ProviderReservationStatus, ReservationId,
    Timestamp,
};
use sha2::{Digest, Sha256};

use super::{
    ProviderScheduler, RENEW_INTERVAL, ReservationRequest, ReservedCallError, SchedulerError,
};

#[derive(Debug)]
struct ActiveReservationLease<K> {
    scheduler: ProviderScheduler,
    reservation_id: String,
    fence: String,
    _kind: std::marker::PhantomData<fn() -> K>,
}

pub(super) struct WaitingReservationGuard {
    scheduler: ProviderScheduler,
    reservation_id: String,
    fence: String,
    armed: bool,
}

impl WaitingReservationGuard {
    pub(super) fn new(scheduler: ProviderScheduler, reservation_id: String, fence: String) -> Self {
        Self {
            scheduler,
            reservation_id,
            fence,
            armed: true,
        }
    }

    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for WaitingReservationGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let scheduler = self.scheduler.clone();
        let reservation_id = self.reservation_id.clone();
        let fence = self.fence.clone();
        let cleanup_reservation_id = reservation_id.clone();
        let fence_fingerprint = fence_fingerprint(&fence);
        spawn_drop_cleanup(
            async move {
                scheduler
                    .cancel_waiting(&reservation_id, &fence, "waiter_dropped")
                    .await
            },
            "queued",
            cleanup_reservation_id,
            fence_fingerprint,
        );
    }
}

/// Best-effort release for the active lease phase, mirroring what
/// `WaitingReservationGuard` does for the queued phase: if the `call_reserved`
/// future is dropped after `activate()` (caller-side timeout/`select!`), the
/// guard spawns a release so the granted units return to the domain instead of
/// waiting for reconcile to quarantine and terminalize the orphaned row.
struct ActiveReservationGuard {
    scheduler: ProviderScheduler,
    reservation_id: String,
    fence: String,
    armed: bool,
}

impl ActiveReservationGuard {
    fn new(scheduler: ProviderScheduler, reservation_id: String, fence: String) -> Self {
        Self {
            scheduler,
            reservation_id,
            fence,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ActiveReservationGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let scheduler = self.scheduler.clone();
        let reservation_id = self.reservation_id.clone();
        let fence = self.fence.clone();
        let cleanup_reservation_id = reservation_id.clone();
        let fence_fingerprint = fence_fingerprint(&fence);
        spawn_drop_cleanup(
            async move {
                scheduler
                    .release(&reservation_id, &fence, "call_dropped")
                    .await
            },
            "active",
            cleanup_reservation_id,
            fence_fingerprint,
        );
    }
}

fn fence_fingerprint(fence: &str) -> String {
    let digest = Sha256::digest(fence.as_bytes());
    hex::encode(&digest[..8])
}

fn spawn_drop_cleanup<F>(
    cleanup: F,
    phase: &'static str,
    reservation_id: String,
    fence_fingerprint: String,
) where
    F: Future<Output = Result<(), SchedulerError>> + Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            if let Err(error) = cleanup.await {
                record_drop_cleanup_failure(
                    phase,
                    "async_cleanup_failed",
                    &reservation_id,
                    &fence_fingerprint,
                    &error.to_string(),
                );
            }
        });
    } else {
        record_drop_cleanup_failure(
            phase,
            "runtime_unavailable",
            &reservation_id,
            &fence_fingerprint,
            "Tokio runtime unavailable; cleanup was not scheduled",
        );
    }
}

fn record_drop_cleanup_failure(
    phase: &'static str,
    reason: &'static str,
    reservation_id: &str,
    fence_fingerprint: &str,
    error: &str,
) {
    #[cfg(test)]
    {
        DROP_CLEANUP_FAILURES.fetch_add(1, Ordering::Relaxed);
        *LAST_DROP_CLEANUP_FAILURE
            .lock()
            .expect("failure capture lock") = Some(DropCleanupFailure {
            phase: phase.to_string(),
            reason: reason.to_string(),
            reservation_id: reservation_id.to_string(),
            fence_fingerprint: fence_fingerprint.to_string(),
            error: error.to_string(),
        });
    }
    tracing::error!(
        error_code = "provider_scheduler_drop_cleanup_failed",
        reservation_phase = phase,
        failure_reason = reason,
        reservation_id,
        fence_fingerprint,
        error,
        "provider scheduler drop cleanup did not complete"
    );
}

#[cfg(test)]
static DROP_CLEANUP_FAILURES: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
#[derive(Clone, Debug)]
struct DropCleanupFailure {
    phase: String,
    reason: String,
    reservation_id: String,
    fence_fingerprint: String,
    error: String,
}

#[cfg(test)]
static LAST_DROP_CLEANUP_FAILURE: StdMutex<Option<DropCleanupFailure>> = StdMutex::new(None);

#[cfg(test)]
static DROP_CLEANUP_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
#[derive(Clone, Debug)]
struct CompletionFenceWarning {
    reservation_id: String,
    provider_kind: String,
    provider_id: String,
    capacity_domain: String,
}

#[cfg(test)]
static LAST_COMPLETION_FENCE_WARNING: StdMutex<Option<CompletionFenceWarning>> =
    StdMutex::new(None);

#[cfg(test)]
#[path = "lease_tests.rs"]
mod tests;

impl<K> Clone for ActiveReservationLease<K> {
    fn clone(&self) -> Self {
        Self {
            scheduler: self.scheduler.clone(),
            reservation_id: self.reservation_id.clone(),
            fence: self.fence.clone(),
            _kind: std::marker::PhantomData,
        }
    }
}

impl<K> ActiveReservationLease<K> {
    async fn renew(&self) -> Result<(), SchedulerError> {
        self.scheduler
            .renew(&self.reservation_id, &self.fence)
            .await
    }

    async fn complete(self) -> Result<(), SchedulerError> {
        self.scheduler
            .complete(&self.reservation_id, &self.fence)
            .await
    }

    async fn fail(self) -> Result<(), SchedulerError> {
        self.scheduler.fail(&self.reservation_id, &self.fence).await
    }
}

/// Read-only reservation context passed to provider operations.
///
/// Terminal lifecycle ownership stays in [`call_reserved`], so provider code
/// cannot release capacity before its operation future completes.
#[derive(Debug)]
pub struct ReservationObservation<K> {
    reservation_id: String,
    provider_kind: axon_api::source::ProviderKind,
    provider_id: String,
    _kind: std::marker::PhantomData<fn() -> K>,
}

impl<K> ReservationObservation<K> {
    #[must_use]
    pub fn snapshot(
        &self,
        priority: JobPriority,
        requested_units: u32,
    ) -> ProviderReservationSnapshot {
        ProviderReservationSnapshot {
            reservation_id: ReservationId::new(self.reservation_id.clone()),
            provider_kind: self.provider_kind,
            provider_id: Some(ProviderId::new(self.provider_id.clone())),
            priority,
            requested_units,
            granted_units: requested_units,
            acquired_at: Some(Timestamp::from(chrono::Utc::now())),
            expires_at: None,
            status: ProviderReservationStatus::Active,
            queue_depth: None,
            cooling: None,
        }
    }
}

async fn release_failed_call<K>(
    lease: &ActiveReservationLease<K>,
    release_guard: &mut ActiveReservationGuard,
    failure_context: &'static str,
) {
    match lease.clone().fail().await {
        Ok(()) | Err(SchedulerError::StaleFence) => release_guard.disarm(),
        Err(release_error) => tracing::warn!(
            reservation_id = %lease.reservation_id,
            error = %release_error,
            failure_context,
            "reservation release failed after provider call failure",
        ),
    }
}

fn record_completion_stale_fence<K>(lease: &ActiveReservationLease<K>) {
    let provider_kind = format!("{:?}", lease.scheduler.domain.kind);
    let provider_id = &lease.scheduler.domain.instance_id;
    let capacity_domain =
        super::domain_name(lease.scheduler.domain.kind).unwrap_or_else(|_| "unknown".to_string());
    #[cfg(test)]
    {
        *LAST_COMPLETION_FENCE_WARNING
            .lock()
            .expect("completion warning capture lock") = Some(CompletionFenceWarning {
            reservation_id: lease.reservation_id.clone(),
            provider_kind: provider_kind.clone(),
            provider_id: provider_id.clone(),
            capacity_domain: capacity_domain.clone(),
        });
    }
    tracing::warn!(
        reservation_id = %lease.reservation_id,
        provider_kind,
        provider_id,
        capacity_domain,
        "reservation fence lost at completion; returning finished provider result",
    );
}

/// Execute one provider operation only after the SQLite scheduler has granted
/// capacity. Provider traits stay unchanged; the lease is the only value the
/// operation receives from the scheduler boundary.
pub async fn call_reserved<K, T, E, F, Fut>(
    scheduler: &ProviderScheduler,
    request: ReservationRequest,
    operation: F,
) -> Result<T, ReservedCallError<E>>
where
    F: FnOnce(ReservationObservation<K>) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let fence = request.fence.clone();
    let grant = scheduler.reserve_wait(request).await?;
    let lease: ActiveReservationLease<K> = ActiveReservationLease {
        scheduler: scheduler.clone(),
        reservation_id: grant.reservation_id().to_string(),
        fence,
        _kind: std::marker::PhantomData,
    };
    scheduler
        .activate(&lease.reservation_id, &lease.fence)
        .await?;
    let mut release_guard = ActiveReservationGuard::new(
        scheduler.clone(),
        lease.reservation_id.clone(),
        lease.fence.clone(),
    );
    let operation = operation(ReservationObservation {
        reservation_id: lease.reservation_id.clone(),
        provider_kind: lease.scheduler.domain.kind,
        provider_id: lease.scheduler.domain.instance_id.clone(),
        _kind: std::marker::PhantomData,
    });
    tokio::pin!(operation);
    let mut renewal = tokio::time::interval(RENEW_INTERVAL);
    renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    renewal.tick().await;
    let value = loop {
        tokio::select! {
            result = &mut operation => {
                break match result {
                    Ok(value) => value,
                    Err(error) => {
                        // Release capacity, but never let a failing release
                        // mask the provider root cause; the drop guard retries
                        // a release the fence still owns.
                        release_failed_call(&lease, &mut release_guard, "provider_error").await;
                        return Err(ReservedCallError::Provider(error));
                    }
                };
            }
            _ = renewal.tick() => {
                if let Err(renew_error) = lease.renew().await {
                    // The pinned operation is dropped when we return, so a
                    // transient renew failure must not leave the row active
                    // and holding units until reconcile notices.
                    release_failed_call(&lease, &mut release_guard, "renew_error").await;
                    return Err(renew_error.into());
                }
            }
        }
    };
    match lease.clone().complete().await {
        Ok(()) => release_guard.disarm(),
        // The provider work succeeded and is already paid for; losing the
        // fence at completion means a third party (job cancel, reconcile
        // terminalization) already terminalized the reservation and released
        // its units, so returning the value cannot oversubscribe the domain.
        // The job observes cancellation through job-level control flow.
        Err(SchedulerError::StaleFence) => {
            release_guard.disarm();
            record_completion_stale_fence(&lease);
        }
        Err(error) => return Err(error.into()),
    }
    Ok(value)
}
