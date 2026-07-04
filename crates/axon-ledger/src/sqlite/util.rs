use std::collections::BTreeMap;

use axon_api::source::*;

use crate::store::Result;

pub(super) fn cleanup_selector_hash(selector: &CleanupSelector) -> Result<String> {
    let selector_json = serde_json::to_vec(selector).map_err(json_error)?;
    Ok(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, &selector_json).to_string())
}

pub(super) fn keyed_manifest_items(
    items: Vec<ManifestItem>,
) -> BTreeMap<SourceItemKey, ManifestItem> {
    items
        .into_iter()
        .map(|item| (item.source_item_key.clone(), item))
        .collect()
}

pub(super) fn manifest_item_changed(old: &ManifestItem, next: &ManifestItem) -> bool {
    old.content_hash != next.content_hash || old.version != next.version || old.mtime != next.mtime
}

pub(super) fn stage_header(phase: PipelinePhase) -> StageResultHeader {
    StageResultHeader {
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        stage_id: StageId::new(uuid::Uuid::from_u128(0)),
        phase,
        status: LifecycleStatus::Completed,
        started_at: timestamp(),
        completed_at: Some(timestamp()),
        counts: StageCounts {
            items_total: None,
            items_done: 0,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    }
}

pub(super) fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

pub(super) fn add_seconds(timestamp: &Timestamp, seconds: u64) -> Result<Timestamp> {
    let seconds = i64::try_from(seconds).map_err(|_| lease_ttl_invalid_error())?;
    let duration = chrono::TimeDelta::try_seconds(seconds).ok_or_else(lease_ttl_invalid_error)?;
    let parsed = chrono::DateTime::parse_from_rfc3339(&timestamp.0)
        .map(|value| value.with_timezone(&chrono::Utc));
    match parsed {
        Ok(value) => value
            .checked_add_signed(duration)
            .map(|value| Timestamp(value.to_rfc3339()))
            .ok_or_else(lease_ttl_invalid_error),
        Err(_) => Ok(timestamp.clone()),
    }
}

pub(super) fn timestamp_after(left: &Timestamp, right: &Timestamp) -> Result<bool> {
    timestamp_str_after(&left.0, &right.0)
}

pub(super) fn timestamp_str_after(left: &str, right: &str) -> Result<bool> {
    let left = chrono::DateTime::parse_from_rfc3339(left).map_err(|error| {
        ApiError::new(
            "source.ledger.invalid_timestamp",
            ErrorStage::Leasing,
            format!("invalid lease timestamp {left}: {error}"),
        )
    })?;
    let right = chrono::DateTime::parse_from_rfc3339(right).map_err(|error| {
        ApiError::new(
            "source.ledger.invalid_timestamp",
            ErrorStage::Leasing,
            format!("invalid lease timestamp {right}: {error}"),
        )
    })?;
    Ok(left > right)
}

pub(super) fn enum_wire_value<T: serde::Serialize>(value: T) -> Result<String> {
    serde_json::to_value(value)
        .map_err(json_error)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| {
            ApiError::new(
                "source.ledger.enum_wire_value_invalid",
                ErrorStage::Planning,
                "ledger scalar enum did not serialize to a string",
            )
        })
}

pub(super) fn json_error(error: serde_json::Error) -> ApiError {
    ApiError::new(
        "source.ledger.json",
        ErrorStage::Upserting,
        format!("ledger JSON operation failed: {error}"),
    )
}

fn lease_ttl_invalid_error() -> ApiError {
    ApiError::new(
        "source.ledger.lease_ttl_invalid",
        ErrorStage::Leasing,
        "lease ttl is too large to represent as a timestamp",
    )
}
