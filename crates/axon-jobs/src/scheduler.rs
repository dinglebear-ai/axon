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
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex as StdMutex, Weak};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

const FOREGROUND_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const RECOVERY_POLL: Duration = Duration::from_secs(5);
const AGING_QUANTUM_SECS: i64 = 30;
/// A queued waiter proves liveness by touching `renewed_at` on every grant
/// poll, so abandonment means "no poll recently", not "queued for a while".
/// Deliberately larger than the recovery cadence (`RECOVERY_POLL` plus
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

fn queue_wait_timeout(priority: JobPriority) -> Option<Duration> {
    (!matches!(priority, JobPriority::Background | JobPriority::Maintenance))
        .then_some(FOREGROUND_WAIT_TIMEOUT)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCapacityDomain {
    pub kind: ProviderKind,
    pub instance_id: String,
    pub authority_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerConfig {
    capacity: u32,
    interactive_reserve: u32,
    max_entries: u32,
    max_units: u32,
}

impl SchedulerConfig {
    pub fn new(
        capacity: u32,
        interactive_reserve: u32,
        max_entries: u32,
        max_units: u32,
    ) -> Result<Self, SchedulerError> {
        if capacity == 0 {
            return Err(SchedulerError::InvalidConfig("capacity must be positive"));
        }
        if interactive_reserve > capacity {
            return Err(SchedulerError::InvalidConfig(
                "interactive reserve cannot exceed capacity",
            ));
        }
        if max_entries == 0 {
            return Err(SchedulerError::InvalidConfig(
                "maximum queue entries must be positive",
            ));
        }
        if max_units < capacity {
            return Err(SchedulerError::InvalidConfig(
                "maximum queued units cannot be lower than capacity",
            ));
        }
        Ok(Self {
            capacity,
            interactive_reserve,
            max_entries,
            max_units,
        })
    }
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
pub enum ReservationGrant {
    Queued {
        reservation_id: String,
    },
    Granted {
        reservation_id: String,
        units: NonZeroU32,
    },
}

impl ReservationGrant {
    pub fn reservation_id(&self) -> &str {
        match self {
            Self::Queued { reservation_id } | Self::Granted { reservation_id, .. } => {
                reservation_id
            }
        }
    }

    pub fn is_granted(&self) -> bool {
        matches!(self, Self::Granted { .. })
    }

    pub fn units(&self) -> u32 {
        match self {
            Self::Queued { .. } => 0,
            Self::Granted { units, .. } => units.get(),
        }
    }
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
    #[error("scheduler database state is inconsistent: {0}")]
    DatabaseState(&'static str),
    #[error("provider request exceeds declared capacity")]
    RequestTooLarge,
    #[error("invalid scheduler configuration: {0}")]
    InvalidConfig(&'static str),
    #[error("scheduler queue limit reached")]
    QueueFull,
    #[error("scheduler lease fence rejected")]
    StaleFence,
    #[error("scheduler reservation is queued")]
    Queued,
    #[error("scheduler reservation wait deadline expired")]
    WaitTimeout,
    #[error("scheduler operation failed ({operation}); rollback also failed ({rollback})")]
    RollbackFailed { operation: String, rollback: String },
}

async fn rollback_after_error(
    connection: &mut PoolConnection<Sqlite>,
    operation_error: SchedulerError,
) -> SchedulerError {
    match sqlx::query("ROLLBACK").execute(&mut **connection).await {
        Ok(_) => operation_error,
        Err(rollback_error) => {
            // The transaction state is uncertain. Never return this connection
            // to the pool, even though the pool's release hook is a second net.
            connection.close_on_drop();
            tracing::error!(
                error_code = "provider_scheduler_rollback_failed",
                "provider scheduler rollback failed; evicting connection"
            );
            SchedulerError::RollbackFailed {
                operation: operation_error.to_string(),
                rollback: rollback_error.to_string(),
            }
        }
    }
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
    dispatch_signal: Arc<DispatchSignal>,
}

#[derive(Debug, Default)]
struct DispatchSignal {
    changed: Notify,
    recovery_claimed: AtomicBool,
}

struct RecoveryClaim(Arc<DispatchSignal>);

impl DispatchSignal {
    fn try_claim_recovery(self: &Arc<Self>) -> Option<RecoveryClaim> {
        self.recovery_claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| RecoveryClaim(Arc::clone(self)))
    }
}

impl Drop for RecoveryClaim {
    fn drop(&mut self) {
        self.0.recovery_claimed.store(false, Ordering::Release);
        self.0.changed.notify_waiters();
    }
}

type CapacityNotifierKey = (String, String);
type CapacityNotifierMap = std::collections::HashMap<CapacityNotifierKey, Weak<DispatchSignal>>;

static CAPACITY_NOTIFIERS: LazyLock<StdMutex<CapacityNotifierMap>> =
    LazyLock::new(|| StdMutex::new(std::collections::HashMap::new()));

fn shared_dispatch_signal(
    _authority_id: &str,
    capacity_domain: &str,
    instance_id: &str,
) -> Arc<DispatchSignal> {
    let mut notifiers = CAPACITY_NOTIFIERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let key = (capacity_domain.to_string(), instance_id.to_string());
    if let Some(notify) = notifiers.get(&key).and_then(Weak::upgrade) {
        return notify;
    }
    let signal = Arc::new(DispatchSignal::default());
    notifiers.insert(key, Arc::downgrade(&signal));
    signal
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
        let capacity_domain = domain_name(domain.kind)?;
        let dispatch_signal =
            shared_dispatch_signal(&domain.authority_id, &capacity_domain, &domain.instance_id);
        Ok(Self {
            pool,
            domain,
            config,
            write_gate,
            dispatch_signal,
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
        let _write_permit = self.write_gate.lock().await;
        let mut connection = self.pool.acquire().await?;
        begin_immediate(&mut connection).await?;
        let result = self.reserve_locked(&mut connection, request).await;
        match result {
            Ok(grant) => {
                sqlx::query("COMMIT").execute(&mut *connection).await?;
                self.dispatch_signal.changed.notify_waiters();
                Ok(grant)
            }
            Err(error) => Err(rollback_after_error(&mut connection, error).await),
        }
    }

    pub async fn reserve_wait(
        &self,
        request: ReservationRequest,
    ) -> Result<ReservationGrant, SchedulerError> {
        let fence = request.fence.clone();
        // Durable worker work may legitimately queue behind a provider call
        // whose timeout plus retries exceeds 30 seconds. Its continuously
        // renewed queue row is the liveness boundary; foreground requests keep
        // the bounded UX deadline.
        let wait_timeout = queue_wait_timeout(request.priority);
        let grant = self.reserve(request).await?;
        if grant.is_granted() {
            return Ok(grant);
        }
        let mut guard =
            WaitingReservationGuard::new(self.clone(), grant.reservation_id().to_string(), fence);
        let result = self
            .wait_for_grant(grant.reservation_id().to_string(), wait_timeout)
            .await;
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
        wait_timeout: Option<Duration>,
    ) -> Result<ReservationGrant, SchedulerError> {
        let started = Instant::now();
        loop {
            let capacity_changed = self.dispatch_signal.changed.notified();
            tokio::pin!(capacity_changed);
            capacity_changed.as_mut().enable();
            let grant = self.reservation_grant(&reservation_id).await?;
            if grant.is_granted() {
                return Ok(grant);
            }
            if wait_timeout.is_some_and(|timeout| started.elapsed() >= timeout) {
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
            // Normal in-process capacity changes are event-driven. The slow
            // timeout remains solely as cross-process/crash recovery because
            // another process cannot signal this Notify.
            if let Some(_claim) = self.dispatch_signal.try_claim_recovery() {
                tokio::select! {
                    _ = &mut capacity_changed => {}
                    _ = tokio::time::sleep(RECOVERY_POLL) => {}
                }
                self.dispatch_queued().await?;
                self.dispatch_signal.changed.notify_waiters();
            } else {
                capacity_changed.await;
            }
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
        self.dispatch_signal.changed.notify_waiters();
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
        self.dispatch_signal.changed.notify_waiters();
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
        self.dispatch_signal.changed.notify_waiters();
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
        self.dispatch_signal.changed.notify_waiters();
        Ok(())
    }
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
