use std::collections::BTreeMap;

use axon_api::source::*;

use super::Result;

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

pub(super) fn cleanup_debt_natural_key(
    debt: &CleanupDebt,
) -> Result<(SourceId, String, CleanupDebtKind, String)> {
    let selector_json = serde_json::to_string(&debt.selector).map_err(|error| {
        ApiError::new(
            "source.ledger.cleanup_selector_invalid",
            ErrorStage::Cleaning,
            format!("failed to serialize cleanup selector: {error}"),
        )
    })?;
    Ok((
        debt.source_id.clone(),
        debt.generation
            .as_ref()
            .map(|value| value.0.clone())
            .unwrap_or_default(),
        debt.kind,
        selector_json,
    ))
}

pub(super) fn validate_cleanup_debt(debt: &CleanupDebt) -> Result<()> {
    match &debt.selector {
        CleanupSelector::Source { source_id } if source_id != &debt.source_id => {
            Err(cleanup_selector_mismatch_error(debt))
        }
        CleanupSelector::Generation {
            source_id,
            generation,
        } if source_id != &debt.source_id || Some(generation) != debt.generation.as_ref() => {
            Err(cleanup_selector_mismatch_error(debt))
        }
        CleanupSelector::SourceItem {
            source_id,
            generation,
            ..
        } if source_id != &debt.source_id || Some(generation) != debt.generation.as_ref() => {
            Err(cleanup_selector_mismatch_error(debt))
        }
        _ => Ok(()),
    }
}

pub(super) fn apply_cleanup_debt_update(existing: &mut CleanupDebt, next: CleanupDebt) {
    let existing_terminal = existing.completed_at.is_some();
    existing.attempts = existing.attempts.max(next.attempts);
    if existing_terminal {
        return;
    }

    existing.debt_id = next.debt_id;
    existing.job_id = next.job_id;
    existing.status = next.status;
    existing.last_error = next.last_error;
    existing.next_retry_at = next.next_retry_at;
    existing.completed_at = next.completed_at;
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

pub(super) fn add_seconds(timestamp: &Timestamp, seconds: u64) -> Timestamp {
    let parsed = chrono::DateTime::parse_from_rfc3339(&timestamp.0)
        .map(|value| value.with_timezone(&chrono::Utc));
    match parsed {
        Ok(value) => Timestamp((value + chrono::Duration::seconds(seconds as i64)).to_rfc3339()),
        Err(_) => timestamp.clone(),
    }
}

pub(super) fn timestamp_after(left: &Timestamp, right: &Timestamp) -> Result<bool> {
    let left = chrono::DateTime::parse_from_rfc3339(&left.0).map_err(|error| {
        ApiError::new(
            "source.ledger.invalid_timestamp",
            ErrorStage::Leasing,
            format!("invalid lease timestamp {}: {error}", left.0),
        )
    })?;
    let right = chrono::DateTime::parse_from_rfc3339(&right.0).map_err(|error| {
        ApiError::new(
            "source.ledger.invalid_timestamp",
            ErrorStage::Leasing,
            format!("invalid lease timestamp {}: {error}", right.0),
        )
    })?;
    Ok(left > right)
}

pub(super) fn source_missing_error(source_id: &SourceId) -> ApiError {
    ApiError::new(
        "source.ledger.source_missing",
        ErrorStage::Planning,
        format!("source {} does not exist", source_id.0),
    )
    .with_source_id(source_id.0.clone())
}

pub(super) fn generation_missing_error(
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> ApiError {
    ApiError::new(
        "source.ledger.generation_missing",
        ErrorStage::Publishing,
        format!("generation {} does not exist", generation.0),
    )
    .with_source_id(source_id.0.clone())
}

pub(super) fn lease_missing_error(lease_id: &LeaseId) -> ApiError {
    ApiError::new(
        "source.ledger.lease_missing",
        ErrorStage::Leasing,
        format!("lease {} does not exist", lease_id.0),
    )
}

fn cleanup_selector_mismatch_error(debt: &CleanupDebt) -> ApiError {
    ApiError::new(
        "source.ledger.cleanup_selector_mismatch",
        ErrorStage::Cleaning,
        "cleanup selector does not match cleanup debt source/generation",
    )
    .with_source_id(debt.source_id.0.clone())
}
