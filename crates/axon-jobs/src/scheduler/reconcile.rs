use super::*;

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
        let mut connection = self.pool.acquire().await?;
        begin_immediate(&mut connection).await?;
        let domain = domain_name(self.domain.kind)?;
        let result = async {
            let expired_queued = self
                .expire_abandoned_queued_locked(&mut connection, &domain)
                .await?;
            let mut result = self
                .reclaim_capacity_affecting_rows_locked(&mut connection, &domain)
                .await?;
            result.expired_queued = expired_queued;
            Ok::<_, SchedulerError>(result)
        }
        .await;
        match result {
            Ok(result) => {
                sqlx::query("COMMIT").execute(&mut *connection).await?;
                if result.expired_queued > 0 {
                    tracing::info!(
                        expired_queued = result.expired_queued,
                        "provider scheduler expired abandoned waiters"
                    );
                }
                Ok(result)
            }
            Err(error) => Err(rollback_after_error(&mut connection, error).await),
        }
    }
}
