use axon_api::source::*;
use sqlx::Row;

use super::SqliteUnifiedJobStore;
use super::control_helpers::{
    append_recovery_filter, fail_stale_job_after_attempt_limit, reset_stale_job_for_recovery,
};
use crate::boundary::Result;
use crate::limits::clamp_page_limit;
use crate::unified_codec::*;

impl SqliteUnifiedJobStore {
    pub(crate) async fn recover_jobs(
        &self,
        request: JobRecoveryRequest,
    ) -> Result<JobRecoveryResult> {
        self.recover_jobs_with_attempt_limit(request, None).await
    }

    /// Recover stale jobs, terminal-failing rows that have consumed their
    /// recovery-attempt budget instead of requeueing them forever.
    pub async fn recover_jobs_with_attempt_limit(
        &self,
        request: JobRecoveryRequest,
        max_attempts: Option<u32>,
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
                "recovery requires a stale cutoff (--stale-before) unless allow_without_cutoff is explicit",
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
        let mut failed = 0_u64;
        if !request.dry_run && scanned > 0 {
            let mut tx = self.pool.begin().await.map_err(sql_error)?;
            for row in rows {
                let job_id = JobId::new(parse_uuid(row.get::<String, _>("job_id"))?);
                let attempt = (row.get::<i64, _>("attempt") as u32).max(1);
                if max_attempts.is_some_and(|limit| attempt >= limit) {
                    if fail_stale_job_after_attempt_limit(
                        &mut tx,
                        job_id,
                        attempt,
                        max_attempts.expect("checked above"),
                    )
                    .await?
                    {
                        failed += 1;
                    }
                    continue;
                }
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
            jobs_failed: failed,
        })
    }
}
