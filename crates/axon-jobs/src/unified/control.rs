use axon_api::source::*;
use axon_error::cooling::ProviderCooling;
use sqlx::Row;

use super::SqliteUnifiedJobStore;
use super::control_helpers::*;
use crate::boundary::Result;
use crate::limits::clamp_page_limit;
use crate::state_machine::validate_transition;
use crate::unified::MAX_PROVIDER_COOLDOWN_WINDOW;
use crate::unified_codec::*;

impl SqliteUnifiedJobStore {
    /// Persist a bounded provider-cooling window on a job that is currently
    /// `Waiting`.
    ///
    /// `cooling.cooldown_until` is clamped to `min(cooldown_until, now +
    /// MAX_PROVIDER_COOLDOWN_WINDOW)` before persisting — a fixed ceiling, not
    /// a floor, so a deadline already in the past round-trips unchanged
    /// rather than being pulled forward. The claim query
    /// (`claim_next_unified_job_unchecked` in
    /// `crates/axon-jobs/src/workers/unified.rs`) excludes rows whose
    /// `cooldown_until` is still in the future via the `idx_axon_jobs_claim_cooldown`
    /// partial index added alongside this column.
    ///
    /// Only applies to a job currently in `Waiting` status — cooling only
    /// makes sense while a job is parked waiting on a provider; applying it
    /// to any other status would be silently meaningless once the claim query
    /// only special-cases `waiting`.
    pub(crate) async fn apply_provider_cooling(
        &self,
        job_id: JobId,
        cooling: ProviderCooling,
    ) -> Result<()> {
        // The status check and the cooldown write must be atomic: without a
        // shared transaction and a status-scoped UPDATE, a concurrent writer
        // (claim, update_job_status, heartbeat, cancel_job — all of which
        // clear cooldown_until on leaving Waiting) could move the job off
        // Waiting between the check and the write, and this function would
        // still unconditionally re-write cooldown_until afterward, leaving a
        // non-Waiting job (e.g. Running) with a stale cooldown that later
        // paths weren't designed to clear on entry.
        let mut tx = self.pool.begin().await.map_err(sql_error)?;
        let row = sqlx::query("SELECT status FROM jobs WHERE job_id = ?")
            .bind(job_id.0.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(sql_error)?
            .ok_or_else(|| missing_job(job_id))?;
        let current = parse_enum::<LifecycleStatus>(row.get::<String, _>("status"))?;
        if current != LifecycleStatus::Waiting {
            return Err(ApiError::new(
                "job_cooling.not_waiting",
                ErrorStage::Publishing,
                format!(
                    "job {} is {:?}, not Waiting; provider cooling only applies to a job parked in Waiting",
                    job_id.0, current
                ),
            ));
        }
        let max_deadline = chrono::Utc::now()
            + chrono::Duration::from_std(MAX_PROVIDER_COOLDOWN_WINDOW)
                .unwrap_or(chrono::Duration::hours(1));
        let clamped = cooling.cooldown_until.min(max_deadline);
        let result = sqlx::query(
            "UPDATE jobs SET cooldown_until = ? WHERE job_id = ? AND status = 'waiting'",
        )
        .bind(clamped.to_rfc3339())
        .bind(job_id.0.to_string())
        .execute(&mut *tx)
        .await
        .map_err(sql_error)?;
        if result.rows_affected() == 0 {
            return Err(ApiError::new(
                "job_cooling.not_waiting",
                ErrorStage::Publishing,
                format!(
                    "job {} left Waiting before cooling could be applied",
                    job_id.0
                ),
            ));
        }
        tx.commit().await.map_err(sql_error)?;
        Ok(())
    }

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
        // cooldown_until: a Waiting job legally transitions to Canceling/
        // Canceled here, and cooldown is only ever meaningful while a job is
        // Waiting — clear it unconditionally so a canceled/canceling job
        // never carries a stale cooldown into its next lifecycle.
        sqlx::query(
            "UPDATE jobs SET
                status = ?,
                phase = ?,
                updated_at = ?,
                finished_at = COALESCE(?, finished_at),
                cooldown_until = NULL
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
        let cutoff = request.stale_before.clone().or_else(|| {
            request.older_than_seconds.map(|seconds| {
                Timestamp::from(chrono::Utc::now() - chrono::Duration::seconds(seconds as i64))
            })
        });
        if cutoff.is_none() && !request.allow_without_cutoff {
            return Err(ApiError::new(
                "job_recovery.cutoff_required",
                ErrorStage::Planning,
                "recovery requires older_than_seconds unless allow_without_cutoff is explicit",
            ));
        }
        let kind_filter = request.kind.map(enum_name).transpose()?;
        let limit = clamp_page_limit(request.limit);
        let mut sql = "SELECT job_id, attempt, request_json, metadata_json, stage_plan_json
                       FROM jobs WHERE status IN ('running', 'waiting')"
            .to_string();
        append_recovery_filter(&mut sql, kind_filter.as_deref(), cutoff.as_ref());
        sql.push_str(
            " ORDER BY COALESCE(json_extract(heartbeat_json, '$.heartbeat_at'), updated_at) ASC,
              job_id ASC LIMIT ",
        );
        sql.push_str(&limit.to_string());
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
        let mut requeued = 0_u64;
        if !request.dry_run && scanned > 0 {
            let mut tx = self.pool.begin().await.map_err(sql_error)?;
            for row in rows {
                let job_id = JobId::new(parse_uuid(row.get::<String, _>("job_id"))?);
                let attempt = (row.get::<i64, _>("attempt") as u32).max(1);
                let metadata = from_json::<MetadataMap>(row.get::<String, _>("metadata_json"))?;
                let stage_plan =
                    from_json::<Vec<JobStagePlan>>(row.get::<String, _>("stage_plan_json"))?;
                let request_json = row.get::<Option<String>, _>("request_json");
                if reset_stale_job_for_recovery(
                    &mut tx,
                    job_id,
                    attempt,
                    attempt + 1,
                    request_json.as_deref(),
                    &metadata,
                    &stage_plan,
                )
                .await?
                {
                    requeued += 1;
                }
            }
            tx.commit().await.map_err(sql_error)?;
        } else {
            requeued = 0;
        }
        Ok(JobRecoveryResult {
            recovered: requeued,
            job_ids: job_ids
                .into_iter()
                .filter_map(|id| parse_uuid(id).ok().map(JobId::new))
                .collect(),
            warnings: Vec::new(),
            jobs_scanned: scanned,
            jobs_requeued: requeued,
            jobs_failed: 0,
        })
    }

    pub(crate) async fn cleanup_jobs(
        &self,
        request: JobCleanupRequest,
    ) -> Result<JobCleanupResult> {
        let cutoff = request.older_than.clone().or_else(|| {
            request.older_than_seconds.map(|seconds| {
                Timestamp::from(chrono::Utc::now() - chrono::Duration::seconds(seconds as i64))
            })
        });
        if cutoff.is_none() && !request.confirm_all_terminal {
            return Err(ApiError::new(
                "job_cleanup.cutoff_required",
                ErrorStage::Planning,
                "cleanup requires older_than_seconds unless confirm_all_terminal is explicit",
            ));
        }
        if let Some(status) = request.status
            && !is_terminal(status)
        {
            return Err(ApiError::new(
                "job_cleanup.non_terminal_status",
                ErrorStage::Planning,
                "cleanup can only prune terminal jobs",
            ));
        }
        let mut predicate = String::new();
        if let Some(status) = request.status {
            predicate.push_str("status = '");
            predicate.push_str(&escape_sql(&enum_name(status)?));
            predicate.push('\'');
        } else {
            predicate.push_str(
                "status IN ('completed', 'completed_degraded', 'failed', 'canceled', 'expired', 'skipped')",
            );
        }
        if let Some(kind) = request.kind {
            predicate.push_str(" AND kind = '");
            predicate.push_str(&escape_sql(&enum_name(kind)?));
            predicate.push('\'');
        }
        if cutoff.is_some() {
            predicate.push_str(" AND updated_at < ?");
        }
        let mut sql = format!("SELECT job_id FROM jobs WHERE {predicate}");
        let limit = clamp_page_limit(request.limit);
        sql.push_str(" ORDER BY updated_at ASC, job_id ASC LIMIT ");
        sql.push_str(&limit.to_string());
        let mut query = sqlx::query(&sql);
        if let Some(cutoff) = cutoff.as_ref() {
            query = query.bind(cutoff.0.as_str());
        }
        let job_ids = query
            .fetch_all(&self.pool)
            .await
            .map_err(sql_error)?
            .into_iter()
            .map(|row| row.get::<String, _>("job_id"))
            .collect::<Vec<_>>();
        let jobs_pruned = job_ids.len() as u64;
        let ids = quoted_job_ids(&job_ids);
        let events_pruned = count_children_by_job_ids(&self.pool, "job_events", &ids).await?;
        let heartbeats_pruned =
            count_children_by_job_ids(&self.pool, "job_heartbeats", &ids).await?;
        let attempts_pruned = count_children_by_job_ids(&self.pool, "job_attempts", &ids).await?;
        let stages_pruned = count_children_by_job_ids(&self.pool, "job_stages", &ids).await?;
        let reservations_pruned =
            count_children_by_job_ids(&self.pool, "provider_reservations", &ids).await?;
        let artifacts_pruned = count_children_by_job_ids(&self.pool, "job_artifacts", &ids).await?;

        let deleted = if !request.dry_run && jobs_pruned > 0 {
            let delete_sql = format!("DELETE FROM jobs WHERE job_id IN ({ids}) AND {predicate}");
            let mut delete = sqlx::query(&delete_sql);
            if let Some(cutoff) = cutoff.as_ref() {
                delete = delete.bind(cutoff.0.as_str());
            }
            delete
                .execute(&self.pool)
                .await
                .map_err(sql_error)?
                .rows_affected()
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
            attempts_pruned,
            stages_pruned,
            reservations_pruned,
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
}
