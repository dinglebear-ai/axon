//! SQLite-authoritative provider scheduler primitives.
//!
//! This is the durable queue boundary. The provider traits remain unaware of
//! scheduling; callers first obtain a grant here and only then invoke a
//! provider. The in-memory reservation manager is intentionally not used by
//! this module.

use axon_api::source::{JobId, JobPriority, ProviderKind, StageId};
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

    async fn reserve_locked(
        &self,
        connection: &mut PoolConnection<Sqlite>,
        request: ReservationRequest,
    ) -> Result<ReservationGrant, SchedulerError> {
        let domain = domain_name(self.domain.kind)?;
        self.expire_abandoned_queued_locked(connection, &domain)
            .await?;
        self.ensure_capacity(connection, &domain, &request).await?;
        let id = self.insert_queued(connection, &domain, &request).await?;
        self.grant_head_locked(connection, &domain).await?;
        self.reservation_grant_locked(connection, &id).await
    }

    async fn try_grant_existing(
        &self,
        reservation_id: &str,
    ) -> Result<ReservationGrant, SchedulerError> {
        let mut connection = self.pool.acquire().await?;
        begin_immediate(&mut connection).await?;
        let domain = domain_name(self.domain.kind)?;
        let result = async {
            self.grant_head_locked(&mut connection, &domain).await?;
            self.reservation_grant_locked(&mut connection, reservation_id)
                .await
        }
        .await;
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

    async fn grant_head_locked(
        &self,
        connection: &mut PoolConnection<Sqlite>,
        domain: &str,
    ) -> Result<(), SchedulerError> {
        self.refresh_effective_priorities(connection, domain)
            .await?;
        let candidate: Option<(String, i64, String)> = sqlx::query_as(
            "SELECT reservation_id, requested_units, effective_priority
             FROM provider_reservations
             WHERE capacity_domain = ? AND instance_id = ? AND status = 'queued'
             ORDER BY CASE effective_priority
                 WHEN 'interactive' THEN 0 WHEN 'high' THEN 1 WHEN 'normal' THEN 2
                 WHEN 'background' THEN 3 ELSE 4 END,
                 enqueue_sequence, reservation_id LIMIT 1",
        )
        .bind(domain)
        .bind(&self.domain.instance_id)
        .fetch_optional(&mut **connection)
        .await?;
        let Some((candidate_id, candidate_units, candidate_priority)) = candidate else {
            return Ok(());
        };
        let active: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(granted_units), 0) FROM provider_reservations
             WHERE capacity_domain = ? AND instance_id = ? AND status IN ('granted','active')",
        )
        .bind(domain)
        .bind(&self.domain.instance_id)
        .fetch_one(&mut **connection)
        .await?;
        let interactive_queued: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM provider_reservations
             WHERE capacity_domain = ? AND instance_id = ? AND status = 'queued'
               AND effective_priority = 'interactive'",
        )
        .bind(domain)
        .bind(&self.domain.instance_id)
        .fetch_one(&mut **connection)
        .await?;
        let limit = if candidate_priority != "interactive" && interactive_queued > 0 {
            self.config
                .capacity
                .saturating_sub(self.config.interactive_reserve)
        } else {
            self.config.capacity
        };
        if active + candidate_units > i64::from(limit) {
            return Ok(());
        }
        sqlx::query(
            "UPDATE provider_reservations SET status = 'granted', granted_units = ?,
             acquired_at = datetime('now'), grant_deadline = datetime('now', '+30 seconds'),
             expires_at = datetime('now', '+300 seconds'), lease_owner = ?, authority_id = ?,
             updated_at = datetime('now') WHERE reservation_id = ? AND status = 'queued'",
        )
        .bind(candidate_units)
        .bind(&self.domain.authority_id)
        .bind(&self.domain.authority_id)
        .bind(candidate_id)
        .execute(&mut **connection)
        .await?;
        Ok(())
    }

    async fn refresh_effective_priorities(
        &self,
        connection: &mut PoolConnection<Sqlite>,
        domain: &str,
    ) -> Result<(), SchedulerError> {
        sqlx::query(
            "UPDATE provider_reservations
             SET effective_priority = CASE max(0,
                    CASE requested_priority
                      WHEN 'interactive' THEN 0 WHEN 'high' THEN 1 WHEN 'normal' THEN 2
                      WHEN 'background' THEN 3 ELSE 4 END
                    - min(4, max(0, (unixepoch('now') - unixepoch(updated_at)) / ?)))
                  WHEN 0 THEN 'interactive' WHEN 1 THEN 'high' WHEN 2 THEN 'normal'
                  WHEN 3 THEN 'background' ELSE 'maintenance' END
             WHERE capacity_domain = ? AND instance_id = ? AND status = 'queued'",
        )
        .bind(AGING_QUANTUM_SECS)
        .bind(domain)
        .bind(&self.domain.instance_id)
        .execute(&mut **connection)
        .await?;
        Ok(())
    }

    async fn reservation_grant_locked(
        &self,
        connection: &mut PoolConnection<Sqlite>,
        reservation_id: &str,
    ) -> Result<ReservationGrant, SchedulerError> {
        let row: Option<(String, i64, i64)> = sqlx::query_as(
            "SELECT status, requested_units, granted_units FROM provider_reservations
             WHERE reservation_id = ? AND authority_id = ?",
        )
        .bind(reservation_id)
        .bind(&self.domain.authority_id)
        .fetch_optional(&mut **connection)
        .await?;
        let Some((status, requested_units, granted_units)) = row else {
            return Err(SchedulerError::StaleFence);
        };
        match status.as_str() {
            "queued" => Ok(ReservationGrant {
                reservation_id: reservation_id.to_string(),
                granted: false,
                units: 0,
            }),
            "granted" | "active" => Ok(ReservationGrant {
                reservation_id: reservation_id.to_string(),
                granted: true,
                units: u32::try_from(granted_units.max(requested_units)).unwrap_or(u32::MAX),
            }),
            _ => Err(SchedulerError::StaleFence),
        }
    }

    async fn expire_abandoned_queued_locked(
        &self,
        connection: &mut PoolConnection<Sqlite>,
        domain: &str,
    ) -> Result<u64, SchedulerError> {
        Ok(sqlx::query(
            "UPDATE provider_reservations SET status = 'expired', granted_units = 0,
             terminal_reason = 'abandoned_waiter', updated_at = datetime('now')
             WHERE capacity_domain = ? AND instance_id = ? AND authority_id = ?
               AND status = 'queued' AND updated_at <= datetime('now', '-30 seconds')",
        )
        .bind(domain)
        .bind(&self.domain.instance_id)
        .bind(&self.domain.authority_id)
        .execute(&mut **connection)
        .await?
        .rows_affected())
    }

    async fn ensure_capacity(
        &self,
        connection: &mut PoolConnection<Sqlite>,
        domain: &str,
        request: &ReservationRequest,
    ) -> Result<(), SchedulerError> {
        let entries: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM provider_reservations WHERE capacity_domain = ?
             AND instance_id = ? AND status IN ('queued','granted','active')",
        )
        .bind(domain)
        .bind(&self.domain.instance_id)
        .fetch_one(&mut **connection)
        .await?;
        let job_entries: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM provider_reservations WHERE job_id = ?
             AND status IN ('queued','granted','active')",
        )
        .bind(request.job_id.0.to_string())
        .fetch_one(&mut **connection)
        .await?;
        let requested_units: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(requested_units), 0) FROM provider_reservations
             WHERE capacity_domain = ? AND instance_id = ? AND status IN ('queued','granted','active')",
        )
        .bind(domain)
        .bind(&self.domain.instance_id)
        .fetch_one(&mut **connection)
        .await?;
        if entries >= i64::from(self.config.max_entries)
            || job_entries >= i64::from(self.config.max_entries)
            || requested_units + i64::from(request.units) > i64::from(self.config.max_units)
        {
            return Err(SchedulerError::QueueFull);
        }
        Ok(())
    }

    async fn insert_queued(
        &self,
        connection: &mut PoolConnection<Sqlite>,
        domain: &str,
        request: &ReservationRequest,
    ) -> Result<String, SchedulerError> {
        let id = format!("sched_{}", Uuid::new_v4());
        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(enqueue_sequence), 0) + 1 FROM provider_reservations
             WHERE capacity_domain = ? AND instance_id = ?",
        )
        .bind(domain)
        .bind(&self.domain.instance_id)
        .fetch_one(&mut **connection)
        .await?;
        let priority = enum_name(request.priority)?;
        let kind = enum_name(self.domain.kind)?;
        sqlx::query(
            "INSERT INTO provider_reservations
             (reservation_id, job_id, stage_id, provider_kind, provider_id, priority,
              requested_units, granted_units, status, updated_at, capacity_domain,
              instance_id, authority_id, enqueue_sequence, requested_priority,
              effective_priority, attempt, fence)
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, 'queued', datetime('now'), ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(request.job_id.0.to_string())
        .bind(request.stage_id.as_ref().map(|stage| stage.0.to_string()))
        .bind(kind)
        .bind(&self.domain.instance_id)
        .bind(&priority)
        .bind(i64::from(request.units))
        .bind(domain)
        .bind(&self.domain.instance_id)
        .bind(&self.domain.authority_id)
        .bind(sequence)
        .bind(&priority)
        .bind(&priority)
        .bind(i64::from(request.attempt))
        .bind(&request.fence)
        .execute(&mut **connection)
        .await?;
        Ok(id)
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
