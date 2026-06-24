use super::super::tei_manifest::read_manifest_url_map;
use crate::ops::input::classify::path_extension;
use crate::ops::input::select;
use axon_core::logging::log_warn;
use std::error::Error;
use std::path::{Path, PathBuf};

pub(super) type InputRecord = (String, String, Option<serde_json::Value>);

/// Per-file size ceiling for local embeds, matching the server validator's
/// `mcp_embed_max_local_bytes` default. A multi-megabyte machine-generated
/// file (a 31 MB JSON index, say) grinds the prose chunker for minutes and
/// floods the collection with thousands of junk chunks — directory walks skip
/// such files with a warning; an explicitly named file is a hard error so the
/// user learns the cap instead of silently embedding nothing.
const MAX_LOCAL_EMBED_FILE_BYTES: u64 = 10 * 1024 * 1024;

pub(super) async fn read_inputs(input: &str) -> Result<Vec<InputRecord>, Box<dyn Error>> {
    read_inputs_with_max_bytes(input, MAX_LOCAL_EMBED_FILE_BYTES).await
}

pub(super) async fn read_inputs_with_max_bytes(
    input: &str,
    max_file_bytes: u64,
) -> Result<Vec<InputRecord>, Box<dyn Error>> {
    let path = PathBuf::from(input);
    // POSIX-style symlink policy (like `du` / `find -H` / `chown -H`): a
    // symlink named explicitly on the command line is followed —
    // `tokio::fs::metadata` resolves it — while symlinks *encountered during
    // traversal* are skipped (see collect_embed_files). The server path is
    // stricter and rejects a symlinked root outright (services/embed.rs).
    match tokio::fs::metadata(&path).await {
        Ok(meta) if meta.is_file() => {
            if meta.len() > max_file_bytes {
                return Err(format!(
                    "embed input {input} is {} bytes, over the {max_file_bytes}-byte local file cap",
                    meta.len()
                )
                .into());
            }
            Ok(vec![(
                path.to_string_lossy().to_string(),
                tokio::fs::read_to_string(&path).await?,
                None,
            )])
        }
        Ok(meta) if meta.is_dir() => {
            let manifest_urls = read_manifest_url_map(&path);
            let files = collect_embed_files(&path).await?;
            let mut out = Vec::new();
            for p in files {
                // Oversized files are skipped (not fatal) on directory walks —
                // a single machine-generated multi-MB file must not grind the
                // chunker or sink an otherwise-fine embed.
                match tokio::fs::metadata(&p).await {
                    Ok(meta) if meta.len() > max_file_bytes => {
                        log_warn(&format!(
                            "command=embed skip_oversized_file path={} size_bytes={} cap_bytes={max_file_bytes}",
                            p.display(),
                            meta.len()
                        ));
                        continue;
                    }
                    _ => {}
                }
                // A canonicalize failure silently defeats the manifest lookup
                // (keyed on the canonical path): the file would re-embed even
                // when unchanged and lose its crawl URL / structured payload.
                // Leave a trace so a manifest-skip miss is attributable.
                let canonical = std::fs::canonicalize(&p).unwrap_or_else(|e| {
                    log_warn(&format!(
                        "command=embed canonicalize_failed path={} err={e}",
                        p.display()
                    ));
                    p.clone()
                });
                let (source, changed, structured) = manifest_urls
                    .get(&canonical)
                    .map(|(u, c, s)| (u.clone(), *c, s.clone()))
                    .unwrap_or_else(|| (p.to_string_lossy().to_string(), true, None));
                if !changed {
                    continue;
                }
                // Skip-on-error: a single unreadable/non-UTF-8 file (one that
                // slipped the binary-extension filter) must not fail the whole
                // embed job — log and move on.
                match tokio::fs::read_to_string(&p).await {
                    Ok(content) => out.push((source, content, structured)),
                    Err(e) => {
                        log_warn(&format!(
                            "command=embed skip_unreadable_file path={} err={e}",
                            p.display()
                        ));
                    }
                }
            }
            Ok(out)
        }
        // Path-shaped input that doesn't resolve is a hard error, never
        // free-text: an embed job for a host path claimed by a worker in a
        // different filesystem namespace (the axon container cannot see
        // /home/<user>/...) used to fall through here and "successfully"
        // embed the literal path string as a one-chunk document.
        _ if select::looks_path_like(input) => Err(format!(
            "local embed path does not exist or is not visible to this process: {input} \
             (local paths must be embedded by a process on the same filesystem — \
             run `axon embed {input}` on the machine that owns the path)"
        )
        .into()),
        _ => Ok(vec![(input.to_string(), input.to_string(), None)]),
    }
}

/// Recursively collect embeddable files under `root`, descending into
/// subdirectories. Prunes VCS/dependency/build directories (`select::is_pruned_dir`)
/// and skips known-binary file extensions (`select::is_binary_ext`) before any
/// read. Symlinks are skipped (their `file_type` is neither file nor dir). The
/// returned paths are sorted for deterministic embed order.
///
/// Resilience matches the file read: the **top-level** `root` read is a hard
/// error (nothing to embed if the target is unreadable), but an unreadable
/// **subdirectory** is logged and skipped rather than failing the whole embed —
/// so one permission-protected subtree doesn't sink an otherwise-fine job.
async fn collect_embed_files(root: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut at_root = true;
    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(e) if at_root => {
                return Err(format!("invalid embed directory {}: {e}", dir.display()).into());
            }
            Err(e) => {
                log_warn(&format!(
                    "command=embed skip_unreadable_dir path={} err={e}",
                    dir.display()
                ));
                at_root = false;
                continue;
            }
        };
        at_root = false;
        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(e) => {
                    log_warn(&format!(
                        "command=embed dir_iter_error path={} err={e}",
                        dir.display()
                    ));
                    break;
                }
            };
            let p = entry.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let Ok(file_type) = entry.file_type().await else {
                log_warn(&format!(
                    "command=embed skip_unknown_type path={}",
                    p.display()
                ));
                continue;
            };
            if file_type.is_dir() {
                if !select::is_pruned_dir(name) {
                    stack.push(p);
                }
            } else if file_type.is_file() && !select::is_binary_ext(path_extension(name)) {
                files.push(p);
            }
        }
    }
    files.sort();
    Ok(files)
}
