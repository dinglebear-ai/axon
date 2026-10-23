//! Generation-scoped, bounded-window side-effect spool.

use std::path::Path;

use axon_api::source::{
    AcquiredSourceItem, ArtifactCandidate, DocumentStatus, JobId, ManifestItem, SourceGenerationId,
    SourceId, SourceItemKey, SourceWarning,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

mod codec;
mod storage;

const MAX_WINDOW_BYTES: usize = 64 * 1024 * 1024;

/// Private disk-indexed records with a configured SQLite cache target and
/// bounded record buffers. The lifetime side-effect guard remains separate.
pub(super) struct GenerationSpool {
    store: storage::RecordStore,
    _directory: Option<tempfile::TempDir>,
    _slot: Option<tokio::sync::OwnedSemaphorePermit>,
    #[cfg(test)]
    fail_after_flush: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SideEffectsSpoolRecord {
    pub(super) skipped_statuses: Vec<DocumentStatus>,
    pub(super) archive_items: Vec<AcquiredSourceItem>,
    pub(super) artifact_candidates: Vec<ArtifactCandidate>,
    pub(super) warnings: Vec<SourceWarning>,
    pub(super) reused_item_keys: Vec<SourceItemKey>,
    pub(super) refreshed_manifest_items: Vec<ManifestItem>,
}

impl GenerationSpool {
    #[cfg(test)]
    pub(super) fn create(path: &Path) -> anyhow::Result<Self> {
        Self::create_scoped(path, "test")
    }

    fn create_scoped(path: &Path, generation: &str) -> anyhow::Result<Self> {
        Ok(Self {
            store: storage::RecordStore::create(path, generation)?,
            _directory: None,
            _slot: None,
            #[cfg(test)]
            fail_after_flush: false,
        })
    }

    #[cfg(test)]
    pub(super) fn temporary(generation: &str) -> anyhow::Result<Self> {
        Self::temporary_with_owner(generation, generation)
    }

    pub(super) fn temporary_for(
        source_id: &SourceId,
        generation: &SourceGenerationId,
        job_id: JobId,
        attempt: u32,
    ) -> anyhow::Result<Self> {
        let owner_key = serde_json::to_string(&(source_id, generation, job_id, attempt))?;
        Self::temporary_with_owner(&generation.0, &owner_key)
    }

    pub(super) fn persistent_for(
        root: &Path,
        source: SourceId,
        generation: SourceGenerationId,
        job: JobId,
        attempt: u32,
    ) -> anyhow::Result<Self> {
        let owner = super::generation_registry::WorkOwner {
            source,
            generation,
            job,
            attempt,
        };
        let (directory, _registration) = super::generation_registry::register_locked(root, &owner)?;
        Self::create_scoped(&directory.join("records.sqlite"), &owner.key()?)
    }

    fn temporary_with_owner(generation: &str, owner_key: &str) -> anyhow::Result<Self> {
        let directory = private_tempdir()?;
        let safe_generation = generation
            .chars()
            .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
            .take(80)
            .collect::<String>();
        let path = directory.path().join(format!("{safe_generation}.sqlite"));
        let mut spool = Self::create_scoped(&path, owner_key)?;
        spool._directory = Some(directory);
        Ok(spool)
    }

    pub(super) fn hold_admission(&mut self, slot: Option<tokio::sync::OwnedSemaphorePermit>) {
        self._slot = slot;
    }

    pub(super) fn work_directory(&self) -> anyhow::Result<&Path> {
        self.store
            .db
            .path()
            .map(Path::new)
            .and_then(Path::parent)
            .ok_or_else(|| anyhow::anyhow!("generation work store has no private directory"))
    }

    pub(super) fn append<T: Serialize>(&mut self, key: &str, value: &T) -> anyhow::Result<bool> {
        anyhow::ensure!(
            self.store.sealed()?.is_none(),
            "generation spool is sealed; create a new attempt"
        );
        if self.store.contains(key)? {
            return Ok(false);
        }
        let encoded = codec::encode(&(key, value), MAX_WINDOW_BYTES)?;
        let inserted = self.store.append(key, &encoded)?;
        #[cfg(test)]
        if self.fail_after_flush {
            anyhow::bail!("injected ambiguous append failure after durable commit");
        }
        Ok(inserted)
    }

    pub(super) fn seal(&mut self) -> anyhow::Result<()> {
        self.store.seal()?;
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn inject_failure_after_flush(&mut self) {
        self.fail_after_flush = true;
    }

    /// One queued record plus one producer record at most. Dropping the receiver
    /// releases a blocked producer, which then drops its private spool owner.
    #[cfg(test)]
    pub(super) fn replay_stream(
        self,
    ) -> (
        tokio::sync::mpsc::Receiver<SideEffectsSpoolRecord>,
        tokio::task::JoinHandle<anyhow::Result<Self>>,
    ) {
        let (receiver, worker) = self.replay_stream_archived(None);
        (receiver, tokio::spawn(async move { Ok(worker.await??.0) }))
    }

    pub(super) fn replay_stream_archived(
        mut self,
        mut archive: Option<Box<dyn axon_adapters::SourceArchiveWriter>>,
    ) -> (
        tokio::sync::mpsc::Receiver<SideEffectsSpoolRecord>,
        tokio::task::JoinHandle<
            anyhow::Result<(Self, Option<axon_adapters::GeneratedArchiveFile>)>,
        >,
    ) {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let worker = tokio::task::spawn_blocking(move || {
            self.seal()?;
            let mut archive_items = 0_u64;
            self.replay_each::<SideEffectsSpoolRecord>(|_, mut record| {
                if let Some(writer) = archive.as_mut() {
                    for item in record.archive_items.drain(..) {
                        writer.append(&item)?;
                        archive_items += 1;
                    }
                }
                sender
                    .blocking_send(record)
                    .map_err(|_| anyhow::anyhow!("generation finalization receiver closed"))
            })?;
            let archive = if archive_items > 0 {
                archive.map(|writer| writer.finish()).transpose()?
            } else {
                None
            };
            Ok((self, archive))
        });
        (receiver, worker)
    }

    pub(super) fn replay_each<T: DeserializeOwned>(
        &self,
        mut absorb: impl FnMut(String, T) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let observed = self.store.visit_raw(|key, payload, _| {
            let (encoded_key, value): (String, T) = serde_json::from_slice(payload)?;
            anyhow::ensure!(
                key == encoded_key,
                "generation spool record key differs from index"
            );
            absorb(encoded_key, value)
        })?;
        if let Some(expected) = self.store.sealed()? {
            anyhow::ensure!(
                expected == observed,
                "generation spool seal differs from replayed records"
            );
        }
        Ok(())
    }
}

pub(super) fn fence_registered(directory: &Path) -> anyhow::Result<impl Send + use<>> {
    storage::recovery_owner(&directory.join("records.sqlite"))
}

#[cfg(test)]
pub(super) fn open_registered(directory: &Path, owner: &str) -> anyhow::Result<GenerationSpool> {
    let spool = GenerationSpool {
        store: storage::RecordStore::open(&directory.join("records.sqlite"), owner)?,
        _directory: None,
        _slot: None,
        #[cfg(test)]
        fail_after_flush: false,
    };
    spool.replay_each::<SideEffectsSpoolRecord>(|_, _| Ok(()))?;
    Ok(spool)
}

fn private_tempdir() -> anyhow::Result<tempfile::TempDir> {
    let directory = tempfile::Builder::new()
        .prefix("axon-generation-spool-")
        .tempdir()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(directory)
}

#[cfg(test)]
#[path = "generation_spool_tests.rs"]
mod tests;
