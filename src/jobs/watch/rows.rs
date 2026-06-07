use super::{WatchDef, WatchDefCreate, WatchRun, WatchRunArtifact};
use crate::jobs::query::ms_to_dt;
use uuid::Uuid;

pub(super) type WatchDefRow = (
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

pub(super) type WatchRunRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    i64,
    i64,
);

pub(super) type WatchRunArtifactRow = (i64, String, String, Option<String>, Option<String>, i64);

fn parse_json(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::json!({}))
}

pub(super) fn parse_watch_def_row(row: WatchDefRow) -> WatchDef {
    let (
        id,
        name,
        task_type,
        task_payload,
        every_seconds,
        enabled,
        next_run_at,
        lease_expires_at,
        last_run_at,
        created_at,
        updated_at,
    ) = row;
    WatchDef {
        id: Uuid::parse_str(&id).unwrap_or_default(),
        name,
        task_type,
        task_payload: parse_json(&task_payload),
        every_seconds,
        enabled: enabled != 0,
        next_run_at: ms_to_dt(next_run_at),
        lease_expires_at: lease_expires_at.map(ms_to_dt),
        last_run_at: last_run_at.map(ms_to_dt),
        created_at: ms_to_dt(created_at),
        updated_at: ms_to_dt(updated_at),
    }
}

pub(super) fn parse_watch_run_row(row: WatchRunRow) -> WatchRun {
    let (
        id,
        watch_id,
        status,
        dispatched_job_id,
        error_text,
        result_json,
        started_at,
        finished_at,
        created_at,
        updated_at,
    ) = row;
    WatchRun {
        id: Uuid::parse_str(&id).unwrap_or_default(),
        watch_id: Uuid::parse_str(&watch_id).unwrap_or_default(),
        status,
        dispatched_job_id: dispatched_job_id.and_then(|raw| Uuid::parse_str(&raw).ok()),
        error_text,
        result_json: result_json.as_deref().map(parse_json),
        started_at: started_at.map(ms_to_dt),
        finished_at: finished_at.map(ms_to_dt),
        created_at: ms_to_dt(created_at),
        updated_at: ms_to_dt(updated_at),
    }
}

pub(super) fn parse_watch_run_artifact_row(row: WatchRunArtifactRow) -> WatchRunArtifact {
    let (id, watch_run_id, kind, path, payload, created_at) = row;
    WatchRunArtifact {
        id,
        watch_run_id: Uuid::parse_str(&watch_run_id).unwrap_or_default(),
        kind,
        path,
        payload: payload
            .as_deref()
            .map(parse_json)
            .unwrap_or_else(|| serde_json::json!({})),
        created_at: ms_to_dt(created_at),
    }
}

pub(super) fn normalize_watch_def_create(input: &WatchDefCreate) -> WatchDefCreate {
    WatchDefCreate {
        name: input.name.trim().to_string(),
        task_type: input.task_type.clone(),
        task_payload: input.task_payload.clone(),
        every_seconds: input.every_seconds,
        enabled: input.enabled,
        next_run_at: input.next_run_at,
    }
}
