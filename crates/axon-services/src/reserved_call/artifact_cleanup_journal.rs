use std::path::{Path, PathBuf};

use axon_api::source::{
    ArtifactId, ArtifactKind, ArtifactRef, JobId, SourceGenerationId, SourceId, Timestamp,
};
use serde::{Deserialize, Serialize};

use super::ArtifactCleanupWork;

mod secure_io;
use secure_io::SecureJournalDir;

const SCHEMA_VERSION: u8 = 1;
static ACTIVE_CLAIMS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<PathBuf, std::fs::File>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum JournalFault {
    Create,
    FileSync,
    Rename,
    Remove,
    OwnerWrite,
    OwnerSync,
    OwnerRename,
    Read,
}

#[cfg(all(test, unix))]
static ROOT_SWAPS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<PathBuf, (PathBuf, PathBuf)>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

#[cfg(all(test, unix))]
pub(super) fn inject_root_swap(root: &Path, displaced: &Path, external: &Path) {
    ROOT_SWAPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            root.to_path_buf(),
            (displaced.to_path_buf(), external.to_path_buf()),
        );
}

#[cfg(test)]
static JOURNAL_FAULTS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashSet<(PathBuf, JournalFault)>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

#[cfg(test)]
pub(super) fn inject_fault(path: &Path, fault: JournalFault) {
    JOURNAL_FAULTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert((path.to_path_buf(), fault));
}

#[cfg(test)]
pub(super) fn inject_next_persist_failure(work: &ArtifactCleanupWork) {
    inject_fault(
        &default_root().join(format!("{}.json", journal_id(work))),
        JournalFault::Create,
    );
}

#[cfg(test)]
pub(super) fn clear_process_local_state() {
    ACTIVE_CLAIMS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

#[cfg(test)]
fn fail_if_injected(path: &Path, fault: JournalFault) -> anyhow::Result<()> {
    if JOURNAL_FAULTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&(path.to_path_buf(), fault))
    {
        anyhow::bail!("injected journal {fault:?} failure");
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArtifactCleanupJournalRecord {
    pub(super) schema_version: u8,
    pub(super) job_id: JobId,
    pub(super) attempt: u32,
    pub(super) source_id: SourceId,
    pub(super) generation: SourceGenerationId,
    pub(super) artifacts: Vec<JournalArtifact>,
    pub(super) created_at: Timestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct JournalArtifact {
    pub(super) artifact_id: ArtifactId,
    pub(super) artifact_kind: ArtifactKind,
}

#[derive(Clone, Debug)]
pub(super) struct JournalToken(PathBuf, std::sync::Arc<SecureJournalDir>);

#[derive(Debug, Default)]
pub(super) struct ReplaySummary {
    pub(super) claimed: usize,
    pub(super) quarantined: usize,
    pub(super) errors: Vec<String>,
}

#[cfg(not(test))]
pub(super) fn default_root() -> PathBuf {
    axon_core::paths::axon_data_base_dir().join("artifact-cleanup-journal")
}

#[cfg(test)]
pub(super) fn default_root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "axon-artifact-cleanup-journal-{}",
        std::process::id()
    ))
}

pub(super) async fn persist(
    root: &Path,
    work: &ArtifactCleanupWork,
) -> anyhow::Result<JournalToken> {
    let root = root.to_path_buf();
    let work = work.clone();
    tokio::task::spawn_blocking(move || persist_blocking(&root, &work)).await?
}

pub(super) fn persist_blocking(
    root: &Path,
    work: &ArtifactCleanupWork,
) -> anyhow::Result<JournalToken> {
    let directory = std::sync::Arc::new(SecureJournalDir::open(root)?);
    let token = JournalToken(
        root.join(format!("{}.json", journal_id(work))),
        std::sync::Arc::clone(&directory),
    );
    directory.rewrite(&token, &record_from_work(work))?;
    directory.verify_path()?;
    Ok(token)
}

pub(super) async fn rewrite(
    token: &JournalToken,
    work: &ArtifactCleanupWork,
) -> anyhow::Result<()> {
    let record = record_from_work(work);
    let token = token.clone();
    tokio::task::spawn_blocking(move || {
        token.1.rewrite(&token, &record)?;
        token.1.verify_path()
    })
    .await?
}

fn record_from_work(work: &ArtifactCleanupWork) -> ArtifactCleanupJournalRecord {
    ArtifactCleanupJournalRecord {
        schema_version: SCHEMA_VERSION,
        job_id: work.job_id,
        attempt: work.attempt,
        source_id: work.source_id.clone(),
        generation: work.generation.clone(),
        artifacts: work
            .artifacts
            .iter()
            .map(|artifact| JournalArtifact {
                artifact_id: artifact.artifact_id.clone(),
                artifact_kind: artifact.artifact_kind,
            })
            .collect(),
        created_at: Timestamp::from(chrono::Utc::now()),
    }
}

pub(super) async fn remove(token: &JournalToken) -> anyhow::Result<()> {
    #[cfg(test)]
    fail_if_injected(&token.0, JournalFault::Remove)?;
    let token = token.clone();
    tokio::task::spawn_blocking(move || {
        token.1.remove(&token)?;
        token.1.verify_path()?;
        {
            ACTIVE_CLAIMS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&token.0);
        }
        Ok(())
    })
    .await?
}

fn journal_id(work: &ArtifactCleanupWork) -> uuid::Uuid {
    journal_id_parts(work.job_id, work.attempt, &work.source_id, &work.generation)
}

fn journal_id_record(record: &ArtifactCleanupJournalRecord) -> uuid::Uuid {
    journal_id_parts(
        record.job_id,
        record.attempt,
        &record.source_id,
        &record.generation,
    )
}

fn journal_id_parts(
    job_id: JobId,
    attempt: u32,
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> uuid::Uuid {
    let identity = format!("{}:{attempt}:{}:{}", job_id.0, source_id.0, generation.0);
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, identity.as_bytes())
}

#[cfg(test)]
#[path = "artifact_cleanup_journal_tests.rs"]
mod tests;

pub(super) async fn replay(
    root: &Path,
    runtime: &crate::context::TargetLocalSourceRuntime,
) -> anyhow::Result<ReplaySummary> {
    let replay_directory = std::sync::Arc::new(SecureJournalDir::open(root)?);
    replay_directory.verify_path()?;
    replay_directory.sweep_stale_temporaries()?;
    let mut summary = ReplaySummary::default();
    let mut entries = match tokio::fs::read_dir(root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(summary),
        Err(error) => return Err(error.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        replay_directory.verify_path()?;
        let path = entry.path();
        let extension = path.extension().and_then(|value| value.to_str());
        if !matches!(extension, Some("json" | "claim")) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if uuid::Uuid::parse_str(stem).is_err() {
            continue;
        }
        let claimed = path.with_extension("claim");
        match claim_in(
            &replay_directory,
            &path,
            &claimed,
            extension == Some("json"),
        ) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(error) => {
                summary.errors.push(error.to_string());
                continue;
            }
        }
        replay_directory.verify_path()?;
        let bytes = match replay_directory.read(&claimed) {
            Ok(bytes) => bytes,
            Err(error) => {
                summary.errors.push(error.to_string());
                ACTIVE_CLAIMS
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(&claimed);
                continue;
            }
        };
        let record = serde_json::from_slice::<ArtifactCleanupJournalRecord>(&bytes).ok();
        let valid = record.filter(|record| valid_record(record, stem));
        let Some(record) = valid else {
            let quarantine = claimed.with_extension(format!("quarantine-{}", uuid::Uuid::new_v4()));
            match replay_directory.quarantine(&claimed, &quarantine) {
                Ok(()) => summary.quarantined += 1,
                Err(error) => summary.errors.push(error.to_string()),
            }
            replay_directory.verify_path()?;
            ACTIVE_CLAIMS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&claimed);
            continue;
        };
        let token = JournalToken(claimed, std::sync::Arc::clone(&replay_directory));
        let work = ArtifactCleanupWork {
            store: runtime.artifact_store.clone(),
            ledger: runtime.ledger.clone(),
            scheduler: runtime.artifact_scheduler.clone(),
            job_id: record.job_id,
            attempt: record.attempt,
            source_id: record.source_id,
            generation: record.generation,
            artifacts: record
                .artifacts
                .into_iter()
                .map(|artifact| ArtifactRef {
                    artifact_id: artifact.artifact_id,
                    artifact_kind: artifact.artifact_kind,
                    uri: String::new(),
                    size_bytes: None,
                    content_hash: None,
                    created_at: record.created_at.clone(),
                })
                .collect(),
            journal: Some(token),
        };
        super::spawn_artifact_cleanup_retry(work);
        summary.claimed += 1;
    }
    Ok(summary)
}

#[cfg(test)]
fn claim(path: &Path, claimed: &Path, needs_rename: bool) -> std::io::Result<bool> {
    let parent = claimed
        .parent()
        .ok_or_else(|| std::io::Error::other("claim has no parent"))?;
    let directory = SecureJournalDir::open(parent).map_err(std::io::Error::other)?;
    claim_in(&directory, path, claimed, needs_rename)
}

fn claim_in(
    directory: &SecureJournalDir,
    path: &Path,
    claimed: &Path,
    needs_rename: bool,
) -> std::io::Result<bool> {
    if ACTIVE_CLAIMS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains_key(claimed)
    {
        return Ok(false);
    }
    let Some(file) = directory
        .acquire_lease(path, claimed, needs_rename)
        .map_err(std::io::Error::other)?
    else {
        return Ok(false);
    };
    write_claim_owner(directory, claimed)?;
    let mut active = ACTIVE_CLAIMS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if active.contains_key(claimed) {
        drop(file);
        return Ok(false);
    }
    active.insert(claimed.to_path_buf(), file);
    Ok(true)
}

fn write_claim_owner(directory: &SecureJournalDir, claimed: &Path) -> std::io::Result<()> {
    let metadata = serde_json::json!({
        "pid": std::process::id(),
        "claimed_at_unix_ms": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        "owner_id": uuid::Uuid::new_v4(),
    });
    directory
        .write_owner(claimed, metadata.to_string().as_bytes())
        .map_err(std::io::Error::other)
}

fn valid_record(record: &ArtifactCleanupJournalRecord, filename_stem: &str) -> bool {
    if record.schema_version != SCHEMA_VERSION
        || journal_id_record(record).to_string() != filename_stem
        || record.source_id.0.trim().is_empty()
        || record.generation.0.trim().is_empty()
        || record.artifacts.is_empty()
    {
        return false;
    }
    let mut ids = std::collections::HashSet::new();
    record.artifacts.iter().all(|artifact| {
        !artifact.artifact_id.0.trim().is_empty() && ids.insert(&artifact.artifact_id.0)
    })
}
