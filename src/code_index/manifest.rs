use crate::code_index::config::{CodeIndexIdentity, max_indexed_file_bytes};
use crate::code_index::store::CodeIndexStore;
use crate::vector::ops::file_ingest::{SelectionPolicy, collect_files};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub(crate) struct ManifestOptions {
    pub max_file_bytes: u64,
}

impl Default for ManifestOptions {
    fn default() -> Self {
        Self {
            max_file_bytes: max_indexed_file_bytes(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HashSource {
    Streamed,
    Stored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileManifestEntry {
    pub relative_path: String,
    pub hash: Option<String>,
    pub hash_source: HashSource,
    pub size_bytes: u64,
    pub mtime_ns: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManifestSnapshot {
    pub files: Vec<FileManifestEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FileDiff {
    pub added: Vec<FileManifestEntry>,
    pub modified: Vec<FileManifestEntry>,
    pub removed: Vec<String>,
}

impl FileDiff {
    pub(crate) fn is_empty(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.removed.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn modified_paths(&self) -> Vec<&str> {
        self.modified
            .iter()
            .map(|entry| entry.relative_path.as_str())
            .collect()
    }
}

pub(crate) async fn build_manifest(
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    options: ManifestOptions,
) -> anyhow::Result<ManifestSnapshot> {
    let files = collect_files(
        &identity.project_root,
        SelectionPolicy::Allowlist {
            include_source: true,
        },
    )
    .await?;
    let mut entries = Vec::new();
    for path in files {
        if let Some(entry) = build_entry(store, identity, path, options).await? {
            entries.push(entry);
        }
    }
    entries.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(ManifestSnapshot { files: entries })
}

async fn build_entry(
    store: &CodeIndexStore,
    identity: &CodeIndexIdentity,
    path: PathBuf,
    options: ManifestOptions,
) -> anyhow::Result<Option<FileManifestEntry>> {
    let metadata = tokio::fs::metadata(&path).await?;
    if metadata.len() > options.max_file_bytes {
        return Ok(None);
    }
    let relative_path = path
        .strip_prefix(&identity.project_root)?
        .to_string_lossy()
        .replace('\\', "/");
    let mtime_ns = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos() as i64;
    if let Some(stored) = store.lookup_file(identity, &relative_path).await?
        && stored.size_bytes == metadata.len()
        && stored.mtime_ns == mtime_ns
        && !stored.pending
    {
        return Ok(Some(FileManifestEntry {
            relative_path,
            hash: Some(stored.hash),
            hash_source: HashSource::Stored,
            size_bytes: metadata.len(),
            mtime_ns,
        }));
    }
    let hash = stream_hash(&path).await?;
    Ok(Some(FileManifestEntry {
        relative_path,
        hash: Some(hash),
        hash_source: HashSource::Streamed,
        size_bytes: metadata.len(),
        mtime_ns,
    }))
}

async fn stream_hash(path: &Path) -> anyhow::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = tokio::io::AsyncReadExt::read(&mut file, &mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}
