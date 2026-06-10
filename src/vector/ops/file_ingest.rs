//! Shared filesystem → chunked-document engine.
//!
//! One recursive walker + one chunk-selection adapter shared by all callers;
//! per-chunk `PreparedDoc` structure varies by provider. Every git provider
//! (after clone) and the local `embed <dir>` path supply only the per-file URL
//! and payload; this module owns file selection and the code-vs-prose chunk
//! decision.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow as anyhow_err};

use crate::core::logging::log_warn;
use crate::ingest::github::{is_indexable_doc_path, is_indexable_source_path};
use crate::vector::ops::input::classify::path_extension;
use crate::vector::ops::input::select;
use crate::vector::ops::input::{
    chunk_text_with_offsets,
    code::{CodeChunk, chunk_code_chunks},
};

/// Which files a directory walk should yield.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionPolicy {
    /// Git-repo ingest: curated allowlist of doc/source extensions.
    Allowlist { include_source: bool },
    /// Local `embed <dir>`: permissive — everything except binary extensions.
    Permissive,
}

/// Recursively collect files under `root` per `policy`.
///
/// Resilience: the top-level `root` read is a hard error (nothing to embed if
/// the target is unreadable), but an unreadable subdirectory is logged and
/// skipped. Pruned directories (`select::is_pruned_dir`: `.git`, `node_modules`,
/// `target`, …) are never descended into. Symlinks are skipped (their
/// `file_type` is neither file nor dir). Returned paths are sorted.
pub async fn collect_files(root: &Path, policy: SelectionPolicy) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut at_root = true;
    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(e) if at_root => {
                return Err(anyhow_err!(
                    "invalid ingest directory {}: {e}",
                    dir.display()
                ));
            }
            Err(e) => {
                log_warn(&format!(
                    "command=ingest skip_unreadable_dir path={} err={e}",
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
                        "command=ingest dir_iter_error path={} err={e}",
                        dir.display()
                    ));
                    continue;
                }
            };
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let ft = match entry.file_type().await {
                Ok(ft) => ft,
                Err(e) => {
                    log_warn(&format!(
                        "command=ingest skip_unknown_type path={} err={e}",
                        path.display()
                    ));
                    continue;
                }
            };
            if ft.is_dir() {
                if !select::is_pruned_dir(name) {
                    stack.push(path);
                }
            } else if ft.is_file() && include_file(&path, root, policy) {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn include_file(path: &Path, root: &Path, policy: SelectionPolicy) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let ext = path_extension(name);
    match policy {
        SelectionPolicy::Permissive => !select::is_binary_ext(ext),
        SelectionPolicy::Allowlist { include_source } => {
            let Ok(rel) = path.strip_prefix(root) else {
                return false;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            is_indexable_doc_path(&rel) || (include_source && is_indexable_source_path(&rel))
        }
    }
}

/// Chunk one file's content into `CodeChunk`s: AST-aware via tree-sitter when a
/// grammar exists for `ext`, otherwise prose chunks adapted to `CodeChunk`.
/// CPU-bound — callers embedding many files should wrap in `spawn_blocking`.
pub fn chunk_file(content: &str, ext: &str) -> Vec<CodeChunk> {
    match chunk_code_chunks(content, ext) {
        Some(chunks) if !chunks.is_empty() => chunks,
        _ => text_chunks(content),
    }
}

/// Report the chunking method for one chunk: `"tree_sitter"` when a tree-sitter
/// grammar exists for `ext` OR the chunk carries a symbol. Note: returns
/// `"tree_sitter"` even for grammar-supported files where no symbols were
/// extracted (tree-sitter ran but found nothing); returns `"prose"` only when
/// no grammar exists and the chunk has no symbol.
///
/// Simplified to use only `chunk.symbol` as the signal, eliminating false
/// positives where prose fallback chunks on grammar-supported extensions were
/// incorrectly labeled `"tree_sitter"`.
pub fn chunking_method(_ext: &str, chunk: &CodeChunk) -> &'static str {
    if chunk.symbol.is_some() {
        "tree_sitter"
    } else {
        "prose"
    }
}

fn text_chunks(text: &str) -> Vec<CodeChunk> {
    chunk_text_with_offsets(text)
        .into_iter()
        .map(|(byte_offset, chunk)| {
            let chunk_len = chunk.len();
            let line_start = line_for_byte(text, byte_offset);
            // Inclusive end so a chunk ending on a newline maps to its own last
            // line, not the next one.
            let line_end = if chunk_len > 0 {
                line_for_byte(text, byte_offset + chunk_len - 1)
            } else {
                line_start
            };
            CodeChunk {
                text: chunk,
                byte_start: byte_offset,
                byte_end: byte_offset + chunk_len,
                start_line: line_start,
                end_line: line_end,
                declaration_start_line: line_start,
                declaration_end_line: line_end,
                symbol: None,
            }
        })
        .collect()
}

fn line_for_byte(content: &str, byte: usize) -> u32 {
    // Snap to a char boundary: an inclusive end may land inside a multibyte
    // character, and slicing on a non-boundary panics.
    let mut capped = byte.min(content.len());
    while capped > 0 && !content.is_char_boundary(capped) {
        capped -= 1;
    }
    content[..capped].bytes().filter(|b| *b == b'\n').count() as u32 + 1
}

#[cfg(test)]
#[path = "file_ingest_tests.rs"]
mod tests;
