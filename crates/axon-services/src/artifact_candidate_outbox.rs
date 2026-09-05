//! Durable post-commit delivery intent for artifact candidates.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axon_adapters::artifact_candidate_batch_idempotency_key;
use axon_api::source::{ArtifactCandidate, JobId, SourceGenerationId, SourceId};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

const MAX_PENDING_DELIVERIES: usize = 1_024;
const MAX_OUTBOX_FILE_BYTES: u64 = 64 * 1024 * 1024;

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
}

#[derive(Debug)]
pub(crate) struct ArtifactCandidateOutbox {
    root: PathBuf,
    gate: Mutex<()>,
    draining: AtomicBool,
    drain_requested: AtomicBool,
    drain_cancelled: AtomicBool,
    drain_task: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    scan_cursor: std::sync::atomic::AtomicUsize,
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
            scan_cursor: std::sync::atomic::AtomicUsize::new(0),
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
        if tokio::fs::try_exists(&path).await? {
            return Ok(Some(pending));
        }
        let bytes = serde_json::to_vec(&pending)?;
        let temporary = self.root.join(format!(
            ".{}.{}.tmp",
            file_key_for(&pending.delivery_key).expect("generated delivery key"),
            uuid::Uuid::new_v4()
        ));
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).await?;
        file.write_all(&bytes).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(&temporary, &path).await?;
        sync_directory(self.root.clone()).await?;
        Ok(Some(pending))
    }

    #[cfg(test)]
    pub(crate) async fn pending(&self) -> anyhow::Result<Vec<PendingArtifactCandidateDelivery>> {
        Ok(self.scan().await?.deliveries)
    }

    pub(crate) async fn scan(&self) -> anyhow::Result<ArtifactCandidateOutboxScan> {
        let _guard = self.gate.lock().await;
        if !tokio::fs::try_exists(&self.root).await? {
            return Ok(ArtifactCandidateOutboxScan {
                deliveries: Vec::new(),
                findings: Vec::new(),
            });
        }
        let mut entries = tokio::fs::read_dir(&self.root).await?;
        let mut paths = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("json") {
                paths.push(path);
            }
        }
        paths.sort();
        if paths.len() > MAX_PENDING_DELIVERIES {
            let start = self
                .scan_cursor
                .fetch_add(MAX_PENDING_DELIVERIES, Ordering::AcqRel)
                % paths.len();
            paths.rotate_left(start);
            paths.truncate(MAX_PENDING_DELIVERIES);
        }
        let mut pending: Vec<PendingArtifactCandidateDelivery> = Vec::new();
        let mut findings = Vec::new();
        for path in paths {
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
            let metadata = match tokio::fs::metadata(&path).await {
                Ok(metadata) => metadata,
                Err(_) => {
                    findings.push(finding(
                        "artifact_candidate.outbox.metadata_failed",
                        file_name,
                    ));
                    continue;
                }
            };
            if metadata.len() > MAX_OUTBOX_FILE_BYTES {
                findings.push(
                    quarantine(&path, "artifact_candidate.outbox.oversized", file_name).await,
                );
                continue;
            }
            let bytes = match tokio::fs::read(&path).await {
                Ok(bytes) => bytes,
                Err(_) => {
                    findings.push(finding("artifact_candidate.outbox.read_failed", file_name));
                    continue;
                }
            };
            let Ok(record) = serde_json::from_slice::<PendingArtifactCandidateDelivery>(&bytes)
            else {
                findings
                    .push(quarantine(&path, "artifact_candidate.outbox.corrupt", file_name).await);
                continue;
            };
            let recomputed = artifact_candidate_batch_idempotency_key(
                &record.job_id,
                &record.source_id,
                &record.generation,
                &record.candidates,
            );
            if file_key_for(&record.delivery_key) != Some(file_key)
                || record.delivery_key != recomputed
            {
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
        pending.sort_by(|left, right| left.delivery_key.cmp(&right.delivery_key));
        Ok(ArtifactCandidateOutboxScan {
            deliveries: pending,
            findings,
        })
    }

    pub(crate) async fn complete(&self, delivery_key: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            file_key_for(delivery_key).is_some(),
            "invalid artifact candidate delivery key"
        );
        let _guard = self.gate.lock().await;
        let path = self.path(delivery_key);
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
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

    fn path(&self, delivery_key: &str) -> PathBuf {
        let file_key = file_key_for(delivery_key).expect("validated delivery key");
        self.root.join(format!("{file_key}.json"))
    }
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
    path: &PathBuf,
    code: &'static str,
    file_name: String,
) -> ArtifactCandidateOutboxFinding {
    let quarantine = path.with_extension(format!("invalid.{}", uuid::Uuid::new_v4()));
    if tokio::fs::rename(path, quarantine).await.is_err() {
        return finding("artifact_candidate.outbox.quarantine_failed", file_name);
    }
    finding(code, file_name)
}

async fn create_private_directory(path: &PathBuf) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(path).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await?;
    }
    Ok(())
}

async fn sync_directory(path: PathBuf) -> anyhow::Result<()> {
    tokio::task::spawn_blocking(move || std::fs::File::open(path)?.sync_all()).await??;
    Ok(())
}

pub(crate) type SharedArtifactCandidateOutbox = Arc<ArtifactCandidateOutbox>;

#[cfg(test)]
#[path = "artifact_candidate_outbox_tests.rs"]
mod tests;
