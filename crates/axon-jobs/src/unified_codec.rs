use axon_api::source::*;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::boundary::Result;

pub(crate) fn new_job_descriptor(
    job_id: JobId,
    kind: JobKind,
    timestamp: Timestamp,
) -> JobDescriptor {
    JobDescriptor {
        kind,
        id: job_id,
        status_url: format!("/v1/jobs/{job_id}", job_id = job_id.0),
        events_url: format!("/v1/jobs/{job_id}/events", job_id = job_id.0),
        stream_url: format!("/v1/jobs/{job_id}/stream", job_id = job_id.0),
        poll_after_ms: 1000,
        cancel_url: Some(format!("/v1/jobs/{job_id}/cancel", job_id = job_id.0)),
        retry_url: Some(format!("/v1/jobs/{job_id}/retry", job_id = job_id.0)),
        job_id,
        status: LifecycleStatus::Queued,
        poll: None,
        created_at: Some(timestamp.clone()),
        updated_at: Some(timestamp),
    }
}

pub(crate) fn descriptor(summary: &JobSummary) -> JobDescriptor {
    JobDescriptor {
        kind: summary.kind,
        id: summary.job_id,
        status_url: format!("/v1/jobs/{job_id}", job_id = summary.job_id.0),
        events_url: format!("/v1/jobs/{job_id}/events", job_id = summary.job_id.0),
        stream_url: format!("/v1/jobs/{job_id}/stream", job_id = summary.job_id.0),
        poll_after_ms: 1000,
        cancel_url: Some(format!(
            "/v1/jobs/{job_id}/cancel",
            job_id = summary.job_id.0
        )),
        retry_url: Some(format!(
            "/v1/jobs/{job_id}/retry",
            job_id = summary.job_id.0
        )),
        job_id: summary.job_id,
        status: summary.status,
        poll: None,
        created_at: Some(summary.created_at.clone()),
        updated_at: Some(summary.updated_at.clone()),
    }
}

pub(crate) async fn find_by_idempotency_key(
    pool: &SqlitePool,
    idempotency_key: &str,
) -> Result<Option<JobSummary>> {
    sqlx::query("SELECT * FROM jobs WHERE idempotency_key = ?")
        .bind(idempotency_key)
        .fetch_optional(pool)
        .await
        .map_err(sql_error)?
        .map(row_to_summary)
        .transpose()
}

pub(crate) async fn ensure_job_pool(pool: &SqlitePool, job_id: JobId) -> Result<()> {
    let found = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM jobs WHERE job_id = ?")
        .bind(job_id.0.to_string())
        .fetch_one(pool)
        .await
        .map_err(sql_error)?;
    if found == 0 {
        return Err(missing_job(job_id));
    }
    Ok(())
}

pub(crate) async fn ensure_job(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    job_id: JobId,
) -> Result<()> {
    let found = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM jobs WHERE job_id = ?")
        .bind(job_id.0.to_string())
        .fetch_one(&mut **tx)
        .await
        .map_err(sql_error)?;
    if found == 0 {
        return Err(missing_job(job_id));
    }
    Ok(())
}

pub(crate) fn row_to_summary(row: sqlx::sqlite::SqliteRow) -> Result<JobSummary> {
    Ok(JobSummary {
        job_id: JobId::new(parse_uuid(row.get::<String, _>("job_id"))?),
        kind: parse_enum(row.get::<String, _>("kind"))?,
        intent: parse_optional_enum(row.get::<Option<String>, _>("intent"))?,
        status: parse_enum(row.get::<String, _>("status"))?,
        phase: parse_enum(row.get::<String, _>("phase"))?,
        created_at: Timestamp(row.get("created_at")),
        updated_at: Timestamp(row.get("updated_at")),
        started_at: row.get::<Option<String>, _>("started_at").map(Timestamp),
        finished_at: row.get::<Option<String>, _>("finished_at").map(Timestamp),
        source_id: row.get::<Option<String>, _>("source_id").map(SourceId::new),
        watch_id: row.get::<Option<String>, _>("watch_id").map(WatchId::new),
        parent_job_id: row
            .get::<Option<String>, _>("parent_job_id")
            .map(parse_uuid)
            .transpose()?
            .map(JobId::new),
        root_job_id: row
            .get::<Option<String>, _>("root_job_id")
            .map(parse_uuid)
            .transpose()?
            .map(JobId::new),
        attempt: row.get::<i64, _>("attempt") as u32,
        priority: parse_enum(row.get::<String, _>("priority"))?,
        counts: from_optional_json(row.get::<Option<String>, _>("counts_json"))?,
        current: from_optional_json(row.get::<Option<String>, _>("current_json"))?,
        heartbeat: from_optional_json(row.get::<Option<String>, _>("heartbeat_json"))?,
        last_error: from_optional_json(row.get::<Option<String>, _>("last_error_json"))?,
        warnings: from_json(row.get::<String, _>("warnings_json"))?,
    })
}

pub(crate) fn row_to_event(row: sqlx::sqlite::SqliteRow) -> Result<JobEvent> {
    Ok(JobEvent {
        event_id: row.get("event_id"),
        sequence: row.get::<i64, _>("sequence") as u64,
        job_id: JobId::new(parse_uuid(row.get::<String, _>("job_id"))?),
        attempt: row.get::<i64, _>("attempt") as u32,
        stage_id: row
            .get::<Option<String>, _>("stage_id")
            .map(parse_uuid)
            .transpose()?
            .map(StageId::new),
        phase: parse_enum(row.get::<String, _>("phase"))?,
        status: parse_enum(row.get::<String, _>("status"))?,
        severity: parse_enum(row.get::<String, _>("severity"))?,
        visibility: parse_enum(row.get::<String, _>("visibility"))?,
        message: row.get("message"),
        timestamp: Timestamp(row.get("timestamp")),
        details: from_json(row.get::<String, _>("details_json"))?,
    })
}

pub(crate) fn row_to_artifact(row: sqlx::sqlite::SqliteRow) -> Result<ArtifactRef> {
    let artifact_id = ArtifactId::new(row.get::<String, _>("artifact_id"));
    Ok(ArtifactRef {
        uri: format!("artifact://{}", artifact_id.0),
        artifact_id,
        artifact_kind: parse_enum(row.get::<String, _>("artifact_kind"))?,
        size_bytes: row
            .get::<Option<i64>, _>("size_bytes")
            .map(|value| value as u64),
        content_hash: row.get("content_hash"),
        created_at: Timestamp(row.get("created_at")),
    })
}

pub(crate) fn row_to_attempt(row: sqlx::sqlite::SqliteRow) -> Result<JobAttemptSnapshot> {
    Ok(JobAttemptSnapshot {
        attempt: row.get::<i64, _>("attempt") as u32,
        status: parse_enum(row.get::<String, _>("status"))?,
        worker_id: row.get("worker_id"),
        started_at: Timestamp(row.get("started_at")),
        finished_at: row.get::<Option<String>, _>("finished_at").map(Timestamp),
        heartbeat_at: row.get::<Option<String>, _>("heartbeat_at").map(Timestamp),
        error: from_optional_json(row.get::<Option<String>, _>("error_json"))?,
    })
}

pub(crate) fn row_to_stage(row: sqlx::sqlite::SqliteRow) -> Result<JobStageSnapshot> {
    Ok(JobStageSnapshot {
        stage_id: StageId::new(parse_uuid(row.get::<String, _>("stage_id"))?),
        phase: parse_enum(row.get::<String, _>("phase"))?,
        status: parse_enum(row.get::<String, _>("status"))?,
        required: row.get::<i64, _>("required") != 0,
        provider_requirements: from_json(row.get::<String, _>("provider_requirements_json"))?,
        counts: from_optional_json(row.get::<Option<String>, _>("counts_json"))?.unwrap_or(
            StageCounts {
                items_total: None,
                items_done: 0,
                documents_total: None,
                documents_done: 0,
                chunks_total: None,
                chunks_done: 0,
                bytes_total: None,
                bytes_done: 0,
            },
        ),
        started_at: row.get::<Option<String>, _>("started_at").map(Timestamp),
        completed_at: row.get::<Option<String>, _>("completed_at").map(Timestamp),
        error: from_optional_json(row.get::<Option<String>, _>("error_json"))?,
    })
}

pub(crate) fn event_details(event: &SourceProgressEvent) -> MetadataMap {
    let mut details = MetadataMap::new();
    details.insert(
        "source_progress_event".to_string(),
        serde_json::to_value(event).unwrap_or(serde_json::Value::Null),
    );
    if let Some(dedupe_key) = &event.dedupe_key {
        details.insert("dedupe_key".to_string(), serde_json::json!(dedupe_key));
    }
    details
}

pub(crate) async fn count_children_by_job_ids(
    pool: &SqlitePool,
    table: &str,
    quoted_job_ids: &str,
) -> Result<u64> {
    if quoted_job_ids.is_empty() {
        return Ok(0);
    }
    sqlx::query_scalar::<_, i64>(&format!(
        "SELECT COUNT(*) FROM {table} WHERE job_id IN ({quoted_job_ids})"
    ))
    .fetch_one(pool)
    .await
    .map(|value| value as u64)
    .map_err(sql_error)
}

pub(crate) fn enum_name<T: serde::Serialize>(value: T) -> Result<String> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .ok_or_else(|| ApiError::new("job.enum_invalid", ErrorStage::Planning, "invalid enum"))
}

pub(crate) fn parse_enum<T: serde::de::DeserializeOwned>(value: String) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(json_error)
}

pub(crate) fn to_json<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(json_error)
}

pub(crate) fn optional_to_json<T: serde::Serialize>(value: &Option<T>) -> Result<Option<String>> {
    value.as_ref().map(to_json).transpose()
}

pub(crate) fn from_json<T: serde::de::DeserializeOwned>(value: String) -> Result<T> {
    serde_json::from_str(&value).map_err(json_error)
}

pub(crate) fn from_optional_json<T: serde::de::DeserializeOwned>(
    value: Option<String>,
) -> Result<Option<T>> {
    value.map(from_json).transpose()
}

pub(crate) fn now_timestamp() -> Timestamp {
    Timestamp::from(chrono::Utc::now())
}

pub(crate) fn is_terminal(status: LifecycleStatus) -> bool {
    matches!(
        status,
        LifecycleStatus::Completed
            | LifecycleStatus::CompletedDegraded
            | LifecycleStatus::Failed
            | LifecycleStatus::Canceled
            | LifecycleStatus::Expired
            | LifecycleStatus::Skipped
    )
}

pub(crate) fn escape_sql(value: &str) -> String {
    value.replace('\'', "''")
}

pub(crate) fn attempt_id(job_id: JobId, attempt: u32) -> String {
    format!("{}:{attempt}", job_id.0)
}

pub(crate) fn reject_non_public_visibility(visibility: Option<Visibility>) -> Result<()> {
    if matches!(
        visibility,
        Some(Visibility::Internal | Visibility::Sensitive)
    ) {
        return Err(ApiError::new(
            "job_event.visibility_forbidden",
            ErrorStage::Authorizing,
            "internal and sensitive job events require an internal/admin event API",
        ));
    }
    Ok(())
}

pub(crate) fn missing_job(job_id: JobId) -> ApiError {
    ApiError::new(
        "job.not_found",
        ErrorStage::Retrieving,
        format!("job {} not found", job_id.0),
    )
}

pub(crate) fn sql_error(error: sqlx::Error) -> ApiError {
    ApiError::new(
        "job.sqlite_error",
        ErrorStage::Publishing,
        error.to_string(),
    )
}

fn parse_optional_enum<T: serde::de::DeserializeOwned>(value: Option<String>) -> Result<Option<T>> {
    value.map(parse_enum).transpose()
}

pub(crate) fn parse_uuid(value: String) -> Result<Uuid> {
    Uuid::parse_str(&value).map_err(|error| {
        ApiError::new(
            "job.uuid_invalid",
            ErrorStage::Retrieving,
            format!("invalid job uuid: {error}"),
        )
    })
}

fn json_error(error: serde_json::Error) -> ApiError {
    ApiError::new("job.json_error", ErrorStage::Publishing, error.to_string())
}
