use axon_api::source::*;
use sqlx::Row;

use super::SqliteUnifiedJobStore;
use super::control_helpers::*;
use crate::boundary::Result;
use crate::limits::clamp_page_limit;
use crate::state_machine::validate_transition;
use crate::unified_codec::*;

impl SqliteUnifiedJobStore {
    pub(crate) async fn cancel_job(
        &self,
        job_id: JobId,
        request: JobCancelRequest,
    ) -> Result<JobCancelResult> {
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        let row = sqlx::query("SELECT status FROM jobs WHERE job_id = ?")
            .bind(job_id.0.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(sql_error)?
            .ok_or_else(|| missing_job(job_id))?;
        let current = parse_enum::<LifecycleStatus>(row.get::<String, _>("status"))?;
        validate_transition(job_id, current, LifecycleStatus::Canceling)?;
        let now = now_timestamp();
        let target = if matches!(current, LifecycleStatus::Queued | LifecycleStatus::Pending)
            || request.force_after_ms == Some(0)
        {
            LifecycleStatus::Canceled
        } else {
            LifecycleStatus::Canceling
        };
        let canceled_at = (target == LifecycleStatus::Canceled).then(|| now.clone());
        sqlx::query(
            "UPDATE jobs SET
                status = ?,
                phase = ?,
                updated_at = ?,
                finished_at = COALESCE(?, finished_at)
             WHERE job_id = ?",
        )
        .bind(enum_name(target)?)
        .bind(enum_name(PipelinePhase::Canceled)?)
        .bind(now.0.as_str())
        .bind(canceled_at.as_ref().map(|ts| ts.0.as_str()))
        .bind(job_id.0.to_string())
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
        if target == LifecycleStatus::Canceled {
            terminalize_active_children(
                &mut tx,
                job_id,
                LifecycleStatus::Canceled,
                &now,
                Some(cancel_api_error(request.reason.as_deref())),
            )
            .await?;
        }
        tx.commit().await.map_err(sql_error)?;
        Ok(JobCancelResult {
            job_id,
            status: target,
            canceled_at,
            reason: request.reason,
        })
    }

    pub(crate) async fn retry_job(
        &self,
        job_id: JobId,
        request: JobRetryRequest,
    ) -> Result<JobRetryResult> {
        let original = self
            .get_job(job_id)
            .await?
            .ok_or_else(|| missing_job(job_id))?;
        if request.mode == JobRetryMode::SameConfig && !request.overrides.is_empty() {
            return Err(ApiError::new(
                "job_retry.overrides_forbidden",
                ErrorStage::Planning,
                "same_config retry cannot include overrides",
            ));
        }
        let row = sqlx::query("SELECT request_json, metadata_json FROM jobs WHERE job_id = ?")
            .bind(job_id.0.to_string())
            .fetch_one(&self.pool)
            .await
            .map_err(sql_error)?;
        let request_json = row.get::<Option<String>, _>("request_json");
        let metadata_json = row.get::<String, _>("metadata_json");
        let mut metadata = from_json::<MetadataMap>(metadata_json)?;
        if request.mode == JobRetryMode::WithOverrides {
            metadata.0.extend(request.overrides.0.clone());
        }
        let mut stage_plan = self
            .job_stages(job_id)
            .await?
            .into_iter()
            .map(|stage| JobStagePlan {
                phase: stage.phase,
                required: stage.required,
                provider_requirements: stage.provider_requirements,
                estimated_items: stage.counts.items_total,
            })
            .collect::<Vec<_>>();
        if let Some(from_phase) = request.from_phase {
            let Some(index) = stage_plan
                .iter()
                .position(|stage| stage.phase == from_phase)
            else {
                return Err(ApiError::new(
                    "job_retry.from_phase_not_found",
                    ErrorStage::Planning,
                    format!("phase {:?} is not present in job {}", from_phase, job_id.0),
                ));
            };
            stage_plan = stage_plan.split_off(index);
        }
        let attempt = original.attempt + 1;
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        reset_job_for_retry(
            &mut tx,
            job_id,
            original.status,
            attempt,
            request.idempotency_key.as_deref(),
            request_json.as_deref(),
            &metadata,
            &stage_plan,
        )
        .await?;
        tx.commit().await.map_err(sql_error)?;
        let retry_job = self
            .get_job(job_id)
            .await?
            .map(|summary| descriptor(&summary))
            .ok_or_else(|| missing_job(job_id))?;
        Ok(JobRetryResult {
            original_job_id: job_id,
            retry_job,
            attempt,
        })
    }

    pub(crate) async fn recover_jobs(
        &self,
        request: JobRecoveryRequest,
    ) -> Result<JobRecoveryResult> {
        if request.older_than_seconds.is_none() && !request.allow_without_cutoff {
            return Err(ApiError::new(
                "job_recovery.cutoff_required",
                ErrorStage::Planning,
                "recovery requires older_than_seconds unless allow_without_cutoff is explicit",
            ));
        }
        let kind_filter = request.kind.map(enum_name).transpose()?;
        let cutoff = request.older_than_seconds.map(|seconds| {
            Timestamp::from(chrono::Utc::now() - chrono::Duration::seconds(seconds as i64))
        });
        let mut sql = "SELECT job_id FROM jobs WHERE status IN ('running', 'waiting')".to_string();
        append_recovery_filter(&mut sql, kind_filter.as_deref(), cutoff.as_ref());
        let mut query = sqlx::query(&sql);
        if let Some(cutoff) = cutoff.as_ref() {
            query = query.bind(cutoff.0.as_str());
        }
        let rows = query.fetch_all(&self.pool).await.map_err(sql_error)?;
        let job_ids = rows
            .iter()
            .map(|row| row.get::<String, _>("job_id"))
            .collect::<Vec<_>>();
        let scanned = rows.len() as u64;
        let failed = if !request.dry_run && scanned > 0 {
            self.fail_recoverable_jobs(&job_ids).await?
        } else {
            0
        };
        Ok(JobRecoveryResult {
            recovered: 0,
            job_ids: Vec::new(),
            warnings: Vec::new(),
            jobs_scanned: scanned,
            jobs_requeued: 0,
            jobs_failed: failed,
        })
    }

    pub(crate) async fn cleanup_jobs(
        &self,
        request: JobCleanupRequest,
    ) -> Result<JobCleanupResult> {
        if request.older_than_seconds.is_none() && !request.confirm_all_terminal {
            return Err(ApiError::new(
                "job_cleanup.cutoff_required",
                ErrorStage::Planning,
                "cleanup requires older_than_seconds unless confirm_all_terminal is explicit",
            ));
        }
        let cutoff = request.older_than_seconds.map(|seconds| {
            Timestamp::from(chrono::Utc::now() - chrono::Duration::seconds(seconds as i64))
        });
        let mut predicate =
            "status IN ('completed', 'completed_degraded', 'failed', 'canceled', 'expired', 'skipped')"
                .to_string();
        if cutoff.is_some() {
            predicate.push_str(" AND updated_at < ?");
        }

        let jobs_pruned = count_with_optional_cutoff(
            &self.pool,
            &format!("SELECT COUNT(*) FROM jobs WHERE {predicate}"),
            cutoff.as_ref(),
        )
        .await?;
        let events_pruned = count_with_optional_cutoff(
            &self.pool,
            &format!("SELECT COUNT(*) FROM job_events WHERE job_id IN (SELECT job_id FROM jobs WHERE {predicate})"),
            cutoff.as_ref(),
        )
        .await?;
        let heartbeats_pruned = count_with_optional_cutoff(
            &self.pool,
            &format!("SELECT COUNT(*) FROM job_heartbeats WHERE job_id IN (SELECT job_id FROM jobs WHERE {predicate})"),
            cutoff.as_ref(),
        )
        .await?;
        let artifacts_pruned = count_with_optional_cutoff(
            &self.pool,
            &format!("SELECT COUNT(*) FROM job_artifacts WHERE job_id IN (SELECT job_id FROM jobs WHERE {predicate})"),
            cutoff.as_ref(),
        )
        .await?;

        let deleted = if !request.dry_run && jobs_pruned > 0 {
            execute_with_optional_cutoff(
                &self.pool,
                &format!("DELETE FROM jobs WHERE {predicate}"),
                cutoff.as_ref(),
            )
            .await?
        } else {
            jobs_pruned
        };
        Ok(JobCleanupResult {
            matched: jobs_pruned,
            deleted,
            dry_run: request.dry_run,
            warnings: Vec::new(),
            jobs_pruned: deleted,
            events_pruned,
            heartbeats_pruned,
            artifacts_pruned,
        })
    }

    pub(crate) async fn list_job_artifacts(
        &self,
        request: JobArtifactListRequest,
    ) -> Result<JobArtifactListResult> {
        ensure_job_pool(&self.pool, request.job_id).await?;
        if request.cursor.is_some() {
            return Err(ApiError::new(
                "job_artifact.cursor_unsupported",
                ErrorStage::Retrieving,
                "sqlite unified job store does not implement artifact cursor pagination yet",
            ));
        }
        let mut sql = "SELECT * FROM job_artifacts WHERE job_id = ?".to_string();
        if let Some(kind) = request.kind {
            sql.push_str(&format!(" AND artifact_kind = '{}'", enum_name(kind)?));
        }
        let limit = clamp_page_limit(request.limit);
        sql.push_str(" ORDER BY created_at DESC LIMIT ");
        sql.push_str(&limit.to_string());
        let rows = sqlx::query(&sql)
            .bind(request.job_id.0.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(sql_error)?;
        let artifacts = rows
            .into_iter()
            .map(row_to_artifact)
            .collect::<Result<Vec<_>>>()?;
        Ok(JobArtifactListResult {
            artifacts,
            limit,
            next_cursor: None,
        })
    }

    pub(crate) async fn reset_jobs(&self) -> Result<()> {
        sqlx::query("DELETE FROM jobs")
            .execute(&self.pool)
            .await
            .map_err(sql_error)?;
        Ok(())
    }

    pub(crate) async fn store_capabilities(&self) -> Result<JobStoreCapability> {
        Ok(CapabilityBase {
            name: "sqlite-unified-job-store".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-jobs".to_string(),
            health: HealthStatus::Healthy,
            features: vec!["sqlite".to_string(), "unified-jobs".to_string()],
            limits: MetadataMap::new(),
        }
        .into())
    }

    async fn fail_recoverable_jobs(&self, job_ids: &[String]) -> Result<u64> {
        if job_ids.is_empty() {
            return Ok(0);
        }
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        let now = now_timestamp();
        let source_error = recovery_source_error();
        let api_error = recovery_api_error();
        let ids = quoted_job_ids(job_ids);
        let job_result = sqlx::query(&format!(
            "UPDATE jobs SET
                status = 'failed',
                phase = 'complete',
                updated_at = ?,
                finished_at = COALESCE(finished_at, ?),
                heartbeat_json = CASE
                    WHEN heartbeat_json IS NULL THEN NULL
                    ELSE json_set(heartbeat_json, '$.status', 'failed', '$.phase', 'complete')
                END,
                last_error_json = ?
             WHERE job_id IN ({ids}) AND status IN ('running', 'waiting')"
        ))
        .bind(now.0.as_str())
        .bind(now.0.as_str())
        .bind(to_json(&Some(source_error))?)
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
        let failed = job_result.rows_affected();
        sqlx::query(&format!(
            "UPDATE job_attempts SET
                status = 'failed',
                finished_at = COALESCE(finished_at, ?),
                error_json = ?
             WHERE job_id IN ({ids}) AND status IN ('queued', 'pending', 'running', 'waiting', 'blocked', 'canceling')"
        ))
        .bind(now.0.as_str())
        .bind(to_json(&Some(api_error.clone()))?)
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
        sqlx::query(&format!(
            "UPDATE job_stages SET
                status = 'failed',
                completed_at = COALESCE(completed_at, ?),
                error_json = ?
             WHERE job_id IN ({ids}) AND status IN ('queued', 'pending', 'running', 'waiting', 'blocked', 'canceling')"
        ))
        .bind(now.0.as_str())
        .bind(to_json(&Some(api_error))?)
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
        sqlx::query(&format!(
            "UPDATE job_heartbeats SET
                heartbeat_json = json_set(heartbeat_json, '$.status', 'failed', '$.phase', 'complete')
             WHERE job_id IN ({ids})"
        ))
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
        sqlx::query(&format!(
            "UPDATE provider_reservations SET
                status = 'failed',
                updated_at = ?
             WHERE job_id IN ({ids}) AND status IN ('requested', 'queued', 'granted', 'active')"
        ))
        .bind(now.0.as_str())
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
        tx.commit().await.map_err(sql_error)?;
        Ok(failed)
    }
}
