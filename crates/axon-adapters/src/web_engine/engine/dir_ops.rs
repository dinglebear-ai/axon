use axon_core::config::Config;
use axon_core::logging::{log_info, log_warn};
use std::error::Error;
use std::path::Path;

pub(super) const LATEST_CLEANUP_DEBT_MARKER: &str = ".axon-latest-cleanup-debt";

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
    update_latest_reflink_with_failure(source_dir, latest_dir, None).await
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LatestFailurePoint {
    Cleanup,
    ParentSync,
}

pub(super) async fn update_latest_reflink_with_failure(
    source_dir: &Path,
    latest_dir: &Path,
    failure: Option<LatestFailurePoint>,
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
    sweep_latest_cleanup_debt(parent, name).await;
    let staging_dir = parent.join(format!(".{name}.staging-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&staging_dir).await.map_err(|e| {
        format!(
            "failed to create latest staging dir {}: {e}",
            staging_dir.display()
        )
    })?;

    let mut exchanged = false;
    let publication: Result<(), Box<dyn Error>> = async {
        populate_latest_staging(source_dir, &staging_dir).await?;
        let had_previous = validate_latest_destination(latest_dir).await?;
        if had_previous {
            exchange_directories(&staging_dir, latest_dir).await?;
            exchanged = true;
            if !is_real_directory(&staging_dir).await? {
                exchange_directories(&staging_dir, latest_dir).await?;
                exchanged = false;
                sync_directory(parent).await?;
                return Err("latest_dir changed to an unsafe path during publication".into());
            }
            let parent_sync = if failure == Some(LatestFailurePoint::ParentSync) {
                Err("injected latest parent sync failure".into())
            } else {
                sync_directory(parent).await
            };
            if let Err(sync_error) = parent_sync {
                exchange_directories(&staging_dir, latest_dir).await?;
                exchanged = false;
                sync_directory(parent).await?;
                return Err(format!(
                    "latest view exchange was rolled back after directory sync failed: {sync_error}"
                )
                .into());
            }
            let cleanup = if failure == Some(LatestFailurePoint::Cleanup) {
                Err(std::io::Error::other(
                    "injected replaced-view cleanup failure",
                ))
            } else {
                tokio::fs::remove_dir_all(&staging_dir).await
            };
            if let Err(error) = cleanup {
                // The exchange and parent-directory sync are the commit point.
                // Leave the uniquely named replaced view as explicit cleanup
                // debt rather than reporting that a successful publication failed.
                mark_latest_cleanup_debt(&staging_dir).await;
                log_warn(&format!(
                    "latest view committed; deferred cleanup of {}: {error}",
                    staging_dir.display()
                ));
            }
        } else {
            tokio::fs::rename(&staging_dir, latest_dir).await?;
            let parent_sync = if failure == Some(LatestFailurePoint::ParentSync) {
                Err("injected latest parent sync failure".into())
            } else {
                sync_directory(parent).await
            };
            if let Err(sync_error) = parent_sync {
                tokio::fs::rename(latest_dir, &staging_dir).await?;
                sync_directory(parent).await?;
                return Err(format!(
                    "initial latest view was rolled back after directory sync failed: {sync_error}"
                )
                .into());
            }
        }

        log_info(&format!(
            "Updated 'latest' armory view via reflink: {}",
            latest_dir.display()
        ));
        Ok(())
    }
    .await;

    if publication.is_err() && !exchanged {
        // Best-effort cleanup preserves the original publication error while
        // preventing abandoned `.staging-*` trees on every early exit path.
        // After an exchange this path holds the replaced view; otherwise it
        // holds only the unpublished candidate.
        let _ = tokio::fs::remove_dir_all(&staging_dir).await;
    }
    publication
}

// LEARNED: duplicating the atomic exchange syscall across rollback branches
// makes filesystem-critical flags and error propagation easy to drift.
// PATTERN: keep the blocking rename exchange behind one async helper and reuse
// it for both publication and compensation.
#[cfg(unix)]
async fn exchange_directories(left: &Path, right: &Path) -> Result<(), Box<dyn Error>> {
    let left = left.to_path_buf();
    let right = right.to_path_buf();
    tokio::task::spawn_blocking(move || {
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            &left,
            rustix::fs::CWD,
            &right,
            rustix::fs::RenameFlags::EXCHANGE,
        )
    })
    .await??;
    Ok(())
}

#[cfg(not(unix))]
async fn exchange_directories(left: &Path, right: &Path) -> Result<(), Box<dyn Error>> {
    let left = left.to_path_buf();
    let right = right.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let parent = left.parent().ok_or("publication directory has no parent")?;
        let swap = parent.join(format!(".axon-directory-swap-{}", uuid::Uuid::new_v4()));
        std::fs::rename(&left, &swap)?;
        if let Err(error) = std::fs::rename(&right, &left) {
            let _ = std::fs::rename(&swap, &left);
            return Err(error.into());
        }
        if let Err(error) = std::fs::rename(&swap, &right) {
            let _ = std::fs::rename(&left, &right);
            let _ = std::fs::rename(&swap, &left);
            return Err(error.into());
        }
        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
    .await?
    .map_err(|error| -> Box<dyn Error> { error })
}

async fn populate_latest_staging(
    source_dir: &Path,
    staging_dir: &Path,
) -> Result<(), Box<dyn Error>> {
    let manifest = "manifest.jsonl";
    let source_manifest = source_dir.join(manifest);
    if path_exists(&source_manifest).await {
        let src = source_manifest.clone();
        let dst = staging_dir.join(manifest);
        tokio::task::spawn_blocking(move || reflink_copy::reflink_or_copy(&src, dst)).await??;
    }

    let markdown = "markdown";
    let source_md = source_dir.join(markdown);
    let target_md = staging_dir.join(markdown);
    if path_exists(&source_md).await {
        tokio::fs::create_dir_all(&target_md).await?;
        let mut entries = tokio::fs::read_dir(&source_md).await?;
        let mut file_pairs = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file()
                && let Some(filename) = path.file_name()
            {
                let dst = target_md.join(filename);
                file_pairs.push((path, dst));
            }
        }
        copy_latest_markdown(file_pairs).await?;
    }

    let _ = sync_tree(staging_dir).await?;
    Ok(())
}

async fn copy_latest_markdown(
    file_pairs: Vec<(std::path::PathBuf, std::path::PathBuf)>,
) -> Result<(), Box<dyn Error>> {
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
        result??;
    }
    Ok(())
}

async fn mark_latest_cleanup_debt(path: &Path) {
    let owned_path = path.to_path_buf();
    let marked =
        tokio::task::spawn_blocking(move || write_cleanup_marker_relative(&owned_path, || Ok(())))
            .await
            .unwrap_or_else(|error| Err(std::io::Error::other(error.to_string())));
    if let Err(error) = marked {
        log_warn(&format!(
            "could not record latest-view cleanup debt for {}: {error}",
            path.display()
        ));
    }
}

#[cfg(unix)]
fn write_cleanup_marker_relative(
    path: &Path,
    after_directory_open: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    use rustix::fs::{Mode, OFlags, open, openat};
    use std::io::Write as _;

    let directory = open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW,
        Mode::empty(),
    )?;
    after_directory_open()?;
    let marker = openat(
        &directory,
        LATEST_CLEANUP_DEBT_MARKER,
        OFlags::WRONLY | OFlags::CLOEXEC | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW,
        Mode::RUSR | Mode::WUSR,
    )?;
    let mut marker_file = std::fs::File::from(marker);
    marker_file.write_all(b"replaced latest view\n")?;
    marker_file.sync_all()?;
    std::fs::File::from(directory).sync_all()
}

#[cfg(not(unix))]
fn write_cleanup_marker_relative(
    _path: &Path,
    _after_directory_open: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "descriptor-relative cleanup markers require Unix openat",
    ))
}

async fn validate_latest_destination(path: &Path) -> Result<bool, Box<dyn Error>> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(format!("latest_dir must not be a symlink: {}", path.display()).into())
        }
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => Err(format!("latest_dir must be a directory: {}", path.display()).into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "failed to inspect latest_dir {} before publication: {error}",
            path.display()
        )
        .into()),
    }
}

async fn is_real_directory(path: &Path) -> Result<bool, Box<dyn Error>> {
    let metadata = tokio::fs::symlink_metadata(path).await.map_err(|error| {
        format!(
            "failed to inspect exchanged latest_dir {}: {error}",
            path.display()
        )
    })?;
    Ok(metadata.is_dir() && !metadata.file_type().is_symlink())
}

async fn sweep_latest_cleanup_debt(parent: &Path, latest_name: &str) {
    let prefix = format!(".{latest_name}.staging-");
    let mut entries = match tokio::fs::read_dir(parent).await {
        Ok(entries) => entries,
        Err(error) => {
            log_warn(&format!(
                "could not scan latest-view cleanup debt in {}: {error}",
                parent.display()
            ));
            return;
        }
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(id) = name.strip_prefix(&prefix) else {
            continue;
        };
        if uuid::Uuid::parse_str(id).is_err() {
            continue;
        }
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let marker = entry.path().join(LATEST_CLEANUP_DEBT_MARKER);
        let Ok(metadata) = tokio::fs::symlink_metadata(&marker).await else {
            continue;
        };
        if !metadata.file_type().is_file() {
            continue;
        }

        // LEARNED: a staging-shaped name alone cannot distinguish abandoned
        // cleanup debt from another publication's live candidate.
        // PATTERN: sweep only UUID-shaped sibling directories carrying the
        // marker written after the durable exchange commit point.
        if let Err(error) = tokio::fs::remove_dir_all(entry.path()).await {
            log_warn(&format!(
                "could not retry latest-view cleanup for {}: {error}",
                entry.path().display()
            ));
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct SyncTreeStats {
    files: usize,
    directories: usize,
}

async fn sync_tree(path: &Path) -> Result<SyncTreeStats, Box<dyn Error>> {
    let root = path.to_path_buf();
    let (files, mut directories) = tokio::task::spawn_blocking(move || {
        fn collect(
            path: &Path,
            files: &mut Vec<std::path::PathBuf>,
            directories: &mut Vec<std::path::PathBuf>,
        ) -> std::io::Result<()> {
            directories.push(path.to_path_buf());
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    collect(&entry.path(), files, directories)?;
                } else {
                    files.push(entry.path());
                }
            }
            Ok(())
        }
        let mut files = Vec::new();
        let mut directories = Vec::new();
        collect(&root, &mut files, &mut directories)?;
        Ok::<_, std::io::Error>((files, directories))
    })
    .await??;

    const MAX_SYNC_CONCURRENCY: usize = 32;
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_SYNC_CONCURRENCY));
    let file_count = files.len();
    let mut joins = tokio::task::JoinSet::new();
    for path in files {
        let permit = semaphore.clone().acquire_owned().await?;
        joins.spawn_blocking(move || {
            let _permit = permit;
            std::fs::File::open(path)?.sync_all()
        });
    }
    while let Some(result) = joins.join_next().await {
        result??;
    }

    // Sync children before parents so all newly-created directory entries are
    // durable. File data syncs above are parallelized as one bounded batch.
    directories.sort_by_key(|directory| std::cmp::Reverse(directory.components().count()));
    for directory in &directories {
        sync_directory(directory).await?;
    }
    Ok(SyncTreeStats {
        files: file_count,
        directories: directories.len(),
    })
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
