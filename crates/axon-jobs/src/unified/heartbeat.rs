use axon_api::source::*;
use sqlx::Row;

use super::SqliteUnifiedJobStore;
use crate::boundary::Result;
use crate::state_machine::validate_transition;
use crate::unified_codec::*;

impl SqliteUnifiedJobStore {
    pub(crate) async fn record_heartbeat(&self, heartbeat: JobHeartbeat) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        let row = sqlx::query("SELECT status, attempt FROM jobs WHERE job_id = ?")
            .bind(heartbeat.job_id.0.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(sql_error)?
            .ok_or_else(|| missing_job(heartbeat.job_id))?;
        let current = parse_enum::<LifecycleStatus>(row.get::<String, _>("status"))?;
        let current_attempt = row.get::<i64, _>("attempt") as u32;
        if heartbeat.attempt < current_attempt {
            return Err(ApiError::new(
                "job.heartbeat_stale_attempt",
                ErrorStage::Publishing,
                format!(
                    "job {} is on attempt {}, got stale heartbeat for attempt {}",
                    heartbeat.job_id.0, current_attempt, heartbeat.attempt
                ),
            ));
        }
        if current != heartbeat.status {
            validate_transition(heartbeat.job_id, current, heartbeat.status)?;
        }
        self.update_heartbeat_summary(&mut tx, &heartbeat, current, current_attempt)
            .await?;
        self.upsert_heartbeat_history(&mut tx, &heartbeat).await?;
        self.upsert_attempt_from_heartbeat(&mut tx, &heartbeat)
            .await?;
        self.upsert_provider_reservations(&mut tx, &heartbeat)
            .await?;
        tx.commit().await.map_err(sql_error)?;

        // Supplement: mirror the heartbeat into the durable observability sink
        // after the authoritative write commits. Sink errors are logged, not
        // propagated.
        self.observe_heartbeat(&heartbeat).await;
        Ok(())
    }

    async fn update_heartbeat_summary(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        heartbeat: &JobHeartbeat,
        expected_status: LifecycleStatus,
        expected_attempt: u32,
    ) -> Result<()> {
        // cooldown_until: a Waiting job legally transitions to Failed/Expired
        // (and others) via a heartbeat update, and cooldown is only ever
        // meaningful while a job is Waiting — clear it on every transition
        // away from Waiting, same as update_job_status's CASE-based clear.
        // Left untouched when the new status IS Waiting so a heartbeat that
        // re-affirms Waiting does not wipe out a cooldown set separately via
        // `apply_provider_cooling`.
        let status_name = enum_name(heartbeat.status)?;
        let result = sqlx::query(
            "UPDATE jobs SET
                status = ?,
                phase = ?,
                attempt = ?,
                counts_json = ?,
                heartbeat_json = ?,
                updated_at = ?,
                started_at = COALESCE(started_at, ?),
                finished_at = COALESCE(?, finished_at),
                cooldown_until = CASE WHEN ? = 'waiting' THEN cooldown_until ELSE NULL END
             WHERE job_id = ? AND status = ? AND attempt <= ?",
        )
        .bind(status_name.as_str())
        .bind(enum_name(heartbeat.phase)?)
        .bind(heartbeat.attempt as i64)
        .bind(optional_to_json(&heartbeat.counts)?)
        .bind(to_json(heartbeat)?)
        .bind(heartbeat.heartbeat_at.0.as_str())
        .bind(
            (heartbeat.status == LifecycleStatus::Running)
                .then_some(heartbeat.heartbeat_at.0.as_str()),
        )
        .bind(is_terminal(heartbeat.status).then_some(heartbeat.heartbeat_at.0.as_str()))
        .bind(status_name.as_str())
        .bind(heartbeat.job_id.0.to_string())
        .bind(enum_name(expected_status)?)
        .bind(expected_attempt as i64)
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
        if result.rows_affected() == 0 {
            return Err(ApiError::new(
                "job.concurrent_status_change",
                ErrorStage::Publishing,
                format!(
                    "job {} changed status or attempt before heartbeat could be recorded",
                    heartbeat.job_id.0
                ),
            ));
        }
        Ok(())
    }

    async fn upsert_heartbeat_history(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        heartbeat: &JobHeartbeat,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO job_heartbeats (
                job_id, attempt, heartbeat_at, heartbeat_json
            ) VALUES (?, ?, ?, ?)
            ON CONFLICT(job_id, attempt) DO UPDATE SET
                heartbeat_at = excluded.heartbeat_at,
                heartbeat_json = excluded.heartbeat_json",
        )
        .bind(heartbeat.job_id.0.to_string())
        .bind(heartbeat.attempt as i64)
        .bind(heartbeat.heartbeat_at.0.as_str())
        .bind(to_json(heartbeat)?)
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
        Ok(())
    }

    async fn upsert_attempt_from_heartbeat(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        heartbeat: &JobHeartbeat,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO job_attempts (
                attempt_id, job_id, attempt, status, worker_id, started_at, finished_at,
                heartbeat_at, error_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL)
            ON CONFLICT(job_id, attempt) DO UPDATE SET
                status = excluded.status,
                worker_id = COALESCE(excluded.worker_id, job_attempts.worker_id),
                heartbeat_at = excluded.heartbeat_at,
                finished_at = COALESCE(excluded.finished_at, job_attempts.finished_at)",
        )
        .bind(attempt_id(heartbeat.job_id, heartbeat.attempt))
        .bind(heartbeat.job_id.0.to_string())
        .bind(heartbeat.attempt as i64)
        .bind(enum_name(heartbeat.status)?)
        .bind(heartbeat.worker_id.as_deref())
        .bind(heartbeat.heartbeat_at.0.as_str())
        .bind(is_terminal(heartbeat.status).then_some(heartbeat.heartbeat_at.0.as_str()))
        .bind(heartbeat.heartbeat_at.0.as_str())
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
        Ok(())
    }

    async fn upsert_provider_reservations(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        heartbeat: &JobHeartbeat,
    ) -> Result<()> {
        for reservation in &heartbeat.provider_reservations {
            sqlx::query(
                "INSERT INTO provider_reservations (
                    reservation_id, job_id, stage_id, provider_kind, provider_id,
                    priority, requested_units, granted_units, acquired_at, expires_at,
                    status, queue_depth, cooling_json, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(reservation_id) DO UPDATE SET
                    job_id = excluded.job_id,
                    stage_id = excluded.stage_id,
                    provider_kind = excluded.provider_kind,
                    provider_id = excluded.provider_id,
                    priority = excluded.priority,
                    requested_units = excluded.requested_units,
                    granted_units = excluded.granted_units,
                    acquired_at = excluded.acquired_at,
                    expires_at = excluded.expires_at,
                    status = excluded.status,
                    queue_depth = excluded.queue_depth,
                    cooling_json = excluded.cooling_json,
                    updated_at = excluded.updated_at",
            )
            .bind(reservation.reservation_id.0.as_str())
            .bind(heartbeat.job_id.0.to_string())
            .bind(heartbeat.stage_id.map(|id| id.0.to_string()))
            .bind(enum_name(reservation.provider_kind)?)
            .bind(reservation.provider_id.as_ref().map(|id| id.0.as_str()))
            .bind(enum_name(reservation.priority)?)
            .bind(reservation.requested_units as i64)
            .bind(reservation.granted_units as i64)
            .bind(reservation.acquired_at.as_ref().map(|ts| ts.0.as_str()))
            .bind(reservation.expires_at.as_ref().map(|ts| ts.0.as_str()))
            .bind(enum_name(reservation.status)?)
            .bind(reservation.queue_depth.map(|depth| depth as i64))
            .bind(optional_to_json(&reservation.cooling)?)
            .bind(heartbeat.heartbeat_at.0.as_str())
            .execute(&mut **tx)
            .await
            .map_err(sql_error)?;
        }
        Ok(())
    }
}
