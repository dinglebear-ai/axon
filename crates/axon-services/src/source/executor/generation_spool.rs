//! Generation-scoped, bounded-window side-effect spool.

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{BufRead as _, BufReader, BufWriter, Seek as _, Write as _};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;

use axon_api::source::{
    AcquiredSourceItem, ArtifactCandidate, ManifestItem, SourceItemKey, SourceWarning,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

const MAX_WINDOW_BYTES: usize = 64 * 1024 * 1024;

/// Append-only JSONL storage with an in-memory deduplication index. Each
/// serialized record is capped at 64 MiB and replay is streaming, but the key
/// index and the current serialized/deserialized values are not an aggregate
/// 64-MiB memory bound.
pub(super) struct GenerationSpool {
    file: File,
    keys: HashSet<String>,
    _directory: Option<tempfile::TempDir>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct SideEffectsSpoolRecord {
    pub(super) archive_items: Vec<AcquiredSourceItem>,
    pub(super) artifact_candidates: Vec<ArtifactCandidate>,
    pub(super) warnings: Vec<SourceWarning>,
    pub(super) reused_item_keys: Vec<SourceItemKey>,
    pub(super) refreshed_manifest_items: Vec<ManifestItem>,
}

impl GenerationSpool {
    pub(super) fn create(path: &Path) -> anyhow::Result<Self> {
        let mut options = OpenOptions::new();
        options.create_new(true).read(true).append(true);
        #[cfg(unix)]
        options.mode(0o600);
        let file = options.open(path)?;
        Ok(Self {
            file,
            keys: HashSet::new(),
            _directory: None,
        })
    }

    pub(super) fn temporary(generation: &str) -> anyhow::Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("axon-generation-spool-")
            .tempdir()?;
        let safe_generation = generation
            .chars()
            .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
            .take(80)
            .collect::<String>();
        let path = directory.path().join(format!("{safe_generation}.jsonl"));
        let mut spool = Self::create(&path)?;
        spool._directory = Some(directory);
        Ok(spool)
    }

    pub(super) fn append<T: Serialize>(&mut self, key: &str, value: &T) -> anyhow::Result<bool> {
        if self.keys.contains(key) {
            return Ok(false);
        }
        let encoded = serde_json::to_vec(&(key, value))?;
        anyhow::ensure!(
            encoded.len() <= MAX_WINDOW_BYTES,
            "generation spool record exceeds 64 MiB window"
        );
        let mut writer = BufWriter::new(&self.file);
        writer.write_all(&encoded)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        self.keys.insert(key.to_string());
        Ok(true)
    }

    pub(super) fn replay_each<T: DeserializeOwned>(
        &self,
        mut absorb: impl FnMut(String, T) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut replay = self.file.try_clone()?;
        replay.seek(std::io::SeekFrom::Start(0))?;
        let mut reader = BufReader::new(replay);
        let mut line = Vec::new();
        loop {
            line.clear();
            let read = reader.read_until(b'\n', &mut line)?;
            if read == 0 {
                break;
            }
            anyhow::ensure!(
                line.len() <= MAX_WINDOW_BYTES + 1,
                "generation spool record exceeds 64 MiB window"
            );
            let (key, value) = serde_json::from_slice(&line)?;
            absorb(key, value)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "generation_spool_tests.rs"]
mod tests;
