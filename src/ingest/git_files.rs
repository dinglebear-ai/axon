//! Shared helpers for git-backed ingest providers.
//!
//! Centralises the repo-tree BFS walk and the `embed_docs` thin wrapper that
//! were previously duplicated verbatim in `generic_git`, `gitlab/files`, and
//! the gitea embed module (Q-H2, Q-M7).

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use crate::core::config::Config;
use crate::ingest::github::{is_indexable_doc_path, is_indexable_source_path};
use crate::vector::ops::{PreparedDoc, embed_prepared_docs};

/// Recursively walk `root`, returning all files that pass the indexability
/// filters.  Skips `.git` directories.  Results are sorted for deterministic
/// ordering (important for reproducible embedding).
///
/// This is the single canonical BFS walk shared by `generic_git`,
/// `gitlab/files`, and `gitea` — previously three near-identical copies.
pub(crate) async fn collect_repo_files(root: &Path, include_source: bool) -> Result<Vec<PathBuf>> {
    let mut dirs = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = dirs.pop() {
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                if entry.file_name() != ".git" {
                    dirs.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let rel = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            if is_indexable_doc_path(&rel) || (include_source && is_indexable_source_path(&rel)) {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Thin wrapper around `embed_prepared_docs` used by all git ingest embed
/// helpers — avoids duplicating the `map_err` / `.chunks_embedded` extraction
/// in every provider embed module (Q-M7).
pub(crate) async fn embed_docs(cfg: &Config, docs: Vec<PreparedDoc>) -> Result<usize> {
    let summary = embed_prepared_docs(cfg, docs, None)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    Ok(summary.chunks_embedded)
}
