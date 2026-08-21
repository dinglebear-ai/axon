//! Reservation queue grant and capacity operations.

use super::*;

impl ProviderScheduler {
    pub(super) async fn reserve_locked(
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

    pub(super) async fn try_grant_existing(
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
             WHERE capacity_domain = ? AND instance_id = ? AND status = 'queued'
               AND COALESCE(effective_priority, '') <> CASE max(0,
                    CASE requested_priority
                      WHEN 'interactive' THEN 0 WHEN 'high' THEN 1 WHEN 'normal' THEN 2
                      WHEN 'background' THEN 3 ELSE 4 END
                    - min(4, max(0, (unixepoch('now') - unixepoch(updated_at)) / ?)))
                  WHEN 0 THEN 'interactive' WHEN 1 THEN 'high' WHEN 2 THEN 'normal'
                  WHEN 3 THEN 'background' ELSE 'maintenance' END",
        )
        .bind(AGING_QUANTUM_SECS)
        .bind(domain)
        .bind(&self.domain.instance_id)
        .bind(AGING_QUANTUM_SECS)
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
        let (entries, job_entries, requested_units): (i64, i64, i64) = sqlx::query_as(
            "SELECT
               (SELECT COUNT(*) FROM provider_reservations
                WHERE capacity_domain = ?1 AND instance_id = ?2
                  AND status IN ('queued','granted','active')),
               (SELECT COUNT(*) FROM provider_reservations
                WHERE job_id = ?3 AND status IN ('queued','granted','active')),
               (SELECT COALESCE(SUM(requested_units), 0) FROM provider_reservations
                WHERE capacity_domain = ?1 AND instance_id = ?2
                  AND status IN ('queued','granted','active'))",
        )
        .bind(domain)
        .bind(&self.domain.instance_id)
        .bind(request.job_id.0.to_string())
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
        let priority = enum_name(request.priority)?;
        let kind = enum_name(self.domain.kind)?;
        sqlx::query(
            "INSERT INTO provider_reservations
             (reservation_id, job_id, stage_id, provider_kind, provider_id, priority,
              requested_units, granted_units, status, updated_at, capacity_domain,
              instance_id, authority_id, enqueue_sequence, requested_priority,
              effective_priority, attempt, fence)
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, 'queued', datetime('now'), ?, ?, ?,
               (SELECT COALESCE(MAX(enqueue_sequence), 0) + 1 FROM provider_reservations
                WHERE capacity_domain = ? AND instance_id = ?), ?, ?, ?, ?)",
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
        .bind(domain)
        .bind(&self.domain.instance_id)
        .bind(&priority)
        .bind(&priority)
        .bind(i64::from(request.attempt))
        .bind(&request.fence)
        .execute(&mut **connection)
        .await?;
        Ok(id)
    }
}
