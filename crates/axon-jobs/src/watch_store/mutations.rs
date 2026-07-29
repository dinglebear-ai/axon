use axon_api::source::*;
use sqlx::Row;
use std::future::Future;

use super::SqliteWatchStore;
use super::rows::{
    json_err, missing_watch, scope_to_str, sqlite_err, validate_source_watch_interval,
};
use crate::boundary::{Result, WatchStore};
use crate::store::now_ms;

pub(super) async fn retry_watch_write<T, F, Fut>(
    operation: &'static str,
    operation_fn: F,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    axon_core::sqlite::retry_on(
        operation,
        |error: &ApiError| axon_core::sqlite::message_is_retryable_busy(&error.to_string()),
        operation_fn,
    )
    .await
}

impl SqliteWatchStore {
    pub(super) async fn delete_once(&self, watch_id: &WatchId) -> Result<bool> {
        let deleted = sqlx::query("DELETE FROM axon_source_watches WHERE watch_id = ?")
            .bind(&watch_id.0)
            .execute(&self.pool)
            .await
            .map_err(sqlite_err)?
            .rows_affected();
        Ok(deleted > 0)
    }

    pub(super) async fn reset_once(&self) -> Result<()> {
        sqlx::query("DELETE FROM axon_source_watches")
            .execute(&self.pool)
            .await
            .map_err(sqlite_err)?;
        Ok(())
    }

    pub(super) async fn record_run_once(&self, watch_id: &WatchId, job_id: &JobId) -> Result<()> {
        let mut transaction = self.pool.begin().await.map_err(sqlite_err)?;
        let watch_exists =
            sqlx::query_scalar::<_, i64>("SELECT 1 FROM axon_source_watches WHERE watch_id = ?")
                .bind(&watch_id.0)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(sqlite_err)?
                .is_some();
        if !watch_exists {
            return Err(missing_watch(watch_id));
        }
        let job_row = sqlx::query("SELECT status FROM jobs WHERE job_id = ?")
            .bind(job_id.0.to_string())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(sqlite_err)?
            .ok_or_else(|| super::rows::missing_job(*job_id))?;
        let status: String = job_row.get("status");
        let now = now_ms();
        sqlx::query(
            "INSERT INTO axon_source_watch_runs (watch_id, job_id, created_at) \
             SELECT ?, ?, ? WHERE NOT EXISTS (SELECT 1 FROM axon_source_watch_runs WHERE watch_id = ? AND job_id = ?)",
        )
        .bind(&watch_id.0)
        .bind(job_id.0.to_string())
        .bind(now)
        .bind(&watch_id.0)
        .bind(job_id.0.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(sqlite_err)?;
        sqlx::query(
            "UPDATE axon_source_watches SET last_job_id = ?, last_status = ?, \
             lease_expires_at = NULL, updated_at = ? WHERE watch_id = ?",
        )
        .bind(job_id.0.to_string())
        .bind(&status)
        .bind(now)
        .bind(&watch_id.0)
        .execute(&mut *transaction)
        .await
        .map_err(sqlite_err)?;
        transaction.commit().await.map_err(sqlite_err)
    }
    pub(super) async fn update_once(
        &self,
        watch_id: WatchId,
        request: WatchUpdateRequest,
    ) -> Result<WatchResult> {
        let existing = sqlx::query("SELECT * FROM axon_source_watches WHERE watch_id = ?")
            .bind(&watch_id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(sqlite_err)?
            .ok_or_else(|| missing_watch(&watch_id))?;
        let mut every_seconds: i64 = existing.get("every_seconds");
        let mut cron: Option<String> = existing.get("cron");
        let mut timezone: Option<String> = existing.get("timezone");
        if let Some(schedule) = &request.schedule {
            every_seconds = validate_source_watch_interval(schedule.every_seconds)?;
            cron = schedule.cron.clone();
            timezone = schedule.timezone.clone();
        }
        let enabled: i64 = request
            .enabled
            .map(i64::from)
            .unwrap_or_else(|| existing.get("enabled"));
        let embed: i64 = request
            .embed
            .map(i64::from)
            .unwrap_or_else(|| existing.get("embed"));
        let options_json = request
            .options
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(json_err)?
            .unwrap_or_else(|| existing.get("options_json"));
        let collection = request
            .collection
            .clone()
            .or_else(|| existing.get::<Option<String>, _>("collection"));
        let scope = request
            .scope
            .map(scope_to_str)
            .unwrap_or_else(|| existing.get("scope"));
        let now = now_ms();
        let next_run_at = if request.schedule.is_some() {
            now + every_seconds * 1000
        } else {
            existing.get("next_run_at")
        };
        sqlx::query("UPDATE axon_source_watches SET enabled=?, every_seconds=?, cron=?, timezone=?, embed=?, options_json=?, collection=?, scope=?, next_run_at=?, updated_at=? WHERE watch_id=?")
            .bind(enabled).bind(every_seconds).bind(&cron).bind(&timezone).bind(embed).bind(&options_json)
            .bind(&collection).bind(&scope).bind(next_run_at).bind(now).bind(&watch_id.0)
            .execute(&self.pool).await.map_err(sqlite_err)?;
        self.get(watch_id.clone())
            .await?
            .ok_or_else(|| missing_watch(&watch_id))
    }
}
