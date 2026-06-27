use std::path::{Component, Path, PathBuf};
use std::time::Duration;

pub(crate) use axon_core::CODE_INDEX_VERSION;
use axon_core::config::parse::tuning;

pub(crate) fn freshness_ttl() -> Duration {
    Duration::from_secs(tuning::code_search_freshness_ttl_secs())
}

pub(crate) fn reindex_timeout() -> Duration {
    Duration::from_secs(tuning::code_search_reindex_timeout_secs())
}

pub(crate) fn max_indexed_file_bytes() -> u64 {
    tuning::code_search_max_file_bytes()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeIndexIdentity {
    pub project_root: PathBuf,
    pub project_key: String,
    pub project_display: String,
    pub collection: String,
    pub embedder_key: String,
    pub index_version: u32,
}

impl CodeIndexIdentity {
    pub fn new(
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
pub struct CodeSearchAllowedRoots {
    roots: Vec<PathBuf>,
}

impl CodeSearchAllowedRoots {
    pub fn from_env() -> anyhow::Result<Self> {
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

    pub fn contains(&self, path: &Path) -> bool {
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

pub fn validate_path_prefix(prefix: &str) -> anyhow::Result<Option<String>> {
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
