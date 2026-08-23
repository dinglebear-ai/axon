use super::{ProviderScheduler, SchedulerError};

/// Durable cleanup performed when a scheduler authority observes leases that
/// can no longer safely make progress. Active units intentionally remain
/// counted after quarantine: a replacement is unsafe until the old provider
/// future has been proven stopped and its lease is explicitly cancelled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reconciliation {
    pub expired_queued: u64,
    pub expired_grants: u64,
    pub quarantined_active: u64,
}

impl ProviderScheduler {
    pub async fn reconcile(&self) -> Result<Reconciliation, SchedulerError> {
        let _write_permit = self.write_gate.lock().await;
        let expired_queued = sqlx::query(
            "UPDATE provider_reservations SET status = 'expired', granted_units = 0,
             terminal_reason = 'abandoned_waiter', updated_at = datetime('now')
             WHERE capacity_domain = ? AND instance_id = ? AND authority_id = ?
               AND status = 'queued' AND updated_at <= datetime('now', '-30 seconds')",
        )
        .bind(domain_name(self.domain.kind)?)
        .bind(&self.domain.instance_id)
        .bind(&self.domain.authority_id)
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
        Ok(Reconciliation {
            expired_queued,
            expired_grants,
            quarantined_active,
        })
    }
}

fn domain_name(kind: axon_api::source::ProviderKind) -> Result<String, SchedulerError> {
    Ok(super::domain_name(kind)?)
}
