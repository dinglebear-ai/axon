//! Reservation lease lifecycle for the SQLite provider scheduler: the active
//! lease handle, the queued/active drop guards, and the `call_reserved`
//! wrapper that executes one provider operation under a granted reservation.
//! Split from `scheduler.rs` to stay under the monolith line cap.

use std::future::Future;

use axon_api::source::{
    JobPriority, ProviderId, ProviderReservationSnapshot, ProviderReservationStatus, ReservationId,
    Timestamp,
};

use super::{
    ProviderScheduler, RENEW_INTERVAL, ReservationRequest, ReservedCallError, SchedulerError,
};

#[derive(Debug)]
pub struct ActiveReservationLease<K> {
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
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = scheduler
                    .cancel_waiting(&reservation_id, &fence, "waiter_dropped")
                    .await;
            });
        }
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
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = scheduler
                    .release(&reservation_id, &fence, "call_dropped")
                    .await;
            });
        }
    }
}

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
    #[must_use]
    pub fn snapshot(
        &self,
        priority: JobPriority,
        requested_units: u32,
    ) -> ProviderReservationSnapshot {
        ProviderReservationSnapshot {
            reservation_id: ReservationId::new(self.reservation_id.clone()),
            provider_kind: self.scheduler.domain.kind,
            provider_id: Some(ProviderId::new(self.scheduler.domain.instance_id.clone())),
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

    pub async fn renew(&self) -> Result<(), SchedulerError> {
        self.scheduler
            .renew(&self.reservation_id, &self.fence)
            .await
    }

    pub async fn complete(self) -> Result<(), SchedulerError> {
        self.scheduler
            .complete(&self.reservation_id, &self.fence)
            .await
    }

    pub async fn cancel(self) -> Result<(), SchedulerError> {
        self.scheduler
            .cancel(&self.reservation_id, &self.fence)
            .await
    }

    pub async fn fail(self) -> Result<(), SchedulerError> {
        self.scheduler.fail(&self.reservation_id, &self.fence).await
    }
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
    F: FnOnce(ActiveReservationLease<K>) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let fence = request.fence.clone();
    let grant = scheduler.reserve_wait(request).await?;
    let lease = ActiveReservationLease {
        scheduler: scheduler.clone(),
        reservation_id: grant.reservation_id,
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
    let operation = operation(lease.clone());
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
                        match lease.clone().fail().await {
                            Ok(()) | Err(SchedulerError::StaleFence) => release_guard.disarm(),
                            Err(release_error) => tracing::warn!(
                                reservation_id = %lease.reservation_id,
                                error = %release_error,
                                "reservation release failed after provider error",
                            ),
                        }
                        return Err(ReservedCallError::Provider(error));
                    }
                };
            }
            _ = renewal.tick() => {
                if let Err(renew_error) = lease.renew().await {
                    // The pinned operation is dropped when we return, so a
                    // transient renew failure must not leave the row active
                    // and holding units until reconcile notices.
                    match lease.clone().fail().await {
                        Ok(()) | Err(SchedulerError::StaleFence) => release_guard.disarm(),
                        Err(release_error) => tracing::warn!(
                            reservation_id = %lease.reservation_id,
                            error = %release_error,
                            "reservation release failed after renew error",
                        ),
                    }
                    return Err(renew_error.into());
                }
            }
        }
    };
    match lease.complete().await {
        Ok(()) => release_guard.disarm(),
        // The provider work succeeded and is already paid for; losing the
        // fence at completion means a third party (job cancel, reconcile
        // terminalization) already terminalized the reservation and released
        // its units, so returning the value cannot oversubscribe the domain.
        // The job observes cancellation through job-level control flow.
        Err(SchedulerError::StaleFence) => {
            release_guard.disarm();
            tracing::warn!(
                "reservation fence lost at completion; returning finished provider result",
            );
        }
        Err(error) => return Err(error.into()),
    }
    Ok(value)
}
