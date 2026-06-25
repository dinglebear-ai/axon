use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use tokio::io::AsyncWriteExt;

use crate::manifest::ManifestEntry;

use super::page::PageOutcome;

fn previous_markdown_path(markdown_dir: &Path, entry: &ManifestEntry) -> Option<PathBuf> {
    let relative = Path::new(&entry.relative_path);
    if relative.is_absolute() {
        return None;
    }

    let output_dir = markdown_dir.parent()?;
    let markdown_relative = relative.strip_prefix("markdown").ok()?;
    if markdown_relative.as_os_str().is_empty() || markdown_relative.file_name().is_none() {
        return None;
    }
    if markdown_relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    Some(output_dir.join("markdown.old").join(markdown_relative))
}

async fn previous_path_is_inside_archive(output_dir: &Path, prev_path: &Path) -> bool {
    let archive_root = output_dir.join("markdown.old");
    let Ok(archive_root) = tokio::fs::canonicalize(&archive_root).await else {
        return false;
    };
    let Ok(prev_path) = tokio::fs::canonicalize(prev_path).await else {
        return false;
    };
    prev_path.starts_with(archive_root)
}

async fn previous_markdown_exists(markdown_dir: &Path, prev_path: Option<&PathBuf>) -> bool {
    let Some(output_dir) = markdown_dir.parent() else {
        return false;
    };
    let Some(prev_path) = prev_path else {
        return false;
    };

    tokio::fs::try_exists(prev_path).await.unwrap_or(false)
        && previous_path_is_inside_archive(output_dir, prev_path).await
}

/// Write a page to disk (or relink from cache) and append its manifest entry.
/// Returns `true` on success, `false` on any I/O failure.
pub async fn write_page_to_manifest(
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
    outcome: &PageOutcome,
    markdown_dir: &Path,
    prev_manifest: &HashMap<String, ManifestEntry>,
    url: &str,
) -> Result<bool, String> {
    match outcome {
        PageOutcome::Reused {
            filename,
            trimmed,
            entry,
        } => {
            let prev_path = prev_manifest
                .get(url)
                .and_then(|m| previous_markdown_path(markdown_dir, m));
            let path = markdown_dir.join(filename);
            let prev_exists = previous_markdown_exists(markdown_dir, prev_path.as_ref()).await;
            if !prev_exists {
                axon_core::logging::log_warn(&format!(
                    "cache_miss: previous file missing for {url}, writing fresh"
                ));
                tokio::fs::write(&path, trimmed.as_bytes())
                    .await
                    .map_err(|e| format!("write failed (cache miss fallback): {e}"))?;
                append_manifest_entry(manifest, entry).await?;
                return Ok(true);
            }
            let link_res = if let Some(ref prev) = prev_path {
                if reflink_copy::reflink_or_copy(prev, &path).is_ok() {
                    Ok(())
                } else {
                    tokio::fs::hard_link(prev, &path).await
                }
            } else {
                Err(std::io::Error::other("no previous path"))
            };
            if link_res.is_err() {
                return Ok(false);
            }
            append_manifest_entry(manifest, entry).await?;
            Ok(true)
        }
        PageOutcome::Write {
            filename,
            trimmed,
            entry,
        } => {
            let path = markdown_dir.join(filename);
            tokio::fs::write(&path, trimmed.as_bytes())
                .await
                .map_err(|e| format!("write failed: {e}"))?;
            append_manifest_entry(manifest, entry).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub async fn append_manifest_entry(
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
    entry: &ManifestEntry,
) -> Result<(), String> {
    let mut line =
        serde_json::to_string(entry).map_err(|e| format!("json serialize failed: {e}"))?;
    line.push('\n');
    manifest
        .write_all(line.as_bytes())
        .await
        .map_err(|e| format!("manifest failed: {e}"))
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
