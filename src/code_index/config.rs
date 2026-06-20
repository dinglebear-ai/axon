use std::path::{Component, Path, PathBuf};
use std::time::Duration;

pub(crate) const CODE_INDEX_VERSION: u32 = 1;
pub(crate) const DEFAULT_FRESHNESS_TTL: Duration = Duration::from_secs(30);
pub(crate) const DEFAULT_REINDEX_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const DEFAULT_CHANGED_FILE_BATCH_SIZE: usize = 50;
pub(crate) const MAX_INDEXED_FILE_BYTES: u64 = 10 * 1024 * 1024;

pub(crate) fn freshness_ttl() -> Duration {
    duration_secs_env("AXON_CODE_SEARCH_FRESHNESS_TTL_SECS", DEFAULT_FRESHNESS_TTL)
}

pub(crate) fn reindex_timeout() -> Duration {
    duration_secs_env(
        "AXON_CODE_SEARCH_REINDEX_TIMEOUT_SECS",
        DEFAULT_REINDEX_TIMEOUT,
    )
}

pub(crate) fn max_indexed_file_bytes() -> u64 {
    u64_env(
        "AXON_CODE_SEARCH_MAX_FILE_BYTES",
        MAX_INDEXED_FILE_BYTES,
        "bytes",
    )
}

fn duration_secs_env(name: &str, fallback: Duration) -> Duration {
    Duration::from_secs(u64_env(name, fallback.as_secs(), "seconds"))
}

pub(crate) fn u64_env(name: &str, fallback: u64, unit: &'static str) -> u64 {
    match std::env::var(name) {
        Ok(value) => match value.parse::<u64>() {
            Ok(parsed) if parsed > 0 => parsed,
            _ => {
                tracing::warn!(
                    env_var = name,
                    value,
                    default = fallback,
                    unit,
                    "invalid code search environment variable; using default"
                );
                fallback
            }
        },
        Err(_) => fallback,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeIndexIdentity {
    pub project_root: PathBuf,
    pub project_key: String,
    pub project_display: String,
    pub collection: String,
    pub embedder_key: String,
    pub index_version: u32,
}

impl CodeIndexIdentity {
    pub(crate) fn new(
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
        let project_display = project_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("local-code")
            .to_string();
        Self {
            project_root,
            project_key,
            project_display,
            collection: collection.to_string(),
            embedder_key: embedder_key.to_string(),
            index_version: CODE_INDEX_VERSION,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(root: &Path, origin: &str, collection: &str, embedder: &str) -> Self {
        Self::new(root.to_path_buf(), origin.to_string(), collection, embedder)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CodeSearchAllowedRoots {
    roots: Vec<PathBuf>,
}

impl CodeSearchAllowedRoots {
    pub(crate) fn from_env() -> anyhow::Result<Self> {
        let raw = std::env::var("AXON_CODE_SEARCH_ALLOWED_ROOTS").unwrap_or_default();
        Self::from_root_strings(
            raw.split([':', ','])
                .map(str::trim)
                .filter(|part| !part.is_empty()),
        )
    }

    fn from_root_strings<'a>(parts: impl IntoIterator<Item = &'a str>) -> anyhow::Result<Self> {
        let mut roots = Vec::new();
        for part in parts {
            roots.push(validate_allowed_root(part)?);
        }
        Ok(Self { roots })
    }

    pub(crate) fn contains(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| path.starts_with(root))
    }
}

fn validate_allowed_root(part: &str) -> anyhow::Result<PathBuf> {
    let canonical = std::fs::canonicalize(part)?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if canonical == Path::new("/") || home.as_deref() == Some(canonical.as_path()) {
        anyhow::bail!(
            "code search allowed root cannot be / or HOME: {}",
            canonical.display()
        );
    }
    Ok(canonical)
}

#[cfg(test)]
impl CodeSearchAllowedRoots {
    pub(crate) fn from_raw_for_test(raw: &str) -> anyhow::Result<Self> {
        Self::from_root_strings(
            raw.split([':', ','])
                .map(str::trim)
                .filter(|part| !part.is_empty()),
        )
    }
}

pub(crate) fn validate_path_prefix(prefix: &str) -> anyhow::Result<Option<String>> {
    let trimmed = prefix.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        anyhow::bail!("path_prefix must be repository-relative");
    }
    for component in path.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            anyhow::bail!("path_prefix cannot escape the repository root");
        }
    }
    let normalized = trimmed.trim_end_matches('/').to_string() + "/";
    Ok(Some(normalized))
}
