//! Durable staged-upload registry.
//!
//! Upload identities (`upl_*`) and artifact identities (`art_*`) are separate
//! domains. A completed upload record durably maps the former to the latter;
//! callers never derive one identifier from the other.

use crate::context::ServiceContext;
use axon_api::source::{
    ApiError, ArtifactHandle, ArtifactId, ArtifactKind, ArtifactReadResult, ErrorStage,
    MetadataMap, Page, SourceWarning, Timestamp, UploadAbortRequest, UploadAbortResult,
    UploadCompleteRequest, UploadCompleteResult, UploadCreateRequest, UploadCreateResult, UploadId,
    UploadListRequest, UploadPurpose, UploadStatus, UploadStatusKind,
};
use axon_core::boundary::{ArtifactBytesWriteRequest, ArtifactStore, FileArtifactStore};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[path = "uploads/store.rs"]
mod store;
use store::*;
#[path = "uploads/cleanup.rs"]
mod cleanup;
use cleanup::expire_retained_artifact_if_needed;
#[path = "uploads/validation.rs"]
mod validation;
use validation::*;

const MAX_UPLOAD_BYTES: u64 = 96 * 1024 * 1024;
const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;
const STAGING_TTL_HOURS: i64 = 24;
const ARTIFACT_RETENTION_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UploadRecord {
    upload_id: UploadId,
    status: UploadStatusKind,
    filename: String,
    content_type: String,
    size_bytes: u64,
    bytes_received: u64,
    purpose: UploadPurpose,
    created_at: Timestamp,
    expires_at: Timestamp,
    expected_sha256: Option<String>,
    sha256: Option<String>,
    artifact_id: Option<ArtifactId>,
    source_ref: Option<String>,
    retention_until: Option<Timestamp>,
    metadata: MetadataMap,
    abort_reason: Option<String>,
    audit_events: Vec<UploadAuditEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UploadAuditEvent {
    event: String,
    at: Timestamp,
}

impl UploadRecord {
    fn status(&self) -> UploadStatus {
        UploadStatus {
            upload_id: self.upload_id.clone(),
            status: self.status,
            filename: self.filename.clone(),
            content_type: self.content_type.clone(),
            size_bytes: self.size_bytes,
            bytes_received: self.bytes_received,
            purpose: self.purpose,
            created_at: self.created_at.clone(),
            expires_at: self.expires_at.clone(),
            artifact_id: self.artifact_id.clone(),
            source_ref: self.source_ref.clone(),
            sha256: self.sha256.clone(),
            retention_until: self.retention_until.clone(),
        }
    }
}

pub async fn create_upload(
    ctx: &ServiceContext,
    request: UploadCreateRequest,
) -> Result<UploadCreateResult, ApiError> {
    validate_create(&request)?;
    cleanup_expired_uploads(ctx).await?;
    let root = upload_root(ctx);
    ensure_private_dir(&root).await?;
    let upload_id = UploadId::new(format!("upl_{}", Uuid::new_v4().simple()));
    let now = Utc::now();
    let expires_at = now + Duration::hours(STAGING_TTL_HOURS);
    let mut metadata = request.metadata;
    if let Some(source_hint) = request.source_hint {
        metadata.insert("source_hint".to_string(), serde_json::json!(source_hint));
    }
    if let Some(source_id) = request.source_id {
        metadata.insert("source_id".to_string(), serde_json::json!(source_id.0));
    }
    let metadata = redact_upload_metadata(metadata)?;
    let record = UploadRecord {
        upload_id: upload_id.clone(),
        status: UploadStatusKind::Pending,
        filename: request.filename,
        content_type: request.content_type,
        size_bytes: request.size_bytes,
        bytes_received: 0,
        purpose: request.purpose,
        created_at: Timestamp::from(now),
        expires_at: Timestamp::from(expires_at),
        expected_sha256: request.sha256.map(normalize_sha256).transpose()?,
        sha256: None,
        artifact_id: None,
        source_ref: None,
        retention_until: None,
        metadata,
        abort_reason: None,
        audit_events: vec![audit_event("upload.create")],
    };
    save_record(&root, &record).await?;
    tracing::info!(event = "upload.create", upload_id = %upload_id.0, "upload staged");
    Ok(UploadCreateResult {
        put_url: format!("/v1/uploads/{}/content", upload_id.0),
        upload_id,
        expires_at: Timestamp::from(expires_at),
    })
}

pub async fn put_upload_content(
    ctx: &ServiceContext,
    upload_id: UploadId,
    bytes: Vec<u8>,
    supplied_content_type: Option<String>,
    supplied_sha256: Option<String>,
) -> Result<UploadStatus, ApiError> {
    validate_upload_id(&upload_id)?;
    if bytes.len() as u64 > MAX_UPLOAD_BYTES {
        return Err(upload_error(
            "upload.too_large",
            "upload exceeds the server size limit",
        ));
    }
    let root = upload_root(ctx);
    let _lock = acquire_lock(&root, &upload_id).await?;
    let mut record = load_record(&root, &upload_id).await?;
    ensure_active(&mut record, &root).await?;
    if !matches!(
        record.status,
        UploadStatusKind::Pending | UploadStatusKind::Received
    ) {
        return Err(upload_error(
            "upload.not_writable",
            "upload is not writable",
        ));
    }
    if bytes.len() as u64 != record.size_bytes {
        return Err(upload_error(
            "upload.size_mismatch",
            format!(
                "expected {} bytes, received {}",
                record.size_bytes,
                bytes.len()
            ),
        ));
    }
    if supplied_content_type
        .as_deref()
        .map(normalized_content_type)
        .is_some_and(|content_type| content_type != normalized_content_type(&record.content_type))
    {
        return Err(upload_error(
            "upload.content_type_mismatch",
            "uploaded content type does not match the staged declaration",
        ));
    }
    let digest = sha256_hex(&bytes);
    verify_hash(record.expected_sha256.as_deref(), &digest)?;
    if let Some(supplied) = supplied_sha256 {
        verify_hash(Some(&normalize_sha256(supplied)?), &digest)?;
    }
    atomic_write(&part_path(&root, &upload_id), &bytes).await?;
    record.status = UploadStatusKind::Received;
    record.bytes_received = bytes.len() as u64;
    record.sha256 = Some(digest);
    record.audit_events.push(audit_event("upload.content"));
    save_record(&root, &record).await?;
    tracing::info!(event = "upload.content", upload_id = %upload_id.0, bytes = bytes.len(), "upload content received");
    Ok(record.status())
}

pub async fn complete_upload(
    ctx: &ServiceContext,
    upload_id: UploadId,
    request: UploadCompleteRequest,
) -> Result<UploadCompleteResult, ApiError> {
    validate_upload_id(&upload_id)?;
    let root = upload_root(ctx);
    let _lock = acquire_lock(&root, &upload_id).await?;
    let mut record = load_record(&root, &upload_id).await?;
    ensure_active(&mut record, &root).await?;
    if record.status == UploadStatusKind::Completed {
        remove_if_exists(part_path(&root, &upload_id)).await?;
        return completed_result(&record);
    }
    if record.status != UploadStatusKind::Received {
        return Err(upload_error(
            "upload.incomplete",
            "upload content must be received before completion",
        ));
    }
    let bytes = tokio::fs::read(part_path(&root, &upload_id))
        .await
        .map_err(|error| io_error("upload.content_missing", error))?;
    if bytes.len() as u64 != record.size_bytes {
        return Err(upload_error(
            "upload.size_mismatch",
            "staged content size changed",
        ));
    }
    let digest = sha256_hex(&bytes);
    verify_hash(record.sha256.as_deref(), &digest)?;
    if let Some(expected) = request.sha256 {
        verify_hash(Some(&normalize_sha256(expected)?), &digest)?;
    }
    let retention_until = Utc::now() + Duration::days(ARTIFACT_RETENTION_DAYS);
    let mut metadata = record.metadata.clone();
    metadata.0.extend(request.source_options.0);
    metadata.insert(
        "filename".to_string(),
        serde_json::json!(record.filename.clone()),
    );
    metadata.insert(
        "upload_id".to_string(),
        serde_json::json!(upload_id.0.clone()),
    );
    metadata.insert("sha256".to_string(), serde_json::json!(digest));
    metadata.insert(
        "retention".to_string(),
        serde_json::json!({"policy": "upload", "retain_until": retention_until.to_rfc3339()}),
    );
    let store = artifact_store(ctx);
    let handle = store
        .put_bytes(ArtifactBytesWriteRequest {
            kind: ArtifactKind::RawContent,
            content_type: record.content_type.clone(),
            bytes,
            source_id: None,
            job_id: None,
            metadata,
        })
        .await?;
    let source_ref = format!("upload://{}", upload_id.0);
    record.status = UploadStatusKind::Completed;
    record.artifact_id = Some(handle.artifact_id.clone());
    record.source_ref = Some(source_ref.clone());
    record.retention_until = Some(Timestamp::from(retention_until));
    record.audit_events.push(audit_event("upload.complete"));
    if let Err(record_error) = save_record(&root, &record).await {
        if let Err(compensation_error) = store.delete(handle.clone()).await {
            return Err(upload_error(
                "upload.promotion_compensation_failed",
                format!(
                    "failed to commit completed upload ({record_error}); failed to remove promoted artifact ({compensation_error})"
                ),
            ));
        }
        return Err(record_error);
    }
    remove_if_exists(part_path(&root, &upload_id)).await?;
    tracing::info!(event = "upload.complete", upload_id = %upload_id.0, artifact_id = %handle.artifact_id.0, "upload promoted");
    Ok(UploadCompleteResult {
        upload_id,
        artifact_id: handle.artifact_id,
        source_ref,
        warnings: Vec::new(),
    })
}

pub async fn get_upload(
    ctx: &ServiceContext,
    upload_id: UploadId,
) -> Result<UploadStatus, ApiError> {
    validate_upload_id(&upload_id)?;
    let root = upload_root(ctx);
    let _lock = acquire_lock(&root, &upload_id).await?;
    let mut record = load_record(&root, &upload_id).await?;
    expire_if_needed(&mut record, &root).await?;
    expire_retained_artifact_if_needed(ctx, &mut record, &root).await?;
    Ok(record.status())
}

pub async fn list_uploads(
    ctx: &ServiceContext,
    request: UploadListRequest,
) -> Result<Page<UploadStatus>, ApiError> {
    cleanup_expired_uploads(ctx).await?;
    let root = upload_root(ctx);
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    if let Some(cursor) = request.cursor.as_deref() {
        validate_upload_id(&UploadId::new(cursor))?;
    }
    let index = load_upload_index(&root).await?;
    let (items, has_more) = index.page(request.cursor.as_deref(), request.status, limit as usize);
    let next_cursor = has_more.then(|| items.last().expect("non-empty page").upload_id.0.clone());
    Ok(Page {
        items,
        next_cursor,
        limit,
        total: None,
    })
}

pub async fn abort_upload(
    ctx: &ServiceContext,
    upload_id: UploadId,
    request: UploadAbortRequest,
) -> Result<UploadAbortResult, ApiError> {
    validate_upload_id(&upload_id)?;
    let root = upload_root(ctx);
    let _lock = acquire_lock(&root, &upload_id).await?;
    let mut record = load_record(&root, &upload_id).await?;
    if record.status == UploadStatusKind::Aborted {
        return Ok(UploadAbortResult {
            upload_id,
            deleted: true,
        });
    }
    if record.status == UploadStatusKind::Completed {
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
    }
    remove_if_exists(part_path(&root, &upload_id)).await?;
    record.status = UploadStatusKind::Aborted;
    record.abort_reason = request.reason.map(|reason| bounded_reason(&reason));
    record.artifact_id = None;
    record.source_ref = None;
    record.audit_events.push(audit_event("upload.abort"));
    save_record(&root, &record).await?;
    tracing::info!(event = "upload.abort", upload_id = %upload_id.0, "upload deleted");
    Ok(UploadAbortResult {
        upload_id,
        deleted: true,
    })
}

pub async fn resolve_upload_artifact(
    ctx: &ServiceContext,
    identity: &str,
) -> Result<Option<ArtifactReadResult>, ApiError> {
    let artifact_id = if identity.starts_with("upl_") {
        let status = get_upload(ctx, UploadId::new(identity)).await?;
        if status.status != UploadStatusKind::Completed {
            return Err(upload_error(
                "upload.not_completed",
                "staged upload is not completed",
            ));
        }
        status.artifact_id.ok_or_else(|| {
            upload_error(
                "upload.invalid_record",
                "completed upload has no artifact mapping",
            )
        })?
    } else {
        validate_artifact_id(identity)?;
        ArtifactId::new(identity)
    };
    let handle = ArtifactHandle {
        artifact_id,
        artifact_kind: ArtifactKind::RawContent,
        uri: None,
    };
    match artifact_store(ctx).get(handle).await {
        Ok(artifact) => Ok(Some(artifact)),
        Err(error) if error.code.0 == "artifact.not_found" => Ok(None),
        Err(error) => Err(error),
    }
}

pub async fn cleanup_expired_uploads(ctx: &ServiceContext) -> Result<u64, ApiError> {
    let root = upload_root(ctx);
    let index = load_upload_index(&root).await?;
    let due_upload_ids = index.due_upload_ids(Utc::now().timestamp_millis());
    let mut expired = 0;
    for upload_id in due_upload_ids {
        let _lock = acquire_lock(&root, &upload_id).await?;
        let mut record = load_record(&root, &upload_id).await?;
        if expire_if_needed(&mut record, &root).await?
            || expire_retained_artifact_if_needed(ctx, &mut record, &root).await?
        {
            expired += 1;
        }
    }
    Ok(expired)
}

fn artifact_store(ctx: &ServiceContext) -> FileArtifactStore {
    FileArtifactStore::new(ctx.cfg.output_dir.join("artifacts"))
}

fn completed_result(record: &UploadRecord) -> Result<UploadCompleteResult, ApiError> {
    Ok(UploadCompleteResult {
        upload_id: record.upload_id.clone(),
        artifact_id: record.artifact_id.clone().ok_or_else(|| {
            upload_error(
                "upload.invalid_record",
                "completed upload has no artifact mapping",
            )
        })?,
        source_ref: record.source_ref.clone().ok_or_else(|| {
            upload_error(
                "upload.invalid_record",
                "completed upload has no source reference",
            )
        })?,
        warnings: Vec::<SourceWarning>::new(),
    })
}

fn audit_event(event: &str) -> UploadAuditEvent {
    UploadAuditEvent {
        event: event.to_string(),
        at: Timestamp::from(Utc::now()),
    }
}

fn upload_error(code: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::new(code, ErrorStage::Publishing, message)
}

fn io_error(code: &'static str, error: std::io::Error) -> ApiError {
    upload_error(code, error.to_string())
}

#[cfg(test)]
#[path = "uploads_tests.rs"]
mod tests;
