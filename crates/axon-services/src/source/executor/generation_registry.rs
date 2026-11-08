//! Bounded private generation work registry. Ledger receipts remain authoritative.
use anyhow::{Context as _, ensure};
use axon_api::source::*;
use fs2::FileExt as _;
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};

const MAX_REGISTERED_ATTEMPTS: usize = crate::context::GENERATION_WORK_ADMISSION;
const MAX_OWNER_BYTES: u64 = 4096;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkOwner {
    pub(super) source: SourceId,
    pub(super) generation: SourceGenerationId,
    pub(super) job: JobId,
    pub(super) attempt: u32,
}

impl WorkOwner {
    pub(super) fn key(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string(&(
            &self.source,
            &self.generation,
            self.job,
            self.attempt,
        ))?)
    }
    fn file_key(&self) -> anyhow::Result<String> {
        Ok(hex::encode(Sha256::digest(self.key()?.as_bytes())))
    }
}

pub(super) struct RegistryLock(std::fs::File);
impl Drop for RegistryLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

pub(super) fn lock(root: &Path) -> anyhow::Result<RegistryLock> {
    std::fs::create_dir_all(root)?;
    let metadata = std::fs::symlink_metadata(root)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "generation registry root must be a directory"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(root.join(".registry.lock"))?;
    ensure!(
        file.metadata()?.is_file(),
        "generation registry owner must be a regular file"
    );
    file.try_lock_exclusive()
        .context("generation work registry is busy")?;
    Ok(RegistryLock(file))
}

pub(super) fn entries(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_name() == ".registry.lock" {
            continue;
        }
        ensure!(
            entries.len() < MAX_REGISTERED_ATTEMPTS,
            "generation work registry exceeds its four-attempt admission"
        );
        let name = entry.file_name();
        let name = name.to_string_lossy();
        ensure!(
            name.len() == 64 && name.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "unknown generation registry entry; preserve for inspection"
        );
        let metadata = std::fs::symlink_metadata(entry.path())?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "generation registry entry must be a private directory"
        );
        entries.push(entry.path());
    }
    entries.sort();
    Ok(entries)
}

#[cfg(test)]
pub(super) fn register(root: &Path, owner: &WorkOwner) -> anyhow::Result<PathBuf> {
    Ok(register_locked(root, owner)?.0)
}

// Keep registration fenced until the caller has acquired the spool owner lock.
// Otherwise another process can observe owner.json before initialization ends.
pub(super) fn register_locked(
    root: &Path,
    owner: &WorkOwner,
) -> anyhow::Result<(PathBuf, RegistryLock)> {
    use std::io::Write as _;
    let registration = lock(root)?;
    let entries = entries(root)?;
    ensure!(
        entries.len() < MAX_REGISTERED_ATTEMPTS,
        "generation work admission exhausted: four attempts retained; reconcile receipts before retry"
    );
    let directory = root.join(owner.file_key()?);
    std::fs::create_dir(&directory)
        .context("generation attempt already registered; reconcile before retry")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
    }
    let encoded = serde_json::to_vec(owner)?;
    ensure!(
        encoded.len() as u64 <= MAX_OWNER_BYTES,
        "generation work owner exceeds admission"
    );
    let mut file = tempfile::NamedTempFile::new_in(&directory)?;
    file.write_all(&encoded)?;
    file.as_file().sync_all()?;
    file.persist(directory.join("owner.json"))
        .map_err(|error| error.error)?;
    std::fs::File::open(&directory)?.sync_all()?;
    std::fs::File::open(root)?.sync_all()?;
    Ok((directory, registration))
}

pub(super) fn read_owner(directory: &Path) -> anyhow::Result<WorkOwner> {
    use std::io::Read as _;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(directory.join("owner.json"))?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= MAX_OWNER_BYTES,
        "invalid generation work owner file"
    );
    let mut bytes = Vec::new();
    file.take(MAX_OWNER_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_OWNER_BYTES,
        "generation work owner exceeds admission"
    );
    let owner: WorkOwner = serde_json::from_slice(&bytes)?;
    ensure!(
        directory.file_name().and_then(|name| name.to_str()) == Some(owner.file_key()?.as_str()),
        "generation work owner identity differs from entry"
    );
    Ok(owner)
}

/// Reconcile closed work only. Never reacquire/index a source, infer a commit,
/// or mutate provider data. Unsettled or unidentified work remains admitted.
pub(crate) async fn recover(
    runtime: &crate::context::TargetLocalSourceRuntime,
) -> anyhow::Result<usize> {
    let _gate = runtime.generation_work_gate.lock().await;
    let Some(root) = runtime.generation_work_root.clone() else {
        return Ok(0);
    };
    let registry_root = root.clone();
    let (registry, entries) = tokio::task::spawn_blocking(move || {
        let lock = lock(&registry_root)?;
        Ok::<_, anyhow::Error>((lock, entries(&registry_root)?))
    })
    .await??;
    let _registry = registry;
    let _settlement = runtime.publication_settlement_gate.lock().await;
    // Receipt reads are atomic SQLite reads. Do not hold the writer gate while
    // waiting for ledger admission: a lease heartbeat can already own that
    // admission permit and be waiting to write. Terminal receipts cannot revert.

    let mut reconciled = 0;
    for directory in entries {
        let candidate = directory.clone();
        let recovered = tokio::task::spawn_blocking(move || {
            let owner = read_owner(&candidate)?;
            // Ownership is independent of SQLite integrity: terminal abandoned
            // attempts may have crashed before creating a usable database.
            let fence = super::generation_spool::fence_registered(&candidate)?;
            Ok::<_, anyhow::Error>((owner, fence))
        })
        .await?;
        let (owner, fence) = match recovered {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(error=%error, "generation work recovery deferred; active or invalid owner");
                continue;
            }
        };
        let receipt = runtime
            .ledger
            .generation_receipt(owner.source.clone(), owner.generation.clone())
            .await?;
        let terminal_failure = receipt.as_ref().is_some_and(|receipt| {
            matches!(
                receipt.status,
                LifecycleStatus::Failed | LifecycleStatus::Canceled | LifecycleStatus::Expired
            )
        });
        let published = receipt.as_ref().is_some_and(|receipt| receipt.published);
        if published || terminal_failure {
            tokio::task::spawn_blocking(move || {
                let _fence = fence;
                std::fs::remove_dir_all(directory)
            })
            .await??;
            reconciled += 1;
        } else {
            tracing::warn!(source_id=%owner.source.0, generation=%owner.generation.0, attempt=owner.attempt,
                "generation work has no settled receipt; preserved without automatic replay");
        }
    }
    tokio::task::spawn_blocking(move || std::fs::File::open(root)?.sync_all()).await??;
    Ok(reconciled)
}

#[cfg(test)]
mod tests;
