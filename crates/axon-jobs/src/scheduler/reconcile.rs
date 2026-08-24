use super::{
    ProviderScheduler, QUARANTINE_RELEASE_SECS, QUEUED_LIVENESS_TIMEOUT_SECS, SchedulerError,
};

/// Durable cleanup performed when a scheduler authority observes leases that
/// can no longer safely make progress. Active units stay counted through the
/// quarantine grace window: a replacement is unsafe until the old provider
/// future has been proven stopped. Once a quarantined lease has gone
/// `QUARANTINE_RELEASE_SECS` without a renewal, its fence is revoked
/// (terminalized) so every later `renew`/`complete`/`fail` from the old holder
/// is rejected with `StaleFence`, and only then do its units return to the
/// domain. A live lease can never be terminalized: renewing clears quarantine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reconciliation {
    pub expired_queued: u64,
    pub expired_grants: u64,
    pub quarantined_active: u64,
    pub released_quarantined: u64,
}

impl ProviderScheduler {
    pub async fn reconcile(&self) -> Result<Reconciliation, SchedulerError> {
        let _write_permit = self.write_gate.lock().await;
        let expired_queued = sqlx::query(
            "UPDATE provider_reservations SET status = 'expired', granted_units = 0,
             terminal_reason = 'abandoned_waiter', updated_at = datetime('now')
             WHERE capacity_domain = ? AND instance_id = ? AND authority_id = ?
               AND status = 'queued'
               AND unixepoch(COALESCE(renewed_at, updated_at)) <= unixepoch('now') - ?",
        )
        .bind(domain_name(self.domain.kind)?)
        .bind(&self.domain.instance_id)
        .bind(&self.domain.authority_id)
        .bind(QUEUED_LIVENESS_TIMEOUT_SECS)
        .execute(&self.pool)
        .await?
        .rows_affected();
        let expired_grants = sqlx::query(
            "UPDATE provider_reservations SET status = 'canceled', granted_units = 0,
             terminal_reason = 'grant_expired', updated_at = datetime('now')
             WHERE capacity_domain = ? AND instance_id = ?
               AND status = 'granted' AND grant_deadline <= datetime('now')",
        )
        .bind(domain_name(self.domain.kind)?)
        .bind(&self.domain.instance_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        let quarantined_active = sqlx::query(
            "UPDATE provider_reservations SET quarantined = 1,
             terminal_reason = 'active_lease_uncertain', updated_at = datetime('now')
             WHERE capacity_domain = ? AND instance_id = ? AND authority_id = ?
               AND status = 'active' AND quarantined = 0
               AND (expires_at <= datetime('now')
                    OR renewed_at <= datetime('now', '-60 seconds'))",
        )
        .bind(domain_name(self.domain.kind)?)
        .bind(&self.domain.instance_id)
        .bind(&self.domain.authority_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        // Terminalize quarantined leases whose renewals stopped long enough
        // ago that the holder is provably gone (dropped future, crashed
        // process, or a restart with the same authority). This is what makes
        // recovery authoritative: without it, orphaned active rows would hold
        // their granted units forever, since the capacity sum counts every
        // 'active' row.
        let released_quarantined = sqlx::query(
            "UPDATE provider_reservations SET status = 'expired', granted_units = 0,
             terminal_reason = 'quarantine_expired', updated_at = datetime('now')
             WHERE capacity_domain = ? AND instance_id = ? AND authority_id = ?
               AND status = 'active' AND quarantined = 1
               AND unixepoch(COALESCE(renewed_at, updated_at)) <= unixepoch('now') - ?",
        )
        .bind(domain_name(self.domain.kind)?)
        .bind(&self.domain.instance_id)
        .bind(&self.domain.authority_id)
        .bind(QUARANTINE_RELEASE_SECS)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(Reconciliation {
            expired_queued,
            expired_grants,
            quarantined_active,
            released_quarantined,
        })
    }
}

fn domain_name(kind: axon_api::source::ProviderKind) -> Result<String, SchedulerError> {
    Ok(super::domain_name(kind)?)
}
