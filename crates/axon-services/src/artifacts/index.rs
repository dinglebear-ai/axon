use super::*;
use std::{collections::BTreeMap, time::UNIX_EPOCH};
use tokio::time::{Duration, sleep};

pub(super) const INDEX_DIR: &str = ".metadata-index";
pub(super) const INDEX_FILE: &str = "index.json";
const INDEX_LOCK_FILE: &str = "index.lock";

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct ArtifactMetadataIndex {
    root_stamp: DirectoryStamp,
    entries: Vec<ArtifactSummary>,
    all_ids: Vec<ArtifactId>,
    by_kind: BTreeMap<String, Vec<ArtifactId>>,
    by_source: BTreeMap<String, Vec<ArtifactId>>,
    by_job: BTreeMap<String, Vec<ArtifactId>>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct DirectoryStamp {
    seconds: u64,
    nanos: u32,
}

impl ArtifactMetadataIndex {
    pub(super) fn from_entries(
        root_stamp: DirectoryStamp,
        mut entries: Vec<ArtifactSummary>,
    ) -> Self {
        entries.sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
        let mut index = Self {
            root_stamp,
            all_ids: entries
                .iter()
                .map(|entry| entry.artifact_id.clone())
                .collect(),
            entries,
            ..Self::default()
        };
        for entry in &index.entries {
            index
                .by_kind
                .entry(artifact_kind_key(entry.kind))
                .or_default()
                .push(entry.artifact_id.clone());
            if let Some(source_id) = &entry.source_id {
                index
                    .by_source
                    .entry(source_id.0.clone())
                    .or_default()
                    .push(entry.artifact_id.clone());
            }
            if let Some(job_id) = &entry.job_id {
                index
                    .by_job
                    .entry(job_id.0.to_string())
                    .or_default()
                    .push(entry.artifact_id.clone());
            }
        }
        index
    }

    pub(super) fn candidates(&self, request: &ArtifactListRequest) -> &[ArtifactId] {
        let mut candidates = Vec::new();
        if let Some(kind) = request.kind {
            let Some(ids) = self.by_kind.get(&artifact_kind_key(kind)) else {
                return &[];
            };
            candidates.push(ids);
        }
        if let Some(source_id) = &request.source_id {
            let Some(ids) = self.by_source.get(&source_id.0) else {
                return &[];
            };
            candidates.push(ids);
        }
        if let Some(job_id) = &request.job_id {
            let Some(ids) = self.by_job.get(&job_id.0.to_string()) else {
                return &[];
            };
            candidates.push(ids);
        }
        candidates
            .into_iter()
            .min_by_key(|ids| ids.len())
            .map(Vec::as_slice)
            .unwrap_or(&self.all_ids)
    }

    pub(super) fn summary(&self, artifact_id: &ArtifactId) -> Option<&ArtifactSummary> {
        self.entries
            .binary_search_by(|entry| entry.artifact_id.cmp(artifact_id))
            .ok()
            .map(|position| &self.entries[position])
    }
}

pub(super) async fn load_artifact_index(
    ctx: &ServiceContext,
) -> Result<ArtifactMetadataIndex, ApiError> {
    let root = artifact_root_for(ctx);
    match tokio::fs::metadata(&root).await {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            return Err(artifact_error(
                "artifact.list_failed",
                "artifact root is not a directory",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ArtifactMetadataIndex::default());
        }
        Err(error) => return Err(io_error("artifact.list_failed", &root, error)),
    }
    let index_dir = root.join(INDEX_DIR);
    tokio::fs::create_dir_all(&index_dir)
        .await
        .map_err(|error| io_error("artifact.list_failed", &index_dir, error))?;
    let _lock = acquire_index_lock(&index_dir).await?;
    let root_stamp = directory_stamp(&root).await?;
    if let Ok(bytes) = tokio::fs::read(index_dir.join(INDEX_FILE)).await
        && let Ok(index) = serde_json::from_slice::<ArtifactMetadataIndex>(&bytes)
        && index.root_stamp == root_stamp
    {
        return Ok(index);
    }
    let manifests = read_manifests(&root).await?;
    let mut entries = Vec::with_capacity(manifests.len());
    for manifest in manifests {
        entries.push(manifest_summary(ctx, &manifest).await?);
    }
    let index = ArtifactMetadataIndex::from_entries(root_stamp, entries);
    let bytes = serde_json::to_vec(&index)
        .map_err(|error| artifact_error("artifact.list_failed", error.to_string()))?;
    axon_core::artifacts::atomic_write_explicit(index_dir.join(INDEX_FILE), &bytes)
        .await
        .map_err(|error| artifact_error("artifact.list_failed", error.to_string()))?;
    Ok(index)
}

async fn directory_stamp(root: &Path) -> Result<DirectoryStamp, ApiError> {
    let modified = tokio::fs::metadata(root)
        .await
        .and_then(|metadata| metadata.modified())
        .map_err(|error| io_error("artifact.list_failed", root, error))?;
    let duration = modified.duration_since(UNIX_EPOCH).map_err(|error| {
        artifact_error(
            "artifact.list_failed",
            format!("artifact root modification time is invalid: {error}"),
        )
    })?;
    Ok(DirectoryStamp {
        seconds: duration.as_secs(),
        nanos: duration.subsec_nanos(),
    })
}

struct ArtifactIndexLock(PathBuf);

impl Drop for ArtifactIndexLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn acquire_index_lock(index_dir: &Path) -> Result<ArtifactIndexLock, ApiError> {
    let path = index_dir.join(INDEX_LOCK_FILE);
    for _ in 0..50 {
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await
        {
            Ok(_) => return Ok(ArtifactIndexLock(path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if lock_is_stale(&path).await {
                    let _ = tokio::fs::remove_file(&path).await;
                } else {
                    sleep(Duration::from_millis(10)).await;
                }
            }
            Err(error) => return Err(io_error("artifact.list_failed", &path, error)),
        }
    }
    Err(artifact_error(
        "artifact.busy",
        "artifact metadata index is busy; retry the operation",
    ))
}

async fn lock_is_stale(path: &Path) -> bool {
    tokio::fs::metadata(path)
        .await
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > Duration::from_secs(300))
}

fn artifact_kind_key(kind: ArtifactKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{kind:?}"))
}
