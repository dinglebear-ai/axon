use axon_api::source::*;

use crate::boundary::Result;
use crate::unified_codec::*;

pub(super) async fn reset_job_for_retry(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    job_id: JobId,
    current_status: LifecycleStatus,
    attempt: u32,
    idempotency_key: Option<&str>,
    request_json: Option<&str>,
    metadata: &MetadataMap,
    stage_plan: &[JobStagePlan],
) -> Result<()> {
    if matches!(
        current_status,
        LifecycleStatus::Running | LifecycleStatus::Waiting | LifecycleStatus::Canceling
    ) {
        return Err(ApiError::new(
            "job_retry.active_job",
            ErrorStage::Planning,
            "only terminal, blocked, queued, or pending jobs can be retried",
        ));
    }
    let now = now_timestamp();
    let result = sqlx::query(
        "UPDATE jobs SET
            intent = 'retry',
            status = 'queued',
            phase = 'queued',
            attempt = ?,
            request_json = ?,
            metadata_json = ?,
            idempotency_key = COALESCE(?, idempotency_key),
            updated_at = ?,
            started_at = NULL,
            finished_at = NULL,
            last_error_json = NULL
         WHERE job_id = ?",
    )
    .bind(attempt as i64)
    .bind(request_json)
    .bind(to_json(metadata)?)
    .bind(idempotency_key)
    .bind(now.0.as_str())
    .bind(job_id.0.to_string())
    .execute(&mut **tx)
    .await
    .map_err(sql_error)?;
    if result.rows_affected() == 0 {
        return Err(missing_job(job_id));
    }
    sqlx::query("DELETE FROM job_stages WHERE job_id = ?")
        .bind(job_id.0.to_string())
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
    for stage in stage_plan {
        sqlx::query(
            "INSERT INTO job_stages (
                stage_id, job_id, phase, status, required, provider_requirements_json,
                counts_json
            ) VALUES (?, ?, ?, 'queued', ?, ?, NULL)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(job_id.0.to_string())
        .bind(enum_name(stage.phase)?)
        .bind(if stage.required { 1_i64 } else { 0_i64 })
        .bind(to_json(&stage.provider_requirements)?)
        .execute(&mut **tx)
        .await
        .map_err(sql_error)?;
    }
    Ok(())
}

pub(super) async fn terminalize_active_children(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    job_id: JobId,
    status: LifecycleStatus,
    timestamp: &Timestamp,
    error: Option<ApiError>,
) -> Result<()> {
    sqlx::query(
        "UPDATE job_attempts SET
            status = ?,
            finished_at = COALESCE(finished_at, ?),
            error_json = ?
         WHERE job_id = ? AND status IN ('queued', 'pending', 'running', 'waiting', 'blocked', 'canceling')",
    )
    .bind(enum_name(status)?)
    .bind(timestamp.0.as_str())
    .bind(optional_to_json(&error)?)
    .bind(job_id.0.to_string())
    .execute(&mut **tx)
    .await
    .map_err(sql_error)?;
    sqlx::query(
        "UPDATE job_stages SET
            status = ?,
            completed_at = COALESCE(completed_at, ?),
            error_json = ?
         WHERE job_id = ? AND status IN ('queued', 'pending', 'running', 'waiting', 'blocked', 'canceling')",
    )
    .bind(enum_name(status)?)
    .bind(timestamp.0.as_str())
    .bind(optional_to_json(&error)?)
    .bind(job_id.0.to_string())
    .execute(&mut **tx)
    .await
    .map_err(sql_error)?;
    let provider_status = provider_status_for_terminal(status);
    sqlx::query(
        "UPDATE provider_reservations SET
            status = ?,
            updated_at = ?
         WHERE job_id = ? AND status IN ('requested', 'queued', 'granted', 'active')",
    )
    .bind(enum_name(provider_status)?)
    .bind(timestamp.0.as_str())
    .bind(job_id.0.to_string())
    .execute(&mut **tx)
    .await
    .map_err(sql_error)?;
    update_heartbeat_json_status(tx, job_id, status).await
}

pub(super) async fn update_heartbeat_json_status(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    job_id: JobId,
    status: LifecycleStatus,
) -> Result<()> {
    let phase = if status == LifecycleStatus::Canceled {
        PipelinePhase::Canceled
    } else {
        PipelinePhase::Complete
    };
    sqlx::query(
        "UPDATE jobs SET
            heartbeat_json = CASE
                WHEN heartbeat_json IS NULL THEN NULL
                ELSE json_set(heartbeat_json, '$.status', ?, '$.phase', ?)
            END
         WHERE job_id = ?",
    )
    .bind(enum_name(status)?)
    .bind(enum_name(phase)?)
    .bind(job_id.0.to_string())
    .execute(&mut **tx)
    .await
    .map_err(sql_error)?;
    sqlx::query(
        "UPDATE job_heartbeats SET
            heartbeat_json = json_set(heartbeat_json, '$.status', ?, '$.phase', ?)
         WHERE job_id = ?",
    )
    .bind(enum_name(status)?)
    .bind(enum_name(phase)?)
    .bind(job_id.0.to_string())
    .execute(&mut **tx)
    .await
    .map_err(sql_error)?;
    Ok(())
}

pub(super) fn append_recovery_filter(
    sql: &mut String,
    kind: Option<&str>,
    cutoff: Option<&Timestamp>,
) {
    if let Some(kind) = kind {
        sql.push_str(" AND kind = '");
        sql.push_str(&escape_sql(kind));
        sql.push('\'');
    }
    if cutoff.is_some() {
        sql.push_str(
            " AND COALESCE(json_extract(heartbeat_json, '$.heartbeat_at'), updated_at) < ?",
        );
    }
}

pub(super) fn quoted_job_ids(job_ids: &[String]) -> String {
    job_ids
        .iter()
        .map(|id| format!("'{}'", escape_sql(id)))
        .collect::<Vec<_>>()
        .join(",")
}

pub(super) fn recovery_source_error() -> SourceError {
    SourceError {
        code: "job.recovered_stale".to_string(),
        severity: Severity::Failed,
        message: "stale running job was failed by recovery".to_string(),
        source_item_key: None,
        retryable: true,
        provider_id: None,
        cause: None,
    }
}

pub(super) fn recovery_api_error() -> ApiError {
    ApiError::new(
        "job.recovered_stale",
        ErrorStage::Publishing,
        "stale running job was failed by recovery",
    )
}

pub(super) fn cancel_api_error(reason: Option<&str>) -> ApiError {
    ApiError::new(
        "job.canceled",
        ErrorStage::Publishing,
        reason.unwrap_or("job was canceled"),
    )
}

fn provider_status_for_terminal(status: LifecycleStatus) -> ProviderReservationStatus {
    match status {
        LifecycleStatus::Canceled => ProviderReservationStatus::Canceled,
        LifecycleStatus::Expired => ProviderReservationStatus::Expired,
        LifecycleStatus::Failed => ProviderReservationStatus::Failed,
        _ => ProviderReservationStatus::Released,
    }
}
