use axon_api::source::*;
use sqlx::{Row, sqlite::SqliteRow};

use crate::boundary::Result;
use crate::watch::validate_every_seconds;

pub(super) fn sqlite_err(err: sqlx::Error) -> ApiError {
    ApiError::new(
        "watch.storage_error",
        ErrorStage::Retrieving,
        format!("watch store error: {err}"),
    )
}

pub(super) fn json_err(err: serde_json::Error) -> ApiError {
    ApiError::new(
        "watch.storage_error",
        ErrorStage::Retrieving,
        format!("watch store serialization error: {err}"),
    )
}

pub(super) fn missing_watch(watch_id: &WatchId) -> ApiError {
    ApiError::new(
        "watch.not_found",
        ErrorStage::Retrieving,
        format!("watch {} not found", watch_id.0),
    )
}

pub(super) fn missing_job(job_id: JobId) -> ApiError {
    ApiError::new(
        "job.not_found",
        ErrorStage::Retrieving,
        format!("job {} not found", job_id.0),
    )
}

fn parse_json_str<T: serde::de::DeserializeOwned>(raw: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(raw.to_string())).ok()
}

fn parse_options(raw: &str) -> Result<AdapterOptions> {
    serde_json::from_str(raw).map_err(json_err)
}

fn parse_scope(raw: &str) -> SourceScope {
    parse_json_str(raw).unwrap_or(SourceScope::Page)
}

pub(super) fn validate_source_watch_interval(every_seconds: u64) -> Result<i64> {
    let every_seconds = i64::try_from(every_seconds).map_err(|_| {
        ApiError::new(
            "watch.invalid_schedule",
            ErrorStage::Validation,
            "every_seconds is too large",
        )
    })?;
    validate_every_seconds(every_seconds).map_err(|message| {
        ApiError::new("watch.invalid_schedule", ErrorStage::Validation, message)
    })?;
    Ok(every_seconds)
}

/// Serialize `SourceScope` as the bare persisted snake_case string.
pub(super) fn scope_to_str(scope: SourceScope) -> String {
    serde_json::to_value(scope)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "page".to_string())
}

pub(super) fn row_to_result(row: &SqliteRow) -> WatchResult {
    let watch_id = WatchId::new(row.get::<String, _>("watch_id"));
    let source_id = SourceId::new(row.get::<String, _>("source_id"));
    let canonical_uri: String = row.get("canonical_uri");
    let adapter = AdapterRef {
        name: row.get("adapter_name"),
        version: row.get("adapter_version"),
    };
    let scope = parse_scope(&row.get::<String, _>("scope"));
    let enabled: i64 = row.get("enabled");
    let every_seconds: i64 = row.get("every_seconds");
    let cron: Option<String> = row.get("cron");
    let timezone: Option<String> = row.get("timezone");
    let last_job_id: Option<String> = row.get("last_job_id");
    let last_status: Option<String> = row.get("last_status");
    let latest_job = last_job_id.map(|job_id| synth_descriptor(&job_id, last_status.as_deref()));

    WatchResult {
        watch_id,
        source_id,
        canonical_uri,
        adapter,
        scope,
        enabled: enabled != 0,
        schedule: WatchSchedule {
            every_seconds: every_seconds.max(0) as u64,
            cron,
            timezone,
        },
        job: None,
        latest_job,
        warnings: Vec::new(),
    }
}

pub(super) fn row_to_request(row: &SqliteRow) -> Result<WatchRequest> {
    let source: String = row.get("source");
    let every_seconds: i64 = row.get("every_seconds");
    let cron: Option<String> = row.get("cron");
    let timezone: Option<String> = row.get("timezone");
    let embed: i64 = row.get("embed");
    let options_json: String = row.get("options_json");
    let collection: Option<String> = row.get("collection");
    let enabled: i64 = row.get("enabled");
    Ok(WatchRequest {
        source,
        schedule: WatchSchedule {
            every_seconds: every_seconds.max(0) as u64,
            cron,
            timezone,
        },
        embed: embed != 0,
        options: parse_options(&options_json)?,
        scope: Some(parse_scope(&row.get::<String, _>("scope"))),
        collection,
        enabled: Some(enabled != 0),
    })
}

pub(super) fn row_to_auth_snapshot(row: &SqliteRow) -> Result<Option<AuthSnapshot>> {
    let raw = row
        .try_get::<Option<String>, _>("auth_snapshot_json")
        .map_err(sqlite_err)?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(json_err))
        .transpose()
}

pub(super) fn synth_descriptor(job_id: &str, status: Option<&str>) -> JobDescriptor {
    let status = status
        .and_then(parse_json_str::<LifecycleStatus>)
        .unwrap_or(LifecycleStatus::Queued);
    let job_id = JobId::new(uuid::Uuid::parse_str(job_id).unwrap_or_default());
    JobDescriptor {
        kind: JobKind::Source,
        id: job_id,
        status_url: format!("/v1/jobs/{}", job_id.0),
        events_url: format!("/v1/jobs/{}/events", job_id.0),
        stream_url: format!("/v1/jobs/{}/stream", job_id.0),
        poll_after_ms: 1000,
        cancel_url: Some(format!("/v1/jobs/{}/cancel", job_id.0)),
        retry_url: Some(format!("/v1/jobs/{}/retry", job_id.0)),
        job_id,
        status,
        poll: None,
        created_at: None,
        updated_at: None,
    }
}
