use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_info, log_warn};
use std::error::Error;
use std::path::Path;

/// Update the `latest/` symlink directory to point at the new crawl output via
/// reflink copies. Guards against self-delete and accidental deletion of parent
/// directories.
pub async fn update_latest_reflink(
    source_dir: &Path,
    latest_dir: &Path,
) -> Result<(), Box<dyn Error>> {
    if source_dir == latest_dir {
        return Err("source_dir and latest_dir must not be the same path".into());
    }
    if source_dir.starts_with(latest_dir) {
        return Err("source_dir must not be inside latest_dir".into());
    }

    if latest_dir.exists() {
        tokio::fs::remove_dir_all(latest_dir).await.map_err(|e| {
            format!(
                "failed to remove old latest dir {}: {e}",
                latest_dir.display()
            )
        })?;
    }
    tokio::fs::create_dir_all(latest_dir)
        .await
        .map_err(|e| format!("failed to create latest dir {}: {e}", latest_dir.display()))?;

    let manifest = "manifest.jsonl";
    let source_manifest = source_dir.join(manifest);
    if source_manifest.exists() {
        let src = source_manifest.clone();
        let dst = latest_dir.join(manifest);
        tokio::task::spawn_blocking(move || reflink_copy::reflink_or_copy(&src, dst)).await??;
    }

    let markdown = "markdown";
    let source_md = source_dir.join(markdown);
    let target_md = latest_dir.join(markdown);
    if source_md.exists() {
        tokio::fs::create_dir_all(&target_md).await?;
        let mut entries = tokio::fs::read_dir(source_md).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                let Some(filename) = path.file_name() else {
                    continue;
                };
                let dst = target_md.join(filename);
                let src = path.clone();
                tokio::task::spawn_blocking(move || reflink_copy::reflink_or_copy(&src, dst))
                    .await??;
            }
        }
    }

    log_info(&format!(
        "Updated 'latest' armory view via reflink: {}",
        latest_dir.display()
    ));
    Ok(())
}

/// Prepare the output directory before a crawl run.
///
/// - Cache mode: archives existing `markdown/` to `markdown.old/` (Recycling Bin Pattern)
///   so the collector can surgically reuse unchanged pages.
/// - Non-cache mode: wipes the directory unless `AXON_NO_WIPE` is set.
/// - Always ensures `markdown/` exists at the end.
pub(super) async fn prepare_crawl_output_dir(
    output_dir: &Path,
    markdown_dir: &Path,
    recycling_bin: &Path,
    cfg: &Config,
) -> Result<(), Box<dyn Error>> {
    if output_dir.exists() {
        if cfg.cache {
            if markdown_dir.exists() {
                if recycling_bin.exists() {
                    tokio::fs::remove_dir_all(recycling_bin).await?;
                }
                tokio::fs::rename(markdown_dir, recycling_bin).await?;
                log_info(&format!(
                    "Archived existing spoils to recycling bin for incremental reuse: {}",
                    recycling_bin.display()
                ));
            }
        } else if std::env::var("AXON_NO_WIPE").is_err() {
            log_warn(&format!(
                "Clearing output directory before crawl: {}",
                output_dir.display()
            ));
            let mut entries = tokio::fs::read_dir(output_dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let meta = tokio::fs::symlink_metadata(&path).await?;
                if meta.is_symlink() || meta.is_file() {
                    tokio::fs::remove_file(&path).await?;
                } else if meta.is_dir() {
                    tokio::fs::remove_dir_all(&path).await?;
                }
            }
        }
    }
    tokio::fs::create_dir_all(markdown_dir).await?;
    Ok(())
}
