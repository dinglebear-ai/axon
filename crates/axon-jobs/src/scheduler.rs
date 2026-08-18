//! SQLite-authoritative provider scheduler primitives.
//!
//! This is the durable queue boundary. The provider traits remain unaware of
//! scheduling; callers first obtain a grant here and only then invoke a
//! provider. The in-memory reservation manager is intentionally not used by
//! this module.

use axon_api::source::{
    JobId, JobPriority, ProviderId, ProviderKind, ProviderReservationSnapshot,
    ProviderReservationStatus, ReservationId, StageId, Timestamp,
};
use serde::Serialize;
use sqlx::{Sqlite, pool::PoolConnection};
use sqlx::{SqlitePool, error::Error as SqlxError};
use std::future::Future;
use std::time::{Duration, Instant};
use uuid::Uuid;

const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_MIN: Duration = Duration::from_millis(20);
const POLL_MAX: Duration = Duration::from_millis(250);
const AGING_QUANTUM_SECS: i64 = 30;
#[cfg(not(test))]
const RENEW_INTERVAL: Duration = Duration::from_secs(20);
#[cfg(test)]
const RENEW_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCapacityDomain {
    pub kind: ProviderKind,
    pub instance_id: String,
    pub authority_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerConfig {
    pub capacity: u32,
    pub interactive_reserve: u32,
    pub max_entries: u32,
    pub max_units: u32,
}

#[derive(Debug, Clone)]
pub struct ReservationRequest {
    pub job_id: JobId,
    pub stage_id: Option<StageId>,
    pub attempt: u32,
    pub fence: String,
    pub priority: JobPriority,
    pub units: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservationGrant {
    pub reservation_id: String,
    pub granted: bool,
    pub units: u32,
}

mod grant;
mod reconcile;
pub use reconcile::Reconciliation;

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("scheduler database error: {0}")]
    Database(#[from] SqlxError),
    #[error("provider request exceeds declared capacity")]
    RequestTooLarge,
    #[error("scheduler queue limit reached")]
    QueueFull,
    #[error("scheduler lease fence rejected")]
    StaleFence,
    #[error("scheduler reservation is queued")]
    Queued,
    #[error("scheduler reservation wait deadline expired")]
    WaitTimeout,
}

#[derive(Debug, thiserror::Error)]
pub enum ReservedCallError<E> {
    #[error("provider reservation failed: {0}")]
    Scheduler(#[from] SchedulerError),
    #[error("reserved provider call failed: {0}")]
    Provider(E),
}

#[derive(Debug, Clone)]
pub struct ProviderScheduler {
    pool: SqlitePool,
    domain: ProviderCapacityDomain,
    config: SchedulerConfig,
}

#[derive(Debug)]
pub struct ActiveReservationLease<K> {
    scheduler: ProviderScheduler,
    reservation_id: String,
    fence: String,
    _kind: std::marker::PhantomData<fn() -> K>,
}

struct WaitingReservationGuard {
    scheduler: ProviderScheduler,
    reservation_id: String,
    fence: String,
    armed: bool,
}

impl WaitingReservationGuard {
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
                        lease.fail().await?;
                        return Err(ReservedCallError::Provider(error));
                    }
                };
            }
            _ = renewal.tick() => {
                lease.renew().await?;
            }
        }
    };
    lease.complete().await?;
    Ok(value)
}

impl ProviderScheduler {
    pub fn new(
        pool: SqlitePool,
        domain: ProviderCapacityDomain,
        config: SchedulerConfig,
    ) -> Result<Self, SchedulerError> {
        if config.capacity == 0
            || config.interactive_reserve > config.capacity
            || config.max_entries == 0
            || config.max_units < config.capacity
        {
            return Err(SchedulerError::RequestTooLarge);
        }
        Ok(Self {
            pool,
            domain,
            config,
        })
    }

    /// Enqueue and attempt the head grant atomically. SQLite's write lock is
    /// the authority; no process-local notification or counter participates in
    /// the correctness decision.
    pub async fn reserve(
        &self,
        request: ReservationRequest,
    ) -> Result<ReservationGrant, SchedulerError> {
        if request.units == 0 || request.units > self.config.capacity {
            return Err(SchedulerError::RequestTooLarge);
        }
        // Reclaim grants whose holder never activated them before computing
        // capacity. Previously these rows survived until an external
        // reconciliation pass and could exhaust the domain indefinitely.
        self.reconcile().await?;
        let mut connection = self.pool.acquire().await?;
        begin_immediate(&mut connection).await?;
        let result = self.reserve_locked(&mut connection, request).await;
        match result {
            Ok(grant) => {
                sqlx::query("COMMIT").execute(&mut *connection).await?;
                Ok(grant)
            }
            Err(error) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
                Err(error)
            }
        }
    }

    pub async fn reserve_wait(
        &self,
        request: ReservationRequest,
    ) -> Result<ReservationGrant, SchedulerError> {
        let fence = request.fence.clone();
        let grant = self.reserve(request).await?;
        if grant.granted {
            return Ok(grant);
        }
        let mut guard =
            WaitingReservationGuard::new(self.clone(), grant.reservation_id.clone(), fence);
        let result = self.wait_for_grant(grant.reservation_id).await;
        if matches!(
            &result,
            Ok(_) | Err(SchedulerError::WaitTimeout | SchedulerError::StaleFence)
        ) {
            guard.disarm();
        }
        result
    }

    async fn wait_for_grant(
        &self,
        reservation_id: String,
    ) -> Result<ReservationGrant, SchedulerError> {
        let started = Instant::now();
        let mut poll = 0_u32;
        loop {
            let grant = self.try_grant_existing(&reservation_id).await?;
            if grant.granted {
                return Ok(grant);
            }
            if started.elapsed() >= WAIT_TIMEOUT {
                let changed = sqlx::query(
                    "UPDATE provider_reservations SET status = 'expired', granted_units = 0,
                     terminal_reason = 'queue_timeout', updated_at = datetime('now')
                     WHERE reservation_id = ? AND authority_id = ? AND status = 'queued'",
                )
                .bind(&reservation_id)
                .bind(&self.domain.authority_id)
                .execute(&self.pool)
                .await?
                .rows_affected();
                if changed > 0 {
                    return Err(SchedulerError::WaitTimeout);
                }
                continue;
            }
            tokio::time::sleep(poll_delay(&reservation_id, poll)).await;
            poll = poll.saturating_add(1);
        }
    }

    async fn cancel_waiting(
        &self,
        reservation_id: &str,
        fence: &str,
        reason: &str,
    ) -> Result<(), SchedulerError> {
        sqlx::query(
            "UPDATE provider_reservations SET status = 'canceled', granted_units = 0,
             terminal_reason = ?, updated_at = datetime('now')
             WHERE reservation_id = ? AND fence = ? AND authority_id = ?
               AND status IN ('queued','granted')",
        )
        .bind(reason)
        .bind(reservation_id)
        .bind(fence)
        .bind(&self.domain.authority_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn complete(&self, reservation_id: &str, fence: &str) -> Result<(), SchedulerError> {
        let changed = sqlx::query(
            "UPDATE provider_reservations SET status = 'released', granted_units = 0,
             terminal_reason = 'completed', updated_at = datetime('now')
             WHERE reservation_id = ? AND fence = ? AND authority_id = ? AND status IN ('granted','active')",
        )
        .bind(reservation_id)
        .bind(fence)
        .bind(&self.domain.authority_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(SchedulerError::StaleFence);
        }
        Ok(())
    }

    async fn activate(&self, reservation_id: &str, fence: &str) -> Result<(), SchedulerError> {
        let changed = sqlx::query(
            "UPDATE provider_reservations SET status = 'active', renewed_at = datetime('now'),
             updated_at = datetime('now')
             WHERE reservation_id = ? AND fence = ? AND authority_id = ? AND status = 'granted'
               AND (grant_deadline IS NULL OR grant_deadline > datetime('now'))",
        )
        .bind(reservation_id)
        .bind(fence)
        .bind(&self.domain.authority_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(SchedulerError::StaleFence);
        }
        Ok(())
    }

    async fn renew(&self, reservation_id: &str, fence: &str) -> Result<(), SchedulerError> {
        let changed = sqlx::query(
            "UPDATE provider_reservations SET renewed_at = datetime('now'),
             expires_at = datetime('now', '+300 seconds'), updated_at = datetime('now')
             WHERE reservation_id = ? AND fence = ? AND authority_id = ? AND status = 'active'",
        )
        .bind(reservation_id)
        .bind(fence)
        .bind(&self.domain.authority_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(SchedulerError::StaleFence);
        }
        Ok(())
    }

    async fn fail(&self, reservation_id: &str, fence: &str) -> Result<(), SchedulerError> {
        let changed = sqlx::query(
            "UPDATE provider_reservations SET status = 'released', granted_units = 0,
             terminal_reason = 'provider_failed', updated_at = datetime('now')
             WHERE reservation_id = ? AND fence = ? AND authority_id = ? AND status IN ('granted','active')",
        )
        .bind(reservation_id)
        .bind(fence)
        .bind(&self.domain.authority_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(SchedulerError::StaleFence);
        }
        Ok(())
    }

    async fn cancel(&self, reservation_id: &str, fence: &str) -> Result<(), SchedulerError> {
        let changed = sqlx::query(
            "UPDATE provider_reservations SET status = 'canceled', granted_units = 0,
             terminal_reason = 'caller_cancelled', updated_at = datetime('now')
             WHERE reservation_id = ? AND fence = ? AND authority_id = ? AND status IN ('granted','active')",
        )
        .bind(reservation_id)
        .bind(fence)
        .bind(&self.domain.authority_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(SchedulerError::StaleFence);
        }
        Ok(())
    }
}

fn poll_delay(reservation_id: &str, poll: u32) -> Duration {
    let growth = POLL_MIN
        .saturating_mul(1_u32.checked_shl(poll.min(4)).unwrap_or(u32::MAX))
        .min(POLL_MAX);
    let jitter = reservation_id.bytes().fold(u64::from(poll), |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(u64::from(byte))
    }) % 17;
    growth.saturating_add(Duration::from_millis(jitter))
}

async fn begin_immediate(connection: &mut PoolConnection<Sqlite>) -> Result<(), SqlxError> {
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut **connection)
        .await?;
    Ok(())
}

fn enum_name<T: Serialize>(value: T) -> Result<String, SqlxError> {
    serde_json::to_value(value)
        .map_err(|error| SqlxError::Protocol(error.to_string()))?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| SqlxError::Protocol("scheduler enum was not a string".into()))
}

fn domain_name(kind: ProviderKind) -> Result<String, SqlxError> {
    Ok(enum_name(kind)?.trim_matches('"').to_owned())
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod tests;
