use std::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDecision {
    AlreadySynced,
    UploadRevision,
    UploadNewSource,
}

pub type SyncResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub fn decide_sync(
    local_url: &str,
    local_hash: &str,
    existing: Option<(&str, &str)>,
) -> SyncResult<SyncDecision> {
    Ok(match existing {
        Some((url, hash)) if url == local_url && hash == local_hash => SyncDecision::AlreadySynced,
        Some((url, _)) if url == local_url => SyncDecision::UploadRevision,
        _ => SyncDecision::UploadNewSource,
    })
}
