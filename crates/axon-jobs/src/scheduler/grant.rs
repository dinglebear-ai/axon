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
        self.reclaim_capacity_affecting_rows_locked(connection, &domain)
            .await?;
        self.ensure_capacity(connection, &domain, &request).await?;
        let id = self.insert_queued(connection, &domain, &request).await?;
        self.grant_head_locked(connection, &domain).await?;
        self.reservation_grant_locked(connection, &id).await
    }

    pub(super) async fn reclaim_capacity_affecting_rows_locked(
        &self,
        connection: &mut PoolConnection<Sqlite>,
        domain: &str,
    ) -> Result<Reconciliation, SchedulerError> {
        let expired_grants = sqlx::query(
            "UPDATE provider_reservations SET status = 'canceled', granted_units = 0,
             terminal_reason = 'grant_expired', updated_at = datetime('now')
             WHERE capacity_domain = ? AND instance_id = ?
               AND status = 'granted' AND grant_deadline <= datetime('now')",
        )
        .bind(domain)
        .bind(&self.domain.instance_id)
        .execute(&mut **connection)
        .await?
        .rows_affected();
        // Admission also advances the two-phase orphan lifecycle so an active
        // reservation abandoned after startup cannot hold capacity forever.
        let quarantined_active = sqlx::query(
            "UPDATE provider_reservations SET quarantined = 1,
             terminal_reason = 'active_lease_uncertain', updated_at = datetime('now')
             WHERE capacity_domain = ? AND instance_id = ? AND authority_id = ?
               AND status = 'active' AND quarantined = 0
               AND (expires_at <= datetime('now') OR renewed_at <= datetime('now', '-60 seconds'))",
        )
        .bind(domain)
        .bind(&self.domain.instance_id)
        .bind(&self.domain.authority_id)
        .execute(&mut **connection)
        .await?
        .rows_affected();
        let released_quarantined = sqlx::query(
            "UPDATE provider_reservations SET status = 'expired', granted_units = 0,
             terminal_reason = 'quarantine_expired', updated_at = datetime('now')
             WHERE capacity_domain = ? AND instance_id = ? AND authority_id = ?
               AND status = 'active' AND quarantined = 1
               AND unixepoch(COALESCE(renewed_at, updated_at)) <= unixepoch('now') - ?",
        )
        .bind(domain)
        .bind(&self.domain.instance_id)
        .bind(&self.domain.authority_id)
        .bind(QUARANTINE_RELEASE_SECS)
        .execute(&mut **connection)
        .await?
        .rows_affected();
        let result = Reconciliation {
            expired_queued: 0,
            expired_grants,
            quarantined_active,
            released_quarantined,
        };
        if result != Reconciliation::default() {
            tracing::info!(
                expired_grants,
                quarantined_active,
                released_quarantined,
                "provider scheduler reclaimed stale capacity"
            );
        }
        Ok(result)
    }

    pub(super) async fn try_grant_existing(
        &self,
        reservation_id: &str,
    ) -> Result<ReservationGrant, SchedulerError> {
        let _write_permit = self.write_gate.lock().await;
        let mut connection = self.pool.acquire().await?;
        begin_immediate(&mut connection).await?;
        let domain = domain_name(self.domain.kind)?;
        let result = async {
            // Record queued-waiter liveness on every poll. `updated_at` is the
            // priority-aging clock, so the poll heartbeat lives in
            // `renewed_at` (unused while queued); abandonment expiry keys off
            // this heartbeat instead of insertion age.
            sqlx::query(
                "UPDATE provider_reservations SET renewed_at = datetime('now')
                 WHERE reservation_id = ? AND authority_id = ? AND status = 'queued'",
            )
            .bind(reservation_id)
            .bind(&self.domain.authority_id)
            .execute(&mut *connection)
            .await?;
            self.reclaim_capacity_affecting_rows_locked(&mut connection, &domain)
                .await?;
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
            Err(error) => Err(rollback_after_error(&mut connection, error).await),
        }
    }

    async fn grant_head_locked(
        &self,
        connection: &mut PoolConnection<Sqlite>,
        domain: &str,
    ) -> Result<(), SchedulerError> {
        self.refresh_effective_priorities(connection, domain)
            .await?;
        let head: Option<(String, i64, String, i64)> = sqlx::query_as(
            "SELECT reservation_id, requested_units, effective_priority, enqueue_sequence
             FROM provider_reservations
             WHERE capacity_domain = ? AND instance_id = ? AND status = 'queued'
             ORDER BY CASE effective_priority
                 WHEN 'interactive' THEN 0 WHEN 'high' THEN 1 WHEN 'normal' THEN 2
                 WHEN 'background' THEN 3 ELSE 4 END,
                 enqueue_sequence, reservation_id
             LIMIT 1",
        )
        .bind(domain)
        .bind(&self.domain.instance_id)
        .fetch_optional(&mut **connection)
        .await?;
        let Some((head_id, head_units, head_priority, head_sequence)) = head else {
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
        let capacity_limit = |priority: &str| {
            if priority != "interactive" && interactive_queued > 0 {
                self.config
                    .capacity
                    .saturating_sub(self.config.interactive_reserve)
            } else {
                self.config.capacity
            }
        };
        let head_fits = active + head_units <= i64::from(capacity_limit(&head_priority));
        let candidate = if head_fits {
            Some((head_id, head_units, head_priority))
        } else {
            // A non-fitting head may be bypassed once to avoid stranding residual
            // capacity. The acquired row is the durable bypass marker: after a
            // later waiter has run, no subsequent waiter can bypass this head,
            // guaranteeing it gets the next opportunity when enough units free.
            let already_bypassed: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                   SELECT 1 FROM provider_reservations
                   WHERE capacity_domain = ? AND instance_id = ?
                     AND enqueue_sequence > ? AND acquired_at IS NOT NULL
                 )",
            )
            .bind(domain)
            .bind(&self.domain.instance_id)
            .bind(head_sequence)
            .fetch_one(&mut **connection)
            .await?;
            if already_bypassed {
                None
            } else {
                let normal_limit = i64::from(capacity_limit("normal")) - active;
                let interactive_limit = i64::from(capacity_limit("interactive")) - active;
                sqlx::query_as(
                    "SELECT reservation_id, requested_units, effective_priority
                     FROM provider_reservations
                     WHERE capacity_domain = ? AND instance_id = ? AND status = 'queued'
                       AND enqueue_sequence > ?
                       AND requested_units <= CASE
                         WHEN effective_priority = 'interactive' THEN ? ELSE ? END
                     ORDER BY CASE effective_priority
                       WHEN 'interactive' THEN 0 WHEN 'high' THEN 1 WHEN 'normal' THEN 2
                       WHEN 'background' THEN 3 ELSE 4 END,
                       enqueue_sequence, reservation_id
                     LIMIT 1",
                )
                .bind(domain)
                .bind(&self.domain.instance_id)
                .bind(head_sequence)
                .bind(interactive_limit)
                .bind(normal_limit)
                .fetch_optional(&mut **connection)
                .await?
            }
        };
        let Some((candidate_id, candidate_units, _candidate_priority)) = candidate else {
            return Ok(());
        };
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
        let invalid_timestamps: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM provider_reservations
             WHERE capacity_domain = ? AND instance_id = ? AND status = 'queued'
               AND unixepoch(updated_at) IS NULL",
        )
        .bind(domain)
        .bind(&self.domain.instance_id)
        .fetch_one(&mut **connection)
        .await?;
        if invalid_timestamps > 0 {
            return Err(SchedulerError::DatabaseState(
                "queued reservation has an invalid aging timestamp",
            ));
        }
        sqlx::query(
            "WITH desired AS (
               SELECT reservation_id,
                 CASE max(0,
                   CASE requested_priority
                     WHEN 'interactive' THEN 0 WHEN 'high' THEN 1 WHEN 'normal' THEN 2
                     WHEN 'background' THEN 3 ELSE 4 END
                   - min(4, max(0, (unixepoch('now') - unixepoch(updated_at)) / ?)))
                 WHEN 0 THEN 'interactive' WHEN 1 THEN 'high' WHEN 2 THEN 'normal'
                 WHEN 3 THEN 'background' ELSE 'maintenance' END AS priority
               FROM provider_reservations
               WHERE capacity_domain = ? AND instance_id = ? AND status = 'queued'
             )
             UPDATE provider_reservations
             SET effective_priority = (
               SELECT priority FROM desired
               WHERE desired.reservation_id = provider_reservations.reservation_id
             )
             WHERE reservation_id IN (
               SELECT desired.reservation_id FROM desired
               WHERE COALESCE(provider_reservations.effective_priority, '') <> desired.priority
             )",
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
            "queued" if granted_units == 0 => Ok(ReservationGrant::Queued {
                reservation_id: reservation_id.to_string(),
            }),
            "granted" | "active" if granted_units == requested_units => {
                let units = u32::try_from(granted_units)
                    .ok()
                    .and_then(NonZeroU32::new)
                    .ok_or(SchedulerError::DatabaseState(
                        "granted reservation has invalid unit accounting",
                    ))?;
                Ok(ReservationGrant::Granted {
                    reservation_id: reservation_id.to_string(),
                    units,
                })
            }
            "queued" | "granted" | "active" => Err(SchedulerError::DatabaseState(
                "reservation status and unit accounting disagree",
            )),
            "released" | "canceled" | "expired" | "failed" => Err(SchedulerError::StaleFence),
            _ => Err(SchedulerError::DatabaseState(
                "reservation has an unknown persisted status",
            )),
        }
    }

    pub(super) async fn expire_abandoned_queued_locked(
        &self,
        connection: &mut PoolConnection<Sqlite>,
        domain: &str,
    ) -> Result<u64, SchedulerError> {
        // Abandonment means "no grant poll recently" (see
        // `QUEUED_LIVENESS_TIMEOUT_SECS`), never "queued for a while": a live
        // waiter refreshes `renewed_at` on every poll while `updated_at`
        // stays at insert time so priority aging keeps progressing.
        Ok(sqlx::query(
            "UPDATE provider_reservations SET status = 'expired', granted_units = 0,
             terminal_reason = 'abandoned_waiter', updated_at = datetime('now')
             WHERE capacity_domain = ? AND instance_id = ? AND authority_id = ?
               AND status = 'queued'
               AND unixepoch(COALESCE(renewed_at, updated_at)) <= unixepoch('now') - ?",
        )
        .bind(domain)
        .bind(&self.domain.instance_id)
        .bind(&self.domain.authority_id)
        .bind(QUEUED_LIVENESS_TIMEOUT_SECS)
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
