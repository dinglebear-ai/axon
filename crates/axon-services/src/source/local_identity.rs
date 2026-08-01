use std::path::Path;

use axon_api::source::SourceId;
use sha2::{Digest as _, Sha256};

pub(crate) fn local_source_id(root: &Path) -> SourceId {
    let stable_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    SourceId::new(format!("src_local_{}", source_token(&stable_root)))
}

fn source_token(root: &Path) -> String {
    stable_token(&file_url_for_path(root).unwrap_or_else(|_| root.display().to_string()))
}

fn stable_token(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let mut token = String::with_capacity(24);
    for byte in &digest[..12] {
        use std::fmt::Write as _;
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}

fn file_url_for_path(path: &Path) -> anyhow::Result<String> {
    url::Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|()| anyhow::anyhow!("failed to build file URL for local source"))
}

#[cfg(test)]
mod tests {
    use super::local_source_id;

    #[test]
    fn equivalent_local_paths_share_a_stable_source_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let nested = dir.path().join("nested");
        std::fs::create_dir_all(&nested).expect("nested");
        let canonical = dir.path().canonicalize().expect("canonical path");
        let equivalent = nested.join("..");
        assert_eq!(local_source_id(&canonical), local_source_id(&equivalent));
    }
}
