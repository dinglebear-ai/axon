use std::collections::HashMap;

use tokio::io::AsyncWriteExt;

use crate::crawl::manifest::ManifestEntry;

use super::page::PageOutcome;

/// Write a page to disk (or relink from cache) and append its manifest entry.
/// Returns `true` on success, `false` on any I/O failure.
pub async fn write_page_to_manifest(
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
    outcome: &PageOutcome,
    markdown_dir: &std::path::Path,
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
                .map(|m| std::path::PathBuf::from(&m.relative_path));
            let path = markdown_dir.join(filename);
            let prev_exists = match prev_path {
                Some(ref p) => tokio::fs::try_exists(p).await.unwrap_or(false),
                None => false,
            };
            if !prev_exists {
                crate::core::logging::log_warn(&format!(
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
