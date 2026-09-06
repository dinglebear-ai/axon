use axon_api::source::*;
use sqlx::Row;
use uuid::Uuid;

use super::SqliteUnifiedJobStore;
use super::ops_helpers::{append_job_filters, bind_job_filters};
use super::terminal_warnings::collect_terminal_warnings;
use crate::boundary::Result;
use crate::limits::clamp_page_limit;
use crate::state_machine::{validate_stage_plan, validate_transition};
use crate::unified::pagination::{JobCursor, decode_job_cursor, encode_job_cursor};
use crate::unified_codec::*;
use axon_core::sqlite::ImmediateTx;

impl SqliteUnifiedJobStore {
    pub(crate) async fn create_job(&self, request: JobCreateRequest) -> Result<JobDescriptor> {
        self.create_job_transaction(request, None, None).await
    }

    pub(crate) async fn create_job_with_snapshot(
        &self,
        request: JobCreateRequest,
        config_json: Option<&str>,
    ) -> Result<JobDescriptor> {
        self.create_job_transaction(request, config_json, None)
            .await
    }

    pub(crate) async fn create_watch_run_atomic(
        &self,
        request: JobCreateRequest,
        watch_id: &WatchId,
    ) -> Result<JobDescriptor> {
        self.create_job_transaction(request, None, Some(watch_id))
            .await
    }

    async fn create_job_transaction(
        &self,
        request: JobCreateRequest,
        config_json: Option<&str>,
        watch_run: Option<&WatchId>,
    ) -> Result<JobDescriptor> {
        validate_stage_plan(&request.stage_plan)?;
        validate_snapshot(&request, config_json)?;
        if let Some(idempotency_key) = request.idempotency_key.as_deref()
            && let Some(summary) = find_by_idempotency_key(&self.pool, idempotency_key).await?
        {
            if let Some(watch_id) = watch_run {
                self.link_watch_run_existing(watch_id, summary.job_id, summary.status)
                    .await?;
            }
            return Ok(descriptor(&summary));
        }

        let job_id = JobId::new(Uuid::new_v4());
        let root_job_id = request.root_job_id.unwrap_or(job_id);
        let now = now_timestamp();
        let request_json = request.request.clone();
        let mut tx = ImmediateTx::begin(&self.pool).await.map_err(sql_error)?;
        insert_config_snapshot(&mut tx, &request, config_json, now.0.as_str()).await?;
        sqlx::query(
            "INSERT INTO jobs (
                job_id, kind, intent, status, phase, priority, source_id, watch_id,
                parent_job_id, root_job_id, attempt, warnings_json, request_json,
                metadata_json, idempotency_key, auth_snapshot_json, config_snapshot_id,
                stage_plan_json, requirements_json, result_schema, error_json,
                created_at, updated_at, deadline_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(job_id.0.to_string())
        .bind(enum_name(request.job_kind)?)
        .bind(enum_name(request.job_intent)?)
        .bind(enum_name(LifecycleStatus::Queued)?)
        .bind(enum_name(PipelinePhase::Queued)?)
        .bind(enum_name(request.priority)?)
        .bind(request.source_id.as_ref().map(|id| id.0.as_str()))
        .bind(request.watch_id.as_ref().map(|id| id.0.as_str()))
        .bind(request.parent_job_id.map(|id| id.0.to_string()))
        .bind(root_job_id.0.to_string())
        .bind(request.attempt as i64)
        .bind(to_json(&request.warnings)?)
        .bind(optional_to_json(&request_json)?)
        .bind(to_json(&request.metadata)?)
        .bind(request.idempotency_key.as_deref())
        .bind(to_json(&request.auth_snapshot)?)
        .bind(
            request
                .config_snapshot_id
                .as_ref()
                .map(|id| id.0.as_str())
                .unwrap_or(""),
        )
        .bind(to_json(&request.stage_plan)?)
        .bind(to_json(&request.requirements)?)
        .bind(request.result_schema.as_deref().unwrap_or(""))
        .bind(optional_to_json(&request.error)?)
        .bind(now.0.as_str())
        .bind(now.0.as_str())
        .bind(request.deadline_at.as_ref().map(|ts| ts.0.as_str()))
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;

        for (ordinal, stage) in request.stage_plan.into_iter().enumerate() {
            let stage_id = stage.stable_id(job_id, ordinal);
            sqlx::query(
                "INSERT INTO job_stages (
                    stage_id, job_id, phase, status, required, provider_requirements_json
                ) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(stage_id.0.to_string())
            .bind(job_id.0.to_string())
            .bind(enum_name(stage.phase)?)
            .bind(enum_name(LifecycleStatus::Queued)?)
            .bind(if stage.required { 1_i64 } else { 0_i64 })
            .bind(to_json(&stage.provider_requirements)?)
            .execute(&mut *tx)
            .await
            .map_err(sql_error)?;
        }

        link_new_watch_run(&mut tx, watch_run, job_id).await?;

        tx.commit().await.map_err(sql_error)?;
        Ok(new_job_descriptor(job_id, request.job_kind, now))
    }

    async fn link_watch_run_existing(
        &self,
        watch_id: &WatchId,
        job_id: JobId,
        status: LifecycleStatus,
    ) -> Result<()> {
        let mut tx = ImmediateTx::begin(&self.pool).await.map_err(sql_error)?;
        let now = crate::store::now_ms();
        sqlx::query(
            "INSERT INTO axon_source_watch_runs (watch_id, job_id, created_at) \
             SELECT ?, ?, ? WHERE NOT EXISTS (SELECT 1 FROM axon_source_watch_runs WHERE watch_id = ? AND job_id = ?)",
        )
        .bind(&watch_id.0)
        .bind(job_id.0.to_string())
        .bind(now)
        .bind(&watch_id.0)
        .bind(job_id.0.to_string())
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
        sqlx::query(
            "UPDATE axon_source_watches SET last_job_id = ?, last_status = ?, lease_expires_at = NULL, updated_at = ? WHERE watch_id = ?",
        )
        .bind(job_id.0.to_string())
        .bind(enum_name(status)?)
        .bind(now)
        .bind(&watch_id.0)
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
        tx.commit().await.map_err(sql_error)
    }

    pub(crate) async fn get_job(&self, job_id: JobId) -> Result<Option<JobSummary>> {
        let row = sqlx::query("SELECT * FROM jobs WHERE job_id = ?")
            .bind(job_id.0.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(sql_error)?;
        row.map(row_to_summary).transpose()
    }

    pub(crate) async fn job_attempts(&self, job_id: JobId) -> Result<Vec<JobAttemptSnapshot>> {
        ensure_job_pool(&self.pool, job_id).await?;
        let rows = sqlx::query("SELECT * FROM job_attempts WHERE job_id = ? ORDER BY attempt ASC")
            .bind(job_id.0.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(sql_error)?;
        rows.into_iter().map(row_to_attempt).collect()
    }

    pub(crate) async fn job_stages(&self, job_id: JobId) -> Result<Vec<JobStageSnapshot>> {
        ensure_job_pool(&self.pool, job_id).await?;
        let rows = sqlx::query("SELECT * FROM job_stages WHERE job_id = ? ORDER BY rowid ASC")
            .bind(job_id.0.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(sql_error)?;
        rows.into_iter().map(row_to_stage).collect()
    }

    pub(crate) async fn update_job_status(&self, status: JobStatusUpdate) -> Result<()> {
        let mut tx = ImmediateTx::begin(&self.pool).await.map_err(sql_error)?;
        let row = sqlx::query(
            "SELECT status, started_at, warnings_json, attempt FROM jobs WHERE job_id = ?",
        )
        .bind(status.job_id.0.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(sql_error)?
        .ok_or_else(|| missing_job(status.job_id))?;
        let current = parse_enum::<LifecycleStatus>(row.get::<String, _>("status"))?;
        let current_attempt = row.get::<i64, _>("attempt") as u32;
        #[cfg(test)]
        super::snapshot_test_hook::pause_once_after_read(status.job_id).await;
        validate_transition(status.job_id, current, status.status)?;
        let now = now_timestamp();
        let job_started_at = (row.get::<Option<String>, _>("started_at").is_none()
            && status.status == LifecycleStatus::Running)
            .then(|| now.0.clone());
        let stage_started_at = (status.status == LifecycleStatus::Running).then(|| now.0.clone());
        let finished_at = is_terminal(status.status).then(|| now.0.clone());
        let terminal_warnings = if is_terminal(status.status) {
            Some(
                collect_terminal_warnings(
                    &mut tx,
                    status.job_id,
                    row.get::<String, _>("warnings_json"),
                )
                .await?,
            )
        } else {
            None
        };

        // cooldown_until: cleared on every transition to a non-Waiting status
        // (a job that cooled once and later runs/completes/fails must not
        // retain a stale cooldown that silently blocks its next legitimate
        // claim). Left untouched when the new status IS Waiting so a
        // heartbeat/update that re-affirms Waiting does not wipe out a
        // cooldown set separately via `apply_provider_cooling`.
        let status_name = enum_name(status.status)?;
        sqlx::query(
            "UPDATE jobs SET
                source_id = COALESCE(?, source_id),
                status = ?, phase = ?, counts_json = ?, current_json = ?,
                last_error_json = ?, warnings_json = COALESCE(?, warnings_json), updated_at = ?,
                started_at = COALESCE(started_at, ?),
                finished_at = COALESCE(?, finished_at),
                cooldown_until = CASE WHEN ? = 'waiting' THEN cooldown_until ELSE NULL END
             WHERE job_id = ?",
        )
        .bind(
            status
                .source_id
                .as_ref()
                .map(|source_id| source_id.0.as_str()),
        )
        .bind(status_name.as_str())
        .bind(enum_name(status.phase)?)
        .bind(optional_to_json(&status.counts)?)
        .bind(optional_to_json(&status.current)?)
        .bind(optional_to_json(&status.error)?)
        .bind(terminal_warnings.as_deref())
        .bind(now.0.as_str())
        .bind(job_started_at.as_deref())
        .bind(finished_at.as_deref())
        .bind(status_name.as_str())
        .bind(status.job_id.0.to_string())
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;

        if let Some(stage_id) = status.stage_id {
            self.update_stage_status(&mut tx, &status, stage_id, &stage_started_at, &finished_at)
                .await?;
        }
        tx.commit().await.map_err(sql_error)?;

        // Supplement: record this transition durably in the observability sink
        // (strictly-increasing per-job sequence + heartbeat). Runs after the
        // authoritative status write commits; sink errors are logged, not
        // propagated, so the observe stream never fails the status update.
        self.observe_status(&status, current_attempt).await;
        Ok(())
    }

    async fn update_stage_status(
        &self,
        tx: &mut sqlx::SqliteConnection,
        status: &JobStatusUpdate,
        stage_id: StageId,
        started_at: &Option<String>,
        finished_at: &Option<String>,
    ) -> Result<()> {
        let result = sqlx::query(
            "UPDATE job_stages SET
                status = ?,
                counts_json = ?,
                started_at = COALESCE(started_at, ?),
                completed_at = COALESCE(?, completed_at),
                error_json = ?
             WHERE stage_id = ? AND job_id = ?",
        )
        .bind(enum_name(status.status)?)
        .bind(optional_to_json(&status.counts)?)
        .bind(started_at.as_deref())
        .bind(finished_at.as_deref())
        .bind(optional_to_json(&status.error)?)
        .bind(stage_id.0.to_string())
        .bind(status.job_id.0.to_string())
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
        if result.rows_affected() == 0 {
            return Err(ApiError::new(
                "job_stage.not_found",
                ErrorStage::Publishing,
                format!("stage {} not found for job {}", stage_id.0, status.job_id.0),
            ));
        }
        Ok(())
    }

    pub(crate) async fn list_jobs(&self, request: JobListRequest) -> Result<Page<JobSummary>> {
        let mut sql = "SELECT * FROM jobs WHERE 1 = 1".to_string();
        let bindings = append_job_filters(&mut sql, &request)?;
        let cursor = request
            .cursor
            .as_deref()
            .map(decode_job_cursor)
            .transpose()
            .map_err(|message| {
                ApiError::new("job.cursor_invalid", ErrorStage::Retrieving, message)
            })?;
        if cursor.is_some() {
            sql.push_str(" AND (updated_at < ? OR (updated_at = ? AND job_id < ?))");
        }
        let total = if cursor.is_none() {
            let total_sql = sql.replacen("SELECT *", "SELECT COUNT(*)", 1);
            let mut total_query = sqlx::query_scalar::<_, i64>(&total_sql);
            if let Some(source_id) = bindings.source_id.as_deref() {
                total_query = total_query.bind(source_id);
            }
            if let Some(watch_id) = bindings.watch_id.as_deref() {
                total_query = total_query.bind(watch_id);
            }
            Some(total_query.fetch_one(&self.pool).await.map_err(sql_error)? as u64)
        } else {
            None
        };
        let limit = clamp_page_limit(request.limit);
        sql.push_str(" ORDER BY updated_at DESC, job_id DESC LIMIT ");
        sql.push_str(&(limit + 1).to_string());
        let mut query = bind_job_filters(sqlx::query(&sql), &bindings);
        if let Some(cursor) = cursor.as_ref() {
            query = query
                .bind(cursor.updated_at.as_str())
                .bind(cursor.updated_at.as_str())
                .bind(cursor.job_id.as_str());
        }
        let rows = query.fetch_all(&self.pool).await.map_err(sql_error)?;
        let mut items = rows
            .into_iter()
            .map(row_to_summary)
            .collect::<Result<Vec<_>>>()?;
        let has_more = items.len() > limit as usize;
        if has_more {
            items.truncate(limit as usize);
        }
        Ok(Page {
            limit,
            total,
            next_cursor: items.last().filter(|_| has_more).map(|job| {
                encode_job_cursor(&JobCursor {
                    updated_at: job.updated_at.0.clone(),
                    job_id: job.job_id.0.to_string(),
                })
            }),
            items,
        })
    }

    pub(crate) async fn latest_sequence(&self, job_id: JobId) -> Result<Option<u64>> {
        ensure_job_pool(&self.pool, job_id).await?;
        let sequence = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(sequence) FROM job_events WHERE job_id = ?",
        )
        .bind(job_id.0.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(sql_error)?
        .map(|sequence| sequence as u64);
        Ok(sequence)
    }
}

fn validate_snapshot(request: &JobCreateRequest, config_json: Option<&str>) -> Result<()> {
    let Some(config_json) = config_json else {
        return Ok(());
    };
    let Some(snapshot_id) = request.config_snapshot_id.as_ref() else {
        return Err(ApiError::new(
            "config_snapshot.missing_id",
            ErrorStage::Publishing,
            "config snapshot material requires a config_snapshot_id",
        ));
    };
    let expected = crate::config_snapshot_store::config_snapshot_id_from_json(config_json);
    if snapshot_id.0 != expected {
        return Err(ApiError::new(
            "config_snapshot.digest_mismatch",
            ErrorStage::Publishing,
            format!(
                "config snapshot id {} does not match its content digest {expected}",
                snapshot_id.0
            ),
        ));
    }
    Ok(())
}

async fn insert_config_snapshot(
    tx: &mut sqlx::SqliteConnection,
    request: &JobCreateRequest,
    config_json: Option<&str>,
    now: &str,
) -> Result<()> {
    let Some(config_json) = config_json else {
        return Ok(());
    };
    let snapshot_id = request
        .config_snapshot_id
        .as_ref()
        .expect("validated snapshot id");
    let inserted = sqlx::query(
        "INSERT INTO config_snapshots (config_snapshot_id, config_json, created_at) VALUES (?, ?, ?) \
         ON CONFLICT(config_snapshot_id) DO NOTHING",
    )
    .bind(snapshot_id.0.as_str())
    .bind(config_json)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(sql_error)?;
    if inserted.rows_affected() == 0 {
        let stored: String = sqlx::query_scalar(
            "SELECT config_json FROM config_snapshots WHERE config_snapshot_id = ?",
        )
        .bind(snapshot_id.0.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(sql_error)?;
        if stored != config_json {
            return Err(ApiError::new(
                "config_snapshot.digest_mismatch",
                ErrorStage::Publishing,
                format!(
                    "config snapshot id {} is already bound to different content",
                    snapshot_id.0
                ),
            ));
        }
    }
    Ok(())
}

async fn link_new_watch_run(
    tx: &mut sqlx::SqliteConnection,
    watch_id: Option<&WatchId>,
    job_id: JobId,
) -> Result<()> {
    let Some(watch_id) = watch_id else {
        return Ok(());
    };
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM axon_source_watches WHERE watch_id = ?")
            .bind(&watch_id.0)
            .fetch_optional(&mut *tx)
            .await
            .map_err(sql_error)?;
    if exists.is_none() {
        return Err(ApiError::new(
            "watch.not_found",
            ErrorStage::Retrieving,
            format!("watch {} not found", watch_id.0),
        ));
    }
    let now = crate::store::now_ms();
    sqlx::query(
        "INSERT INTO axon_source_watch_runs (watch_id, job_id, created_at) VALUES (?, ?, ?)",
    )
    .bind(&watch_id.0)
    .bind(job_id.0.to_string())
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(sql_error)?;
    sqlx::query(
        "UPDATE axon_source_watches SET last_job_id = ?, last_status = 'queued', \
         lease_expires_at = NULL, updated_at = ? WHERE watch_id = ?",
    )
    .bind(job_id.0.to_string())
    .bind(now)
    .bind(&watch_id.0)
    .execute(&mut *tx)
    .await
    .map_err(sql_error)?;
    Ok(())
}
