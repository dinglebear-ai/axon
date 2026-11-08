//! Durable post-commit delivery intent for artifact candidates.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axon_adapters::artifact_candidate_batch_idempotency_key;
use axon_api::source::{ArtifactCandidate, JobId, SourceGenerationId, SourceId};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::sync::Mutex;

mod quota;
use quota::{bounded_encode, lock_disk_admission};
mod scan;
mod structure;
use scan::{decode_record, scan_paths};
const MAX_PENDING_DELIVERIES: usize = 1_024;
const MAX_OUTBOX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_OUTBOX_DISK_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingArtifactCandidateDelivery {
    pub(crate) delivery_key: String,
    pub(crate) job_id: JobId,
    pub(crate) source_id: SourceId,
    pub(crate) generation: SourceGenerationId,
    pub(crate) candidates: Vec<ArtifactCandidate>,
    pub(crate) staged_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArtifactCandidateOutboxFinding {
    pub(crate) code: &'static str,
    pub(crate) file_name: String,
}

pub(crate) struct ArtifactCandidateOutboxScan {
    pub(crate) deliveries: Vec<PendingArtifactCandidateDelivery>,
    pub(crate) findings: Vec<ArtifactCandidateOutboxFinding>,
    pub(crate) has_more: bool,
}

#[derive(Debug)]
pub(crate) struct ArtifactCandidateOutbox {
    root: PathBuf,
    gate: Mutex<()>,
    draining: AtomicBool,
    drain_requested: AtomicBool,
    drain_cancelled: AtomicBool,
    drain_task: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    scan_cursor: std::sync::Mutex<Option<PathBuf>>,
    quota_ready: Arc<AtomicBool>,
}

impl ArtifactCandidateOutbox {
    pub(crate) fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            gate: Mutex::new(()),
            draining: AtomicBool::new(false),
            drain_requested: AtomicBool::new(false),
            drain_cancelled: AtomicBool::new(false),
            drain_task: std::sync::Mutex::new(None),
            scan_cursor: std::sync::Mutex::new(None),
            quota_ready: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) async fn stage(
        &self,
        job_id: JobId,
        source_id: SourceId,
        generation: SourceGenerationId,
        mut candidates: Vec<ArtifactCandidate>,
    ) -> anyhow::Result<Option<PendingArtifactCandidateDelivery>> {
        if candidates.is_empty() {
            return Ok(None);
        }
        let started = std::time::Instant::now();
        candidates.sort_by(|left, right| left.id.cmp(&right.id));
        let delivery_key =
            artifact_candidate_batch_idempotency_key(&job_id, &source_id, &generation, &candidates);
        let pending = PendingArtifactCandidateDelivery {
            delivery_key,
            job_id,
            source_id,
            generation,
            candidates,
            staged_at_unix_ms: now_unix_ms()?,
        };
        let _guard = self.gate.lock().await;
        create_private_directory(&self.root).await?;
        let path = self.path(&pending.delivery_key);
        match tokio::fs::symlink_metadata(&path).await {
            Ok(metadata) => {
                anyhow::ensure!(
                    metadata.is_file()
                        && !metadata.file_type().is_symlink()
                        && metadata.len() <= MAX_OUTBOX_FILE_BYTES,
                    "existing outbox delivery is not a bounded regular file"
                );
                return Ok(Some(pending));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let (pending, bytes) = tokio::task::spawn_blocking(move || {
            let bytes = bounded_encode(&pending, MAX_OUTBOX_FILE_BYTES)?;
            structure::charge(&bytes).map_err(|_| {
                anyhow::anyhow!("artifact candidate outbox decoded structure budget exceeded")
            })?;
            Ok::<_, anyhow::Error>((pending, bytes))
        })
        .await??;
        let _disk_owner = lock_disk_admission(&self.root)?;
        let root = self.root.clone();
        let quota_ready = self.quota_ready.clone();
        tokio::task::spawn_blocking(move || {
            use std::io::Write as _;
            let _owner = _disk_owner;
            let mut quota = quota::open(&root, !quota_ready.load(Ordering::Acquire))?;
            quota_ready.store(true, Ordering::Release);
            quota::reserve(
                &mut quota,
                &path,
                bytes.len() as u64,
                MAX_OUTBOX_DISK_BYTES - quota::DISK_OVERHEAD,
            )?;
            let mut temporary = tempfile::NamedTempFile::new_in(&root)?;
            temporary.write_all(&bytes)?;
            temporary.as_file().sync_all()?;
            temporary.persist(path).map_err(|error| error.error)?;
            std::fs::File::open(root)?.sync_all()?;
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        tracing::info!(job_id=?pending.job_id, candidates=pending.candidates.len(), elapsed_us=started.elapsed().as_micros() as u64, "source outbox page staged");
        Ok(Some(pending))
    }

    #[cfg(test)]
    pub(crate) async fn pending(&self) -> anyhow::Result<Vec<PendingArtifactCandidateDelivery>> {
        Ok(self.scan().await?.deliveries)
    }

    pub(crate) async fn scan(&self) -> anyhow::Result<ArtifactCandidateOutboxScan> {
        self.scan_with_budget(MAX_OUTBOX_FILE_BYTES).await
    }

    async fn scan_with_budget(&self, budget: u64) -> anyhow::Result<ArtifactCandidateOutboxScan> {
        self.scan_with_limits(budget, structure::limit()).await
    }

    async fn scan_with_limits(
        &self,
        budget: u64,
        structure_budget: u64,
    ) -> anyhow::Result<ArtifactCandidateOutboxScan> {
        let _guard = self.gate.lock().await;
        if !tokio::fs::try_exists(&self.root).await? {
            return Ok(scan::empty_scan());
        }
        let cursor = self.scan_cursor.lock().expect("outbox cursor lock").clone();
        let (paths, mut has_more) =
            scan_paths(&self.root, cursor.as_ref(), self.quota_ready.clone()).await?;
        let mut admitted_bytes = 0_u64;
        let mut structural_bytes = 0_u64;
        let mut last_visited = cursor;
        let mut pending: Vec<PendingArtifactCandidateDelivery> = Vec::new();
        let mut findings = Vec::new();
        for path in paths {
            let previous_visited = last_visited.replace(path.clone());
            let file_name = path.file_name().map_or_else(
                || "<unknown>".to_string(),
                |value| value.to_string_lossy().into_owned(),
            );
            let Some(file_key) = path.file_stem().and_then(|value| value.to_str()) else {
                findings.push(
                    quarantine(&path, "artifact_candidate.outbox.invalid_name", file_name).await,
                );
                continue;
            };
            if !valid_file_key(file_key) {
                findings.push(
                    quarantine(&path, "artifact_candidate.outbox.invalid_name", file_name).await,
                );
                continue;
            }
            let metadata = match tokio::fs::symlink_metadata(&path).await {
                Ok(metadata) => metadata,
                Err(_) => {
                    findings.push(finding(
                        "artifact_candidate.outbox.metadata_failed",
                        file_name,
                    ));
                    continue;
                }
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                findings.push(finding("artifact_candidate.outbox.invalid_file", file_name));
                continue;
            }
            if metadata.len() > MAX_OUTBOX_FILE_BYTES {
                findings.push(
                    quarantine(&path, "artifact_candidate.outbox.oversized", file_name).await,
                );
                continue;
            }
            // Admit one legacy-sized file even with a smaller test budget;
            // subsequent records may not exceed the aggregate scan window.
            if admitted_bytes > 0 && metadata.len() > budget.saturating_sub(admitted_bytes) {
                last_visited = previous_visited;
                has_more = true;
                break;
            }
            let bytes = match read_bounded(&path).await {
                Ok(bytes) => bytes,
                Err(_) => {
                    findings.push(finding("artifact_candidate.outbox.read_failed", file_name));
                    continue;
                }
            };
            if admitted_bytes > 0 && bytes.len() as u64 > budget.saturating_sub(admitted_bytes) {
                last_visited = previous_visited;
                has_more = true;
                break;
            }
            admitted_bytes = admitted_bytes.saturating_add(bytes.len() as u64);
            let remaining_structure = structure_budget.saturating_sub(structural_bytes);
            let decoded = decode_record(bytes, remaining_structure).await?;
            let (record, charged) = match decoded {
                Ok(Some(value)) => value,
                Ok(None) => {
                    last_visited = previous_visited;
                    has_more = true;
                    break;
                }
                Err(structure::Rejected::Complexity) => {
                    // May be a legacy whole-generation delivery. Preserve it
                    // for a bounded migration rather than silently deleting evidence.
                    findings.push(finding(
                        "artifact_candidate.outbox.structure_limit",
                        file_name,
                    ));
                    continue;
                }
                Err(structure::Rejected::Corrupt) => {
                    findings.push(
                        quarantine(&path, "artifact_candidate.outbox.corrupt", file_name).await,
                    );
                    continue;
                }
            };
            structural_bytes += charged;
            if !scan::record_matches_file(&record, file_key) {
                findings.push(
                    quarantine(
                        &path,
                        "artifact_candidate.outbox.integrity_failed",
                        file_name,
                    )
                    .await,
                );
                continue;
            }
            pending.push(record);
        }
        *self.scan_cursor.lock().expect("outbox cursor lock") =
            if has_more { last_visited } else { None };
        pending.sort_by(|left, right| left.delivery_key.cmp(&right.delivery_key));
        Ok(ArtifactCandidateOutboxScan {
            deliveries: pending,
            findings,
            has_more,
        })
    }

    pub(crate) async fn complete(&self, delivery_key: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            file_key_for(delivery_key).is_some(),
            "invalid artifact candidate delivery key"
        );
        let _guard = self.gate.lock().await;
        let path = self.path(delivery_key);
        if !tokio::fs::try_exists(&self.root).await? {
            return Ok(());
        }
        let root = self.root.clone();
        let ready = self.quota_ready.clone();
        tokio::task::spawn_blocking(move || {
            let _owner = lock_disk_admission(&root)?;
            let mut quota = quota::open(&root, !ready.load(Ordering::Acquire))?;
            ready.store(true, Ordering::Release);
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            // Sync removal before freeing its reservation; a crash may overcount, never undercount.
            std::fs::File::open(&root)?.sync_all()?;
            quota::release(&mut quota, &path)?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }

    pub(crate) fn begin_drain(&self) -> bool {
        self.drain_cancelled.store(false, Ordering::Release);
        self.drain_requested.store(true, Ordering::Release);
        self.draining
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(crate) fn register_drain_task(&self, task: tokio::task::JoinHandle<()>) {
        *self.drain_task.lock().expect("outbox drain task lock") = Some(task);
    }

    pub(crate) fn drain_cancelled(&self) -> bool {
        self.drain_cancelled.load(Ordering::Acquire)
    }

    pub(crate) async fn shutdown_drain(&self) {
        self.drain_cancelled.store(true, Ordering::Release);
        self.drain_requested.store(false, Ordering::Release);
        let task = self
            .drain_task
            .lock()
            .expect("outbox drain task lock")
            .take();
        if let Some(task) = task {
            task.abort();
            let _ = task.await;
        }
        self.draining.store(false, Ordering::Release);
    }

    pub(crate) fn start_drain_pass(&self) {
        *self.scan_cursor.lock().expect("outbox cursor lock") = None;
        self.drain_requested.store(false, Ordering::Release);
    }

    pub(crate) fn continue_or_finish_drain(&self) -> bool {
        if self.drain_requested.swap(false, Ordering::AcqRel) {
            return true;
        }
        self.draining.store(false, Ordering::Release);
        if self.drain_requested.load(Ordering::Acquire)
            && self
                .draining
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            self.drain_requested.store(false, Ordering::Release);
            return true;
        }
        false
    }

    /// Finish an exhausted retry cycle while preserving a request queued during
    /// its final pass. `true` keeps the current supervised runner responsible
    /// for that request; `false` releases ownership before the task exits.
    pub(crate) fn finish_exhausted_drain(&self) -> bool {
        self.continue_or_finish_drain()
    }

    fn path(&self, delivery_key: &str) -> PathBuf {
        let file_key = file_key_for(delivery_key).expect("validated delivery key");
        self.root.join(format!("{file_key}.json"))
    }
}

async fn read_bounded(path: &PathBuf) -> anyhow::Result<Vec<u8>> {
    let mut options = tokio::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let file = options.open(path).await?;
    anyhow::ensure!(
        file.metadata().await?.is_file(),
        "outbox entry must be a regular file"
    );
    let mut bytes = Vec::new();
    file.take(MAX_OUTBOX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_OUTBOX_FILE_BYTES,
        "outbox entry exceeds byte limit"
    );
    Ok(bytes)
}

fn valid_file_key(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn file_key_for(value: &str) -> Option<&str> {
    value
        .strip_prefix("sha256:")
        .filter(|key| valid_file_key(key))
}

fn now_unix_ms() -> anyhow::Result<u64> {
    Ok(u64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
}

fn finding(code: &'static str, file_name: String) -> ArtifactCandidateOutboxFinding {
    metrics::counter!("axon_artifact_candidate_outbox_findings_total", "code" => code).increment(1);
    tracing::warn!(code, file_name, "artifact candidate outbox scan finding");
    ArtifactCandidateOutboxFinding { code, file_name }
}

async fn quarantine(
    path: &Path,
    code: &'static str,
    file_name: String,
) -> ArtifactCandidateOutboxFinding {
    let quarantine = path.with_extension(format!("invalid.{}", uuid::Uuid::new_v4()));
    let path = path.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        quota::quarantine_file(
            &path,
            &quarantine,
            MAX_OUTBOX_DISK_BYTES - quota::DISK_OVERHEAD,
        )
    })
    .await;
    if !matches!(result, Ok(Ok(()))) {
        return finding("artifact_candidate.outbox.quarantine_failed", file_name);
    }
    finding(code, file_name)
}

async fn create_private_directory(path: &PathBuf) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(path).await?;
    let metadata = tokio::fs::symlink_metadata(path).await?;
    anyhow::ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "outbox root must be a private regular directory"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await?;
    }
    Ok(())
}

pub(crate) type SharedArtifactCandidateOutbox = Arc<ArtifactCandidateOutbox>;

#[cfg(test)]
#[path = "artifact_candidate_outbox_tests.rs"]
mod tests;
