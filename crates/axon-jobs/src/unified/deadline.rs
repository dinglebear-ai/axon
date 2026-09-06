use axon_api::source::{ApiError, ErrorStage, JobId, LifecycleStatus};
use axon_core::sqlite::ImmediateTx;
use sqlx::Row;

use super::SqliteUnifiedJobStore;
use super::control_helpers::terminalize_active_children;
use crate::boundary::Result;
use crate::unified_codec::{now_timestamp, optional_to_json, sql_error};

impl SqliteUnifiedJobStore {
    /// Expire every active job past its durable execution deadline, including
    /// all owned child rows, before canceling the matching local runner.
    pub(crate) async fn expire_past_deadline_jobs(&self) -> Result<u64> {
        let now = now_timestamp();
        let mut tx = ImmediateTx::begin(&self.pool).await.map_err(sql_error)?;
        let expired = sqlx::query(
            "SELECT job_id, attempt FROM jobs
             WHERE status IN ('running', 'waiting', 'canceling')
               AND deadline_at IS NOT NULL AND deadline_at < ?",
        )
        .bind(now.0.as_str())
        .fetch_all(&mut *tx)
        .await
        .map_err(sql_error)?;
        let error = ApiError::new(
            "job.deadline_expired",
            ErrorStage::Planning,
            "job exceeded its configured execution deadline",
        );
        let mut transitioned = Vec::with_capacity(expired.len());
        for row in expired {
            let job_id = JobId(
                row.try_get::<String, _>("job_id")
                    .map_err(sql_error)?
                    .parse()
                    .map_err(|parse_error| {
                        ApiError::new(
                            "job.invalid_id",
                            ErrorStage::Planning,
                            format!("invalid persisted job id: {parse_error}"),
                        )
                    })?,
            );
            let attempt = row.try_get::<i64, _>("attempt").map_err(sql_error)? as u32;
            let result = sqlx::query(
                "UPDATE jobs SET status = 'expired', phase = 'canceled', updated_at = ?,
                    finished_at = ?, cooldown_until = NULL, last_error_json = ?
                 WHERE job_id = ? AND attempt = ?
                   AND status IN ('running', 'waiting', 'canceling')",
            )
            .bind(now.0.as_str())
            .bind(now.0.as_str())
            .bind(optional_to_json(&Some(error.clone()))?)
            .bind(job_id.0.to_string())
            .bind(attempt as i64)
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
            if result.rows_affected() == 1 {
                terminalize_active_children(
                    &mut tx,
                    job_id,
                    LifecycleStatus::Expired,
                    &now,
                    Some(error.clone()),
                )
                .await?;
                transitioned.push(job_id);
            }
        }
        tx.commit().await.map_err(sql_error)?;
        for job_id in &transitioned {
            crate::workers::cancel_job(*job_id);
        }
        Ok(transitioned.len() as u64)
    }
}
