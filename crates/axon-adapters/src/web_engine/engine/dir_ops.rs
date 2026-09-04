use axon_core::config::Config;
use axon_core::logging::{log_info, log_warn};
use std::error::Error;
use std::path::Path;

/// Non-blocking path existence check. Returns `false` on any I/O error.
pub(super) async fn path_exists(path: &Path) -> bool {
    tokio::fs::try_exists(path).await.unwrap_or(false)
}

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

    let parent = latest_dir
        .parent()
        .ok_or("latest_dir must have a parent directory")?;
    let name = latest_dir
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("latest_dir must have a valid file name")?;
    let staging_dir = parent.join(format!(".{name}.staging-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&staging_dir).await.map_err(|e| {
        format!(
            "failed to create latest staging dir {}: {e}",
            staging_dir.display()
        )
    })?;

    let publication: Result<(), Box<dyn Error>> = async {
        let manifest = "manifest.jsonl";
        let source_manifest = source_dir.join(manifest);
        if path_exists(&source_manifest).await {
            let src = source_manifest.clone();
            let dst = staging_dir.join(manifest);
            if let Err(error) =
                tokio::task::spawn_blocking(move || reflink_copy::reflink_or_copy(&src, dst))
                    .await?
            {
                let _ = tokio::fs::remove_dir_all(&staging_dir).await;
                return Err(error.into());
            }
        }

        let markdown = "markdown";
        let source_md = source_dir.join(markdown);
        let target_md = staging_dir.join(markdown);
        if path_exists(&source_md).await {
            tokio::fs::create_dir_all(&target_md).await?;
            let mut entries = tokio::fs::read_dir(&source_md).await?;
            let mut file_pairs: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_file() {
                    let Some(filename) = path.file_name() else {
                        continue;
                    };
                    let dst = target_md.join(filename);
                    file_pairs.push((path, dst));
                }
            }
            // Parallelize reflink copies via spawn_blocking + JoinSet, capped at 32
            // concurrent tasks to avoid overwhelming the runtime and file-descriptor
            // resources on large markdown directories.
            const MAX_COPY_CONCURRENCY: usize = 32;
            let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_COPY_CONCURRENCY));
            let mut join_set = tokio::task::JoinSet::new();
            for (src, dst) in file_pairs {
                let permit = semaphore.clone().acquire_owned().await?;
                join_set.spawn_blocking(move || {
                    let _permit = permit;
                    reflink_copy::reflink_or_copy(&src, dst)
                });
            }
            while let Some(result) = join_set.join_next().await {
                if let Err(error) = result? {
                    let _ = tokio::fs::remove_dir_all(&staging_dir).await;
                    return Err(error.into());
                }
            }
        }

        sync_tree(&staging_dir).await?;
        let had_previous = path_exists(latest_dir).await;
        if had_previous {
            let old = staging_dir.clone();
            let new = latest_dir.to_path_buf();
            tokio::task::spawn_blocking(move || {
                rustix::fs::renameat_with(
                    rustix::fs::CWD,
                    &old,
                    rustix::fs::CWD,
                    &new,
                    rustix::fs::RenameFlags::EXCHANGE,
                )
            })
            .await??;
            sync_directory(parent).await?;
            tokio::fs::remove_dir_all(&staging_dir).await?;
        } else {
            tokio::fs::rename(&staging_dir, latest_dir).await?;
            sync_directory(parent).await?;
        }

        log_info(&format!(
            "Updated 'latest' armory view via reflink: {}",
            latest_dir.display()
        ));
        Ok(())
    }
    .await;

    if publication.is_err() {
        // Best-effort cleanup preserves the original publication error while
        // preventing abandoned `.staging-*` trees on every early exit path.
        // After an exchange this path holds the replaced view; otherwise it
        // holds only the unpublished candidate.
        let _ = tokio::fs::remove_dir_all(&staging_dir).await;
    }
    publication
}

async fn sync_tree(path: &Path) -> Result<(), Box<dyn Error>> {
    let root = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        fn sync_dir(path: &Path) -> std::io::Result<()> {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    sync_dir(&entry.path())?;
                } else {
                    std::fs::File::open(entry.path())?.sync_all()?;
                }
            }
            std::fs::File::open(path)?.sync_all()
        }
        sync_dir(&root)
    })
    .await??;
    Ok(())
}

async fn sync_directory(path: &Path) -> Result<(), Box<dyn Error>> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || std::fs::File::open(path)?.sync_all()).await??;
    Ok(())
}

/// Prepare the output directory before a crawl run.
///
/// - Cache or etag-conditional mode: archives existing `markdown/` to `markdown.old/`
///   (Recycling Bin Pattern) so the collector can surgically reuse unchanged pages.
///   `--etag-conditional` needs the recycling bin even without `--cache` so that
///   304-skipped pages can be relinked from `markdown.old` during reconciliation.
/// - Non-cache/non-etag mode: wipes the directory unless `AXON_NO_WIPE` is set.
/// - Always ensures `markdown/` exists at the end.
pub(super) async fn prepare_crawl_output_dir(
    output_dir: &Path,
    markdown_dir: &Path,
    recycling_bin: &Path,
    cfg: &Config,
) -> Result<(), Box<dyn Error>> {
    if path_exists(output_dir).await {
        if cfg.cache || cfg.etag_conditional {
            if path_exists(markdown_dir).await {
                if path_exists(recycling_bin).await {
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

#[cfg(test)]
#[path = "dir_ops_tests.rs"]
mod tests;
