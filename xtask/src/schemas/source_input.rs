use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceInput {
    pub path: String,
    pub kind: SourceInputKind,
    pub checksum: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceInputKind {
    RustModule,
    RustDirectory,
    MarkdownContract,
    SqlMigrationDirectory,
}

pub fn source_inputs(root: &Path, paths: &[&str]) -> Result<Vec<SourceInput>> {
    let mut inputs = Vec::with_capacity(paths.len());
    for path in paths {
        inputs.push(source_input(root, PathBuf::from(path))?);
    }
    inputs.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(inputs)
}

fn source_input(root: &Path, rel_path: PathBuf) -> Result<SourceInput> {
    let path = root.join(&rel_path);
    let bytes = if path.is_dir() {
        directory_bytes(root, &rel_path)?
    } else {
        std::fs::read(&path)
            .with_context(|| format!("failed to read schema source input {}", rel_path.display()))?
    };
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let normalized_path = normalize_path(&rel_path);
    Ok(SourceInput {
        kind: source_input_kind(&normalized_path, path.is_dir()),
        path: normalized_path,
        checksum: format!("sha256:{digest}"),
    })
}

pub(super) fn source_input_kind(path: &str, is_dir: bool) -> SourceInputKind {
    let path = path.replace('\\', "/");
    if path.split('/').any(|component| component == "migrations") && is_dir {
        SourceInputKind::SqlMigrationDirectory
    } else if is_dir {
        SourceInputKind::RustDirectory
    } else if path.ends_with(".md") {
        SourceInputKind::MarkdownContract
    } else {
        SourceInputKind::RustModule
    }
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn directory_bytes(root: &Path, rel_path: &Path) -> Result<Vec<u8>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root.join(rel_path)) {
        let entry = entry?;
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort();

    let mut bytes = Vec::new();
    for path in files {
        let repo_rel = path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        bytes.extend_from_slice(repo_rel.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&std::fs::read(&path)?);
        bytes.push(0);
    }
    Ok(bytes)
}
