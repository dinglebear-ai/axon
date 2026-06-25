use super::MAX_PENDING_FILES;
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub(crate) struct PendingFile {
    last_seen: Instant,
    retries: u8,
    last_len: Option<u64>,
    last_mtime: Option<std::time::SystemTime>,
    stable_since: Option<Instant>,
}

#[derive(Debug, Default)]
pub struct PendingFiles {
    pub(crate) files: BTreeMap<PathBuf, PendingFile>,
    pub(crate) coalesced_events: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingState {
    NotReady,
    Stable,
    Terminal,
}

impl PendingFile {
    fn new(now: Instant) -> Self {
        Self {
            last_seen: now,
            retries: 0,
            last_len: None,
            last_mtime: None,
            stable_since: None,
        }
    }
}

impl PendingFiles {
    pub(crate) fn push(&mut self, path: PathBuf, now: Instant) -> bool {
        if let Some(entry) = self.files.get_mut(&path) {
            entry.last_seen = now;
            self.coalesced_events += 1;
            return true;
        }
        if self.files.len() >= MAX_PENDING_FILES {
            return false;
        }
        self.files.insert(path, PendingFile::new(now));
        true
    }

    pub(crate) fn requeue(&mut self, path: PathBuf, now: Instant, max_retries: u8) -> bool {
        let entry = self
            .files
            .entry(path)
            .or_insert_with(|| PendingFile::new(now));
        if entry.retries >= max_retries {
            return false;
        }
        entry.retries += 1;
        entry.last_seen = now;
        entry.stable_since = None;
        true
    }

    pub(crate) fn debounced_paths(&self, now: Instant, debounce: Duration) -> Vec<PathBuf> {
        self.files
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.last_seen) >= debounce)
            .map(|(path, _)| path.clone())
            .collect()
    }

    pub(crate) fn remove(&mut self, path: &Path) {
        self.files.remove(path);
    }

    pub(crate) fn clear(&mut self) {
        self.files.clear();
    }

    pub(crate) fn stable(
        &mut self,
        path: &Path,
        now: Instant,
        settle: Duration,
    ) -> Result<PendingState> {
        let Some(entry) = self.files.get_mut(path) else {
            return Ok(PendingState::Terminal);
        };
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => return Ok(PendingState::Terminal),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PendingState::Terminal);
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            return Ok(PendingState::Terminal);
        }
        let len = metadata.len();
        let mtime = metadata.modified().ok();
        if entry.last_len == Some(len) && entry.last_mtime == mtime {
            let stable_since = *entry.stable_since.get_or_insert(now);
            return Ok(if now.duration_since(stable_since) >= settle {
                PendingState::Stable
            } else {
                PendingState::NotReady
            });
        }
        entry.last_len = Some(len);
        entry.last_mtime = mtime;
        entry.stable_since = Some(now);
        Ok(PendingState::NotReady)
    }
}
