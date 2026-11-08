//! Bounded lexical inventory and structural decoding for outbox scans.

use super::*;

pub(super) async fn scan_paths(
    root: &std::path::Path,
    cursor: Option<&PathBuf>,
    ready: Arc<AtomicBool>,
) -> anyhow::Result<(std::collections::BTreeSet<PathBuf>, bool)> {
    let root = root.to_path_buf();
    let cursor = cursor.cloned();
    tokio::task::spawn_blocking(move || {
        // Ownership follows the blocking task through cancellation. Reconcile
        // legacy files once on first access; all current writers reserve names
        // under this same disk fence before making delivery files visible.
        let _owner = lock_disk_admission(&root)?;
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

pub(super) async fn decode_record(
    bytes: Vec<u8>,
    remaining_structure: u64,
) -> anyhow::Result<Result<Option<(PendingArtifactCandidateDelivery, u64)>, structure::Rejected>> {
    tokio::task::spawn_blocking(move || {
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
