#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDecision {
    AlreadySynced,
    UploadRevision,
    UploadNewSource,
}

pub fn decide_sync(
    local_url: &str,
    local_hash: &str,
    existing: Option<(&str, &str)>,
) -> SyncDecision {
    match existing {
        Some((url, hash)) if url == local_url && hash == local_hash => SyncDecision::AlreadySynced,
        Some((url, _)) if url == local_url => SyncDecision::UploadRevision,
        _ => SyncDecision::UploadNewSource,
    }
}
