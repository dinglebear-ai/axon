use super::{FreshnessDef, FreshnessDefCreate, FreshnessRun};
use uuid::Uuid;

fn ms_to_dt(ms: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms).unwrap_or_else(chrono::Utc::now)
}

pub(super) type FreshnessDefRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    i64,
    i64,
    Option<i64>,
    Option<i64>,
    i64,
    i64,
);

pub(super) type FreshnessRunRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    i64,
    i64,
);

fn parse_json(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::json!({}))
}

pub(super) fn parse_freshness_def_row(row: FreshnessDefRow) -> FreshnessDef {
    let (
        id,
        name,
        command,
        target,
        identity_hash,
        request_json,
        config_json,
        every_seconds,
        enabled,
        next_run_at,
        lease_expires_at,
        last_run_at,
        created_at,
        updated_at,
    ) = row;
    FreshnessDef {
        id: Uuid::parse_str(&id).unwrap_or_default(),
        name,
        command,
        target,
        identity_hash,
        request_json: parse_json(&request_json),
        config_json: parse_json(&config_json),
        every_seconds,
        enabled: enabled != 0,
        next_run_at: ms_to_dt(next_run_at),
        lease_expires_at: lease_expires_at.map(ms_to_dt),
        last_run_at: last_run_at.map(ms_to_dt),
        created_at: ms_to_dt(created_at),
        updated_at: ms_to_dt(updated_at),
    }
}

pub(super) fn parse_freshness_run_row(row: FreshnessRunRow) -> FreshnessRun {
    let (
        id,
        freshness_id,
        status,
        dispatched_job_id,
        error_text,
        result_json,
        started_at,
        finished_at,
        heartbeat_at,
        created_at,
        updated_at,
    ) = row;
    FreshnessRun {
        id: Uuid::parse_str(&id).unwrap_or_default(),
        freshness_id: Uuid::parse_str(&freshness_id).unwrap_or_default(),
        status,
        dispatched_job_id: dispatched_job_id.and_then(|raw| Uuid::parse_str(&raw).ok()),
        error_text,
        result_json: result_json.as_deref().map(parse_json),
        started_at: started_at.map(ms_to_dt),
        finished_at: finished_at.map(ms_to_dt),
        heartbeat_at: heartbeat_at.map(ms_to_dt),
        created_at: ms_to_dt(created_at),
        updated_at: ms_to_dt(updated_at),
    }
}

pub(super) fn normalize_freshness_def_create(input: &FreshnessDefCreate) -> FreshnessDefCreate {
    FreshnessDefCreate {
        name: input.name.trim().to_string(),
        command: input.command.trim().to_string(),
        target: input.target.trim().to_string(),
        identity_hash: input.identity_hash.trim().to_string(),
        request_json: input.request_json.clone(),
        config_json: input.config_json.clone(),
        every_seconds: input.every_seconds,
        enabled: input.enabled,
        next_run_at: input.next_run_at,
    }
}
