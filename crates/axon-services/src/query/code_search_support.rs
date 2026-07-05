use std::error::Error;
use std::path::{Component, Path, PathBuf};

use axon_core::CODE_INDEX_VERSION;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FreshnessWarning {
    TimedOut { timeout_ms: u64 },
    Failed { error: String },
    AlreadyRunning,
    MissingCommittedIndex,
}

impl FreshnessWarning {
    pub fn message(&self) -> String {
        match self {
            Self::TimedOut { timeout_ms } => {
                format!("refresh timed out after {timeout_ms}ms; stale index used")
            }
            Self::Failed { error } => {
                format!("refresh failed: {error}; stale index used")
            }
            Self::AlreadyRunning => "refresh already running; stale index used".to_string(),
            Self::MissingCommittedIndex => {
                "no committed code index; rerun without --no-freshness to build it".to_string()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum ReindexProgress {
    Started {
        generation: i64,
        total_files: usize,
        added_files: usize,
        modified_files: usize,
        removed_files: usize,
        total_batches: usize,
    },
    BatchFinished {
        generation: i64,
        batch_number: usize,
        total_batches: usize,
        processed_files: usize,
        total_files: usize,
        batch_files: usize,
        embedded_docs: usize,
    },
    CleanupStarted {
        generation: i64,
        cleanup_paths: usize,
    },
    CommitStarted {
        generation: i64,
    },
    Finished {
        generation: i64,
    },
}

pub trait ReindexProgressSink: Sync {
    fn emit(&self, progress: ReindexProgress);
}

pub(super) fn validate_path_prefix(
    prefix: &str,
) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    let trimmed = prefix.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err("path_prefix must be repository-relative".into());
    }
    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err("path_prefix cannot escape the repository root".into());
        }
    }
    Ok(Some(trimmed.trim_end_matches('/').to_string() + "/"))
}

#[derive(Debug, Clone, Default)]
pub(super) struct CodeSearchAllowedRoots {
    roots: Vec<PathBuf>,
}

impl CodeSearchAllowedRoots {
    pub(super) fn from_env() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let raw = std::env::var("AXON_CODE_SEARCH_ALLOWED_ROOTS").unwrap_or_default();
        Self::from_root_strings(
            raw.split([':', ','])
                .map(str::trim)
                .filter(|part| !part.is_empty()),
        )
    }

    fn from_root_strings<'a>(
        parts: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let mut roots = Vec::new();
        for part in parts {
            roots.push(validate_allowed_root(part)?);
        }
        Ok(Self { roots })
    }

    pub(super) fn contains(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| path.starts_with(root))
    }
}

fn validate_allowed_root(part: &str) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let canonical = std::fs::canonicalize(part)?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if canonical == Path::new("/") || home.as_deref() == Some(canonical.as_path()) {
        return Err(format!(
            "code search allowed root cannot be / or HOME: {}",
            canonical.display()
        )
        .into());
    }
    Ok(canonical)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::query) struct CodeIndexIdentity {
    pub project_root: PathBuf,
    pub project_key: String,
}

impl CodeIndexIdentity {
    pub(super) fn new(
        project_root: PathBuf,
        project_origin: String,
        collection: &str,
        embedder_key: &str,
    ) -> Self {
        let key_seed = format!(
            "origin={project_origin}\ncollection={collection}\nembedder={embedder_key}\nindex_version={CODE_INDEX_VERSION}"
        );
        let project_key =
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, key_seed.as_bytes()).to_string();
        Self {
            project_root,
            project_key,
        }
    }
}
