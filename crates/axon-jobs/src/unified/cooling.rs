use axon_api::source::{ApiError, ErrorStage, JobId, LifecycleStatus};
use axon_core::sqlite::ImmediateTx;
use axon_error::cooling::ProviderCooling;
use sqlx::Row;

use super::{MAX_PROVIDER_COOLDOWN_WINDOW, SqliteUnifiedJobStore, retry_job_write};
use crate::boundary::Result;
use crate::unified_codec::{missing_job, parse_enum, sql_error};

impl SqliteUnifiedJobStore {
    #[allow(dead_code)]
    pub(crate) async fn apply_provider_cooling(
        &self,
        job_id: JobId,
        cooling: ProviderCooling,
    ) -> Result<()> {
        retry_job_write("job provider cooling", || {
            self.apply_provider_cooling_once(job_id, cooling.clone())
        })
        .await
    }

    async fn apply_provider_cooling_once(
        &self,
        job_id: JobId,
        cooling: ProviderCooling,
    ) -> Result<()> {
        let mut tx = ImmediateTx::begin(&self.pool).await.map_err(sql_error)?;
        let row = sqlx::query("SELECT status FROM jobs WHERE job_id = ?")
            .bind(job_id.0.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(sql_error)?
            .ok_or_else(|| missing_job(job_id))?;
        let current = parse_enum::<LifecycleStatus>(row.get::<String, _>("status"))?;
        #[cfg(test)]
        super::snapshot_test_hook::pause_once_after_read(job_id).await;
        if current != LifecycleStatus::Waiting {
            return Err(ApiError::new(
                "job_cooling.not_waiting",
                ErrorStage::Publishing,
                format!("job {} is {current:?}, not Waiting", job_id.0),
            ));
        }
        let max_deadline = chrono::Utc::now()
            + chrono::Duration::from_std(MAX_PROVIDER_COOLDOWN_WINDOW)
                .unwrap_or(chrono::Duration::hours(1));
        let result = sqlx::query(
            "UPDATE jobs SET cooldown_until = ? WHERE job_id = ? AND status = 'waiting'",
        )
        .bind(cooling.cooldown_until.min(max_deadline).to_rfc3339())
        .bind(job_id.0.to_string())
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
        if result.rows_affected() == 0 {
            return Err(ApiError::new(
                "job_cooling.not_waiting",
                ErrorStage::Publishing,
                format!("job {} left Waiting before cooling was applied", job_id.0),
            ));
        }
        tx.commit().await.map_err(sql_error)
    }
}
