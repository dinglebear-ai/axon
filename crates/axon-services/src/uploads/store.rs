use super::validation::validate_upload_id;
use super::{UploadRecord, audit_event, io_error, upload_error};
use crate::context::ServiceContext;
use axon_api::source::{ApiError, UploadId, UploadStatusKind};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tokio::time::{Duration, sleep};

#[cfg(test)]
use std::sync::{Mutex, OnceLock};

pub(super) const INDEX_FILE: &str = ".upload-index.json";
const INDEX_DIRTY_FILE: &str = ".upload-index.dirty";
const INDEX_LOCK_FILE: &str = ".upload-index.lock";

#[cfg(test)]
static FAIL_RECORD_SAVE: OnceLock<Mutex<Option<String>>> = OnceLock::new();

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct UploadIndex {
    by_id: Vec<axon_api::source::UploadStatus>,
    by_status: BTreeMap<String, Vec<UploadId>>,
    by_expiry: Vec<UploadExpiryEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UploadExpiryEntry {
    deadline_millis: i64,
    upload_id: UploadId,
}

impl UploadIndex {
    fn from_records(records: Vec<UploadRecord>) -> Result<Self, ApiError> {
        let mut index = Self {
            by_id: records.iter().map(UploadRecord::status).collect(),
            ..Self::default()
        };
        index.reindex()?;
        Ok(index)
    }

    fn upsert(&mut self, record: &UploadRecord) -> Result<(), ApiError> {
        match self
            .by_id
            .binary_search_by(|status| status.upload_id.cmp(&record.upload_id))
        {
            Ok(position) => self.by_id[position] = record.status(),
            Err(position) => self.by_id.insert(position, record.status()),
        }
        self.reindex_from_statuses()?;
        Ok(())
    }

    fn reindex(&mut self) -> Result<(), ApiError> {
        self.by_id
            .sort_by(|left, right| left.upload_id.cmp(&right.upload_id));
        self.reindex_from_statuses()
    }

    fn reindex_from_statuses(&mut self) -> Result<(), ApiError> {
        self.by_status.clear();
        self.by_expiry.clear();
        for status in &self.by_id {
            self.by_status
                .entry(status_key(status.status).to_string())
                .or_default()
                .push(status.upload_id.clone());
            let deadline = match status.status {
                UploadStatusKind::Pending | UploadStatusKind::Received => Some(&status.expires_at),
                UploadStatusKind::Completed => status.retention_until.as_ref(),
                UploadStatusKind::Aborted | UploadStatusKind::Expired => None,
            };
            if let Some(deadline) = deadline {
                self.by_expiry.push(UploadExpiryEntry {
                    deadline_millis: timestamp_millis(deadline)?,
                    upload_id: status.upload_id.clone(),
                });
            }
        }
        self.by_expiry.sort_by(|left, right| {
            left.deadline_millis
                .cmp(&right.deadline_millis)
                .then_with(|| left.upload_id.cmp(&right.upload_id))
        });
        Ok(())
    }

    pub(super) fn page(
        &self,
        cursor: Option<&str>,
        status: Option<UploadStatusKind>,
        limit: usize,
    ) -> (Vec<axon_api::source::UploadStatus>, bool) {
        let ids = status.and_then(|status| self.by_status.get(status_key(status)));
        let start = ids.map_or_else(
            || {
                self.by_id.partition_point(|entry| {
                    cursor.is_some_and(|cursor| entry.upload_id.0.as_str() <= cursor)
                })
            },
            |ids| {
                ids.partition_point(|upload_id| {
                    cursor.is_some_and(|cursor| upload_id.0.as_str() <= cursor)
                })
            },
        );
        let items = if let Some(ids) = ids {
            ids[start..]
                .iter()
                .take(limit + 1)
                .filter_map(|upload_id| {
                    self.by_id
                        .binary_search_by(|entry| entry.upload_id.cmp(upload_id))
                        .ok()
                        .map(|position| self.by_id[position].clone())
                })
                .collect::<Vec<_>>()
        } else {
            self.by_id[start..]
                .iter()
                .take(limit + 1)
                .cloned()
                .collect::<Vec<_>>()
        };
        let has_more = items.len() > limit;
        (items.into_iter().take(limit).collect(), has_more)
    }

    pub(super) fn due_upload_ids(&self, now_millis: i64) -> Vec<UploadId> {
        self.by_expiry
            .iter()
            .take_while(|entry| entry.deadline_millis <= now_millis)
            .map(|entry| entry.upload_id.clone())
            .collect()
    }
}

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
    let _index_lock = acquire_named_lock(root, INDEX_LOCK_FILE).await?;
    let mut index = load_or_rebuild_index_locked(root).await?;
    #[cfg(test)]
    if should_fail_record_save(&record.upload_id) {
        return Err(upload_error(
            "upload.test_record_save_failed",
            "injected upload record save failure",
        ));
    }
    let dirty_path = root.join(INDEX_DIRTY_FILE);
    tokio::fs::write(&dirty_path, b"dirty")
        .await
        .map_err(|error| io_error("upload.write_failed", error))?;
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|error| upload_error("upload.write_failed", error.to_string()))?;
    if let Err(error) = atomic_write(&record_path(root, &record.upload_id), &bytes).await {
        let _ = tokio::fs::remove_file(&dirty_path).await;
        return Err(error);
    }
    if let Err(error) = index.upsert(record) {
        tracing::warn!(upload_id = %record.upload_id.0, %error, "upload index update deferred");
        return Ok(());
    }
    if let Err(error) = save_index_locked(root, &index).await {
        tracing::warn!(upload_id = %record.upload_id.0, %error, "upload index write deferred");
        return Ok(());
    }
    if let Err(error) = remove_if_exists(dirty_path).await {
        tracing::warn!(upload_id = %record.upload_id.0, %error, "upload index remains marked dirty");
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn fail_next_record_save(upload_id: &UploadId) {
    *FAIL_RECORD_SAVE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("upload record failpoint mutex poisoned") = Some(upload_id.0.clone());
}

#[cfg(test)]
fn should_fail_record_save(upload_id: &UploadId) -> bool {
    let mut target = FAIL_RECORD_SAVE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("upload record failpoint mutex poisoned");
    if target.as_deref() == Some(upload_id.0.as_str()) {
        target.take();
        true
    } else {
        false
    }
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
    axon_core::artifacts::atomic_write_explicit(path, bytes)
        .await
        .map(|_| ())
        .map_err(|error| upload_error("upload.write_failed", error.to_string()))
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
    acquire_named_lock(root, &format!("{}.lock", upload_id.0)).await
}

async fn acquire_named_lock(root: &Path, file_name: &str) -> Result<UploadLock, ApiError> {
    ensure_private_dir(root).await?;
    let path = root.join(file_name);
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

pub(super) async fn load_upload_index(root: &Path) -> Result<UploadIndex, ApiError> {
    match tokio::fs::metadata(root).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(UploadIndex::default());
        }
        Err(error) => return Err(io_error("upload.list_failed", error)),
    }
    let _lock = acquire_named_lock(root, INDEX_LOCK_FILE).await?;
    load_or_rebuild_index_locked(root).await
}

async fn load_or_rebuild_index_locked(root: &Path) -> Result<UploadIndex, ApiError> {
    if !root.join(INDEX_DIRTY_FILE).exists() {
        match tokio::fs::read(root.join(INDEX_FILE)).await {
            Ok(bytes) => {
                if let Ok(index) = serde_json::from_slice(&bytes) {
                    return Ok(index);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error("upload.list_failed", error)),
        }
    }
    let index = rebuild_index_locked(root).await?;
    save_index_locked(root, &index).await?;
    remove_if_exists(root.join(INDEX_DIRTY_FILE)).await?;
    Ok(index)
}

async fn rebuild_index_locked(root: &Path) -> Result<UploadIndex, ApiError> {
    let mut entries = tokio::fs::read_dir(root)
        .await
        .map_err(|error| io_error("upload.list_failed", error))?;
    let mut records = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| io_error("upload.list_failed", error))?
    {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json")
            || !path
                .file_stem()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.starts_with("upl_"))
        {
            continue;
        }
        records.push(load_record_path(&path).await?);
    }
    UploadIndex::from_records(records)
}

async fn save_index_locked(root: &Path, index: &UploadIndex) -> Result<(), ApiError> {
    let bytes = serde_json::to_vec(index)
        .map_err(|error| upload_error("upload.write_failed", error.to_string()))?;
    atomic_write(&root.join(INDEX_FILE), &bytes).await
}

fn status_key(status: UploadStatusKind) -> &'static str {
    match status {
        UploadStatusKind::Pending => "pending",
        UploadStatusKind::Received => "received",
        UploadStatusKind::Completed => "completed",
        UploadStatusKind::Aborted => "aborted",
        UploadStatusKind::Expired => "expired",
    }
}

fn timestamp_millis(timestamp: &axon_api::source::Timestamp) -> Result<i64, ApiError> {
    DateTime::parse_from_rfc3339(&timestamp.0)
        .map(|value| value.timestamp_millis())
        .map_err(|error| upload_error("upload.invalid_record", error.to_string()))
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
