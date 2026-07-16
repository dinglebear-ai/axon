use super::{
    ArtifactHandle, ArtifactKind, ArtifactStore, ServiceContext, UploadRecord, UploadStatusKind,
    artifact_store, audit_event, save_record, upload_error,
};
use axon_api::source::ApiError;
use chrono::{DateTime, Utc};
use std::path::Path;

pub(super) async fn expire_retained_artifact_if_needed(
    ctx: &ServiceContext,
    record: &mut UploadRecord,
    root: &Path,
) -> Result<bool, ApiError> {
    if record.status != UploadStatusKind::Completed {
        return Ok(false);
    }
    let Some(retention_until) = record.retention_until.as_ref() else {
        return Err(upload_error(
            "upload.invalid_record",
            "completed upload has no retention deadline",
        ));
    };
    let deadline = DateTime::parse_from_rfc3339(&retention_until.0)
        .map_err(|error| upload_error("upload.invalid_record", error.to_string()))?
        .with_timezone(&Utc);
    if deadline > Utc::now() {
        return Ok(false);
    }
    let artifact_id = record.artifact_id.clone().ok_or_else(|| {
        upload_error(
            "upload.invalid_record",
            "completed upload has no artifact mapping",
        )
    })?;
    artifact_store(ctx)
        .delete(ArtifactHandle {
            artifact_id,
            artifact_kind: ArtifactKind::RawContent,
            uri: None,
        })
        .await?;
    record.status = UploadStatusKind::Expired;
    record.artifact_id = None;
    record.source_ref = None;
    record
        .audit_events
        .push(audit_event("upload.retention_expire"));
    save_record(root, record).await?;
    tracing::info!(event = "upload.retention_expire", upload_id = %record.upload_id.0, "retained upload artifact expired");
    Ok(true)
}
