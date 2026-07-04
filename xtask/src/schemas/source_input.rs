use std::collections::BTreeMap;
use std::io::Read;
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
    let mut cache = SourceInputCache::default();
    source_inputs_with_cache(root, paths, &mut cache)
}

pub fn source_inputs_with_cache(
    root: &Path,
    paths: &[&str],
    cache: &mut SourceInputCache,
) -> Result<Vec<SourceInput>> {
    let mut inputs = Vec::with_capacity(paths.len());
    for path in paths {
        inputs.push(cache.source_input(root, PathBuf::from(path))?);
    }
    inputs.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(inputs)
}

#[derive(Debug, Default)]
pub struct SourceInputCache {
    checksums: BTreeMap<String, String>,
}

impl SourceInputCache {
    pub fn source_input(&mut self, root: &Path, rel_path: PathBuf) -> Result<SourceInput> {
        let path = root.join(&rel_path);
        let normalized_path = normalize_path(&rel_path);
        let is_dir = path.is_dir();
        let checksum = if let Some(checksum) = self.checksums.get(&normalized_path) {
            checksum.clone()
        } else {
            let digest = if is_dir {
                directory_digest(root, &rel_path)?
            } else {
                file_digest(&path).with_context(|| {
                    format!("failed to read schema source input {}", rel_path.display())
                })?
            };
            let checksum = format!("sha256:{digest}");
            self.checksums
                .insert(normalized_path.clone(), checksum.clone());
            checksum
        };
        Ok(SourceInput {
            kind: source_input_kind(&normalized_path, is_dir),
            path: normalized_path,
            checksum,
        })
    }
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

fn file_digest(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn directory_digest(root: &Path, rel_path: &Path) -> Result<String> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root.join(rel_path)) {
        let entry = entry?;
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort();

    let mut hasher = Sha256::new();
    for path in files {
        let repo_rel = path
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        hasher.update(repo_rel.as_bytes());
        hasher.update([0]);

        let mut file = std::fs::File::open(&path)?;
        let mut buffer = [0_u8; 8192];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
