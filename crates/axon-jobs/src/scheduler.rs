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
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use uuid::Uuid;

const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_MIN: Duration = Duration::from_millis(20);
const POLL_MAX: Duration = Duration::from_millis(250);
const AGING_QUANTUM_SECS: i64 = 30;
/// A queued waiter proves liveness by touching `renewed_at` on every grant
/// poll, so abandonment means "no poll recently", not "queued for a while".
/// Deliberately larger than the slowest poll cadence (`POLL_MAX` plus
/// writer-gate stalls) and decoupled from `WAIT_TIMEOUT` so third parties
/// never expire a live waiter and priority aging (`AGING_QUANTUM_SECS`,
/// measured from the untouched `updated_at`) can actually progress.
const QUEUED_LIVENESS_TIMEOUT_SECS: i64 = 90;
/// Quarantined-active leases whose fence has not renewed for this long are
/// terminalized by `reconcile`, releasing their granted units. Renewal clears
/// quarantine, so a live lease that is still renewing can never reach this;
/// the margin over the 60-second quarantine staleness threshold is the grace
/// period for a stalled-but-recovering holder.
const QUARANTINE_RELEASE_SECS: i64 = 120;
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
mod lease;
mod reconcile;
use lease::WaitingReservationGuard;
pub use lease::{ActiveReservationLease, call_reserved};
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
    write_gate: SqliteWriteGate,
}

/// Process-local admission gate for scheduler mutations sharing one SQLite DB.
///
/// SQLite admits one writer at a time. Without this gate, concurrent provider
/// calls can park every SQLx connection worker inside SQLite's busy handler,
/// starving unrelated job heartbeats and control-plane reads of a pool slot.
///
/// The gate is intentionally process-local even though the DB is shared with
/// short-lived CLI processes: cross-process writers are serialized by SQLite's
/// own write lock, so the accepted bound is that a gate holder may stall up to
/// the busy timeout behind an external writer while in-process writers queue
/// behind the gate.
#[derive(Debug, Clone, Default)]
pub struct SqliteWriteGate(Arc<Mutex<()>>);

/// Backward-compatible scheduler-facing name for the shared SQLite writer gate.
pub type SchedulerWriteGate = SqliteWriteGate;

impl SqliteWriteGate {
    #[doc(hidden)]
    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.0.lock().await
    }

    /// Attempt admission without parking behind another SQLite writer.
    pub fn try_lock(&self) -> Option<tokio::sync::MutexGuard<'_, ()>> {
        self.0.try_lock().ok()
    }
}

impl ProviderScheduler {
    pub fn new(
        pool: SqlitePool,
        domain: ProviderCapacityDomain,
        config: SchedulerConfig,
    ) -> Result<Self, SchedulerError> {
        Self::new_with_write_gate(pool, domain, config, SchedulerWriteGate::default())
    }

    pub fn new_with_write_gate(
        pool: SqlitePool,
        domain: ProviderCapacityDomain,
        config: SchedulerConfig,
        write_gate: SqliteWriteGate,
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
            write_gate,
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
        let _write_permit = self.write_gate.lock().await;
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
                let _write_permit = self.write_gate.lock().await;
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
        let _write_permit = self.write_gate.lock().await;
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
        let _write_permit = self.write_gate.lock().await;
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
        let _write_permit = self.write_gate.lock().await;
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
        let _write_permit = self.write_gate.lock().await;
        // A successful renewal proves the holder is alive, so it also clears
        // quarantine: reconcile only terminalizes quarantined rows whose
        // renewals have stopped, keeping live leases immune to capacity loss.
        let changed = sqlx::query(
            "UPDATE provider_reservations SET renewed_at = datetime('now'), quarantined = 0,
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
        self.release(reservation_id, fence, "provider_failed").await
    }

    async fn release(
        &self,
        reservation_id: &str,
        fence: &str,
        reason: &str,
    ) -> Result<(), SchedulerError> {
        let _write_permit = self.write_gate.lock().await;
        let changed = sqlx::query(
            "UPDATE provider_reservations SET status = 'released', granted_units = 0,
             terminal_reason = ?, updated_at = datetime('now')
             WHERE reservation_id = ? AND fence = ? AND authority_id = ? AND status IN ('granted','active')",
        )
        .bind(reason)
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
        let _write_permit = self.write_gate.lock().await;
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

#[cfg(test)]
#[path = "scheduler_fairness_tests.rs"]
mod fairness_tests;
