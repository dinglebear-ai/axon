use super::validation::validate_upload_id;
use super::{UploadRecord, audit_event, io_error, upload_error};
use crate::context::ServiceContext;
use axon_api::source::{ApiError, UploadId, UploadStatusKind};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use tokio::time::{Duration, sleep};
use uuid::Uuid;

pub(super) fn upload_root(ctx: &ServiceContext) -> PathBuf {
    ctx.cfg.output_dir.join("artifacts").join("uploads")
}

fn record_path(root: &Path, upload_id: &UploadId) -> PathBuf {
    root.join(format!("{}.json", upload_id.0))
}

pub(super) fn part_path(root: &Path, upload_id: &UploadId) -> PathBuf {
    root.join(format!("{}.part", upload_id.0))
}

pub(super) async fn save_record(root: &Path, record: &UploadRecord) -> Result<(), ApiError> {
    ensure_private_dir(root).await?;
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|error| upload_error("upload.write_failed", error.to_string()))?;
    atomic_write(&record_path(root, &record.upload_id), &bytes).await
}

pub(super) async fn load_record(
    root: &Path,
    upload_id: &UploadId,
) -> Result<UploadRecord, ApiError> {
    load_record_path(&record_path(root, upload_id)).await
}

pub(super) async fn load_record_path(path: &Path) -> Result<UploadRecord, ApiError> {
    let bytes = tokio::fs::read(path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            upload_error("upload.not_found", "upload not found")
        } else {
            io_error("upload.read_failed", error)
        }
    })?;
    let record: UploadRecord = serde_json::from_slice(&bytes)
        .map_err(|error| upload_error("upload.invalid_record", error.to_string()))?;
    validate_upload_id(&record.upload_id)?;
    if path.file_stem().and_then(|value| value.to_str()) != Some(record.upload_id.0.as_str()) {
        return Err(upload_error(
            "upload.invalid_record",
            "upload record identity mismatch",
        ));
    }
    Ok(record)
}

pub(super) async fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ApiError> {
    let parent = path
        .parent()
        .ok_or_else(|| upload_error("upload.invalid_path", "missing upload root"))?;
    ensure_private_dir(parent).await?;
    let temp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("upload"),
        Uuid::new_v4().simple()
    ));
    tokio::fs::write(&temp, bytes)
        .await
        .map_err(|error| io_error("upload.write_failed", error))?;
    tokio::fs::rename(&temp, path)
        .await
        .map_err(|error| io_error("upload.write_failed", error))
}

pub(super) async fn ensure_private_dir(path: &Path) -> Result<(), ApiError> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|error| io_error("upload.write_failed", error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|error| io_error("upload.write_failed", error))?;
    }
    Ok(())
}

pub(super) struct UploadLock(PathBuf);

impl Drop for UploadLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub(super) async fn acquire_lock(
    root: &Path,
    upload_id: &UploadId,
) -> Result<UploadLock, ApiError> {
    ensure_private_dir(root).await?;
    let path = root.join(format!("{}.lock", upload_id.0));
    for _ in 0..50 {
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await
        {
            Ok(_) => return Ok(UploadLock(path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if lock_is_stale(&path).await {
                    let _ = tokio::fs::remove_file(&path).await;
                    continue;
                }
                sleep(Duration::from_millis(10)).await;
            }
            Err(error) => return Err(io_error("upload.lock_failed", error)),
        }
    }
    Err(upload_error(
        "upload.busy",
        "upload is busy; retry the operation",
    ))
}

async fn lock_is_stale(path: &Path) -> bool {
    tokio::fs::metadata(path)
        .await
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > Duration::from_secs(300))
}

pub(super) async fn ensure_active(record: &mut UploadRecord, root: &Path) -> Result<(), ApiError> {
    if expire_if_needed(record, root).await? {
        return Err(upload_error("upload.expired", "upload has expired"));
    }
    if matches!(
        record.status,
        UploadStatusKind::Aborted | UploadStatusKind::Expired
    ) {
        return Err(upload_error(
            "upload.not_writable",
            "upload is no longer active",
        ));
    }
    Ok(())
}

pub(super) async fn expire_if_needed(
    record: &mut UploadRecord,
    root: &Path,
) -> Result<bool, ApiError> {
    if matches!(
        record.status,
        UploadStatusKind::Completed | UploadStatusKind::Aborted | UploadStatusKind::Expired
    ) {
        return Ok(false);
    }
    let expires = DateTime::parse_from_rfc3339(&record.expires_at.0)
        .map_err(|error| upload_error("upload.invalid_record", error.to_string()))?
        .with_timezone(&Utc);
    if expires > Utc::now() {
        return Ok(false);
    }
    remove_if_exists(part_path(root, &record.upload_id)).await?;
    record.status = UploadStatusKind::Expired;
    record.audit_events.push(audit_event("upload.expire"));
    save_record(root, record).await?;
    tracing::info!(event = "upload.expire", upload_id = %record.upload_id.0, "staged upload expired");
    Ok(true)
}

pub(super) async fn remove_if_exists(path: PathBuf) -> Result<(), ApiError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("upload.delete_failed", error)),
    }
}
