use super::sync::{SyncDecision, decide_sync};

#[test]
fn same_url_same_hash_marks_synced_without_upload() {
    let decision = decide_sync(
        "https://example.com/a",
        "hash-1",
        Some(("https://example.com/a", "hash-1")),
    )
    .expect("sync decision");

    assert_eq!(decision, SyncDecision::AlreadySynced);
}

#[test]
fn same_url_different_hash_uploads_revision() {
    let decision = decide_sync(
        "https://example.com/a",
        "hash-2",
        Some(("https://example.com/a", "hash-1")),
    )
    .expect("sync decision");

    assert_eq!(decision, SyncDecision::UploadRevision);
}
