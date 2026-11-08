//! Bounded lexical inventory and structural decoding for outbox scans.

use super::*;

pub(super) async fn scan_paths(
    owner: OperationOwner,
    outbox: &ArtifactCandidateOutbox,
    cursor: Option<&PathBuf>,
) -> anyhow::Result<(std::collections::BTreeSet<PathBuf>, bool)> {
    let root = outbox.root.clone();
    let ready = outbox.quota_ready.clone();
    let cursor = cursor.cloned();
    blocking(owner, move || {
        // Ownership follows the blocking task through cancellation. Reconcile
        // legacy files once on first access; all current writers reserve names
        // under this same disk fence before making delivery files visible.
        let _owner = lock_disk_admission(&root)?;
        #[cfg(test)]
        pause_scan_for_test(&root);
        let db = quota::open(&root, !ready.load(Ordering::Acquire))?;
        ready.store(true, Ordering::Release);
        let cursor_name = cursor
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let (names, has_more) = quota::inventory_page(&db, cursor_name, MAX_PENDING_DELIVERIES)?;
        let paths = names.into_iter().map(|name| root.join(name)).collect();
        Ok((paths, has_more))
    })
    .await?
}

pub(super) async fn admitted_metadata(
    owner: OperationOwner,
    path: &Path,
    file_name: String,
) -> Result<std::fs::Metadata, ArtifactCandidateOutboxFinding> {
    let metadata = tokio::fs::symlink_metadata(path).await.map_err(|_| {
        finding(
            "artifact_candidate.outbox.metadata_failed",
            file_name.clone(),
        )
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(finding("artifact_candidate.outbox.invalid_file", file_name));
    }
    if metadata.len() > MAX_OUTBOX_FILE_BYTES {
        return Err(quarantine(
            owner,
            path,
            "artifact_candidate.outbox.oversized",
            file_name,
        )
        .await);
    }
    Ok(metadata)
}

pub(super) async fn decode_record(
    owner: OperationOwner,
    bytes: Vec<u8>,
    remaining_structure: u64,
) -> anyhow::Result<Result<Option<(PendingArtifactCandidateDelivery, u64)>, structure::Rejected>> {
    blocking(owner, move || {
        let charged = structure::charge(&bytes)?;
        if charged > remaining_structure {
            return Ok(None);
        }
        let record = serde_json::from_slice::<PendingArtifactCandidateDelivery>(&bytes)
            .map_err(|_| structure::Rejected::Corrupt)?;
        Ok::<_, structure::Rejected>(Some((record, charged)))
    })
    .await
    .map_err(Into::into)
}

pub(super) fn record_matches_file(
    record: &PendingArtifactCandidateDelivery,
    file_key: &str,
) -> bool {
    let recomputed = artifact_candidate_batch_idempotency_key(
        &record.job_id,
        &record.source_id,
        &record.generation,
        &record.candidates,
    );
    file_key_for(&record.delivery_key) == Some(file_key) && record.delivery_key == recomputed
}

pub(super) fn empty_scan() -> ArtifactCandidateOutboxScan {
    ArtifactCandidateOutboxScan {
        deliveries: Vec::new(),
        findings: Vec::new(),
        has_more: false,
    }
}

// Files belong to one test's private temporary root, never a global hook.
#[cfg(test)]
fn pause_scan_for_test(root: &Path) {
    if !root.join(".pause-scan-test").exists() {
        return;
    }
    std::fs::write(root.join(".scan-started-test"), b"").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !root.join(".release-scan-test").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "test scan release timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
