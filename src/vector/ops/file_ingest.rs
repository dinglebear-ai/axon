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
    chunk_markdown_with_offsets, chunk_text_with_offsets,
    code::{ChunkSource, CodeChunk, chunk_code_chunks},
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
    let name_lower = name.to_ascii_lowercase();
    let ext = path_extension(name);
    match policy {
        SelectionPolicy::Permissive => {
            !select::is_binary_ext(ext) && !select::is_generated_filename(&name_lower)
        }
        SelectionPolicy::Allowlist { include_source } => {
            let Ok(rel) = path.strip_prefix(root) else {
                return false;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            is_indexable_doc_path(&rel) || (include_source && is_indexable_source_path(&rel))
        }
    }
}

/// Maximum number of chunks to emit for a single JSON, YAML, or TOML file.
/// Top-level-key extraction can produce hundreds of chunks for large generated
/// schemas; cap it to limit index noise without dropping structural files entirely.
const MAX_JSON_YAML_CHUNKS: usize = 64;

/// Chunk one file's content into `CodeChunk`s: AST-aware via tree-sitter when a
/// grammar exists for `ext`, otherwise prose chunks adapted to `CodeChunk`.
/// CPU-bound — callers embedding many files should wrap in `spawn_blocking`.
pub fn chunk_file(content: &str, ext: &str) -> Vec<CodeChunk> {
    chunk_file_reporting_cap(content, ext).0
}

/// Like [`chunk_file`], but also returns how many chunks the JSON/YAML/TOML cap
/// dropped (`0` if none). Callers with the file path should log a warning when
/// `dropped > 0` so the truncation is observable rather than silent — a large
/// data-bearing file (big OpenAPI spec, i18n bundle, lockfile) would otherwise
/// be partially indexed with nothing to explain why later queries miss its tail.
pub fn chunk_file_reporting_cap(content: &str, ext: &str) -> (Vec<CodeChunk>, usize) {
    if matches!(ext, "md" | "mdx" | "rst") {
        return (markdown_chunks(content), 0);
    }
    let mut chunks = match chunk_code_chunks(content, ext) {
        Some(chunks) if !chunks.is_empty() => chunks,
        _ => text_chunks(content),
    };
    let mut dropped = 0;
    if matches!(ext, "json" | "yaml" | "yml" | "toml") && chunks.len() > MAX_JSON_YAML_CHUNKS {
        dropped = chunks.len() - MAX_JSON_YAML_CHUNKS;
        chunks.truncate(MAX_JSON_YAML_CHUNKS);
    }
    (chunks, dropped)
}

/// Report the chunking method for one chunk.
pub fn chunking_method(_ext: &str, chunk: &CodeChunk) -> &'static str {
    chunk.source.as_str()
}

fn markdown_chunks(text: &str) -> Vec<CodeChunk> {
    chunk_markdown_with_offsets(text)
        .into_iter()
        .map(|(byte_start, byte_end, chunk)| {
            let line_start = line_for_byte(text, byte_start);
            let line_end = if byte_end > byte_start {
                line_for_byte(text, byte_end - 1)
            } else {
                line_start
            };
            CodeChunk {
                text: chunk,
                byte_start,
                byte_end,
                start_line: line_start,
                end_line: line_end,
                declaration_start_line: line_start,
                declaration_end_line: line_end,
                symbol: None,
                source: ChunkSource::Markdown,
            }
        })
        .collect()
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
                source: ChunkSource::Prose,
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
