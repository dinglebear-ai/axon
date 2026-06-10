use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use crate::core::config::Config;
use crate::core::logging::log_warn;
use crate::ingest::subprocess::MAX_INGEST_FILE_BYTES;
use crate::vector::ops::PreparedDoc;
use crate::vector::ops::input::classify::{
    classify_file_type, is_test_path, language_name, path_extension,
};
use crate::vector::ops::input::{
    chunk_text_with_offsets,
    code::{
        CodeChunk, chunk_code_chunks, code_symbol_extraction_status, supports_tree_sitter_chunking,
    },
};

use super::super::meta::{GitHubPayloadParams, build_github_payload};
use super::super::{GitHubCommonFields, is_indexable_doc_path, is_indexable_source_path};
use crate::ingest::git_payload::ContentKind;
const MAX_FILE_BYTES: u64 = MAX_INGEST_FILE_BYTES;

pub(super) fn file_extension(path: &str) -> String {
    path_extension(path).to_ascii_lowercase()
}

pub(super) struct FileEmbedCtx {
    pub cfg: Config,
    pub repo_root: PathBuf,
    pub owner: String,
    pub name: String,
    pub default_branch: String,
    pub repo_description: Option<String>,
    pub pushed_at: Option<String>,
    pub is_private: Option<bool>,
}

/// Recursively walk a directory and collect file paths relative to `root`.
pub(super) async fn collect_indexable_files(
    root: &Path,
    include_source: bool,
) -> Result<Vec<String>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if name == ".git" || name == "node_modules" || name == "__pycache__" {
                continue;
            }

            if entry.file_type().await?.is_dir() {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(root) {
                let rel_str = rel.to_string_lossy().to_string();
                let should_index = is_indexable_doc_path(&rel_str)
                    || (include_source && is_indexable_source_path(&rel_str));
                if should_index {
                    files.push(rel_str);
                }
            }
        }
    }

    Ok(files)
}

/// Read a single file from the cloned repo and build **one** `PreparedDoc` for the
/// entire file (all chunks as `chunks: Vec<String>`).
///
/// P-H1: Previously this emitted one `PreparedDoc` per chunk, which caused:
/// - TEI to receive a single-chunk batch per doc (batching never engaged).
/// - A guaranteed no-op stale-tail delete per chunk (filter `chunk_index >= 1`
///   on a 1-chunk doc is always empty).
///
/// Now each file produces exactly one `PreparedDoc`; TEI can batch all chunks
/// together and the stale-tail delete fires once per file with the true count.
pub(super) async fn read_file_embed_docs(
    ctx: &FileEmbedCtx,
    path: &str,
) -> Result<Vec<PreparedDoc>, String> {
    let full_path = ctx.repo_root.join(path);

    match tokio::fs::metadata(&full_path).await {
        Ok(meta) if meta.len() > MAX_FILE_BYTES => {
            log_warn(&format!(
                "command=ingest_github skip_large_file path={path} size_bytes={}",
                meta.len()
            ));
            return Ok(Vec::new());
        }
        Err(e) => {
            log_warn(&format!(
                "command=ingest_github stat_failed path={path} err={e}"
            ));
            return Err(format!("stat failed for {path}: {e}"));
        }
        _ => {}
    }

    // Split the read from UTF-8 decoding: a genuine I/O error is a failure that
    // blocks stale cleanup, but a non-UTF-8 file is benign data we simply skip
    // (matching the oversized-file skip above). Conflating the two via
    // `read_to_string` would let a single Latin-1/binary file abort the entire
    // repo ingest.
    let bytes = match tokio::fs::read(&full_path).await {
        Ok(b) => b,
        Err(e) => {
            log_warn(&format!(
                "command=ingest_github read_failed path={path} err={e}"
            ));
            return Err(format!("read failed for {path}: {e}"));
        }
    };
    let text = match String::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => {
            log_warn(&format!("command=ingest_github skip_non_utf8 path={path}"));
            return Ok(Vec::new());
        }
    };
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }

    let ext = file_extension(path);
    let ext_for_chunk = ext.clone();
    let (chunks, text) = tokio::task::spawn_blocking(move || {
        let chunks = code_or_text_chunks(&text, &ext_for_chunk);
        (chunks, text)
    })
    .await
    .map_err(|e| format!("chunk_code panicked: {e}"))?;
    if chunks.is_empty() {
        return Ok(Vec::new());
    }

    let base_url = format!(
        "https://github.com/{}/{}/blob/{}/{}",
        ctx.owner, ctx.name, ctx.default_branch, path
    );

    let lang = language_name(&ext).to_string();
    let ftype = classify_file_type(path).to_string();
    let is_test = is_test_path(path);
    let file_size = text.len();
    let symbol_status = code_symbol_extraction_status(&text, &ext, &chunks);
    if matches!(symbol_status, "skipped_large" | "none_found") {
        log_warn(&format!(
            "command=ingest_github symbol_extraction_status path={path} ext={ext} status={symbol_status}"
        ));
    }

    // Use the overall file line range (first chunk start → last chunk end).
    let file_line_start = chunks.first().map(|c| c.start_line);
    let file_line_end = chunks.last().map(|c| c.end_line);
    let chunk_method = chunking_method(&ext, chunks.first().expect("non-empty"));

    // Per-chunk payload overrides (P-H1): the file's chunks share one PreparedDoc
    // for TEI batching, but each chunk keeps its own symbol_* and code_line_*
    // metadata. The embed pipeline merges these over the file-level `extra`, so
    // the ranking symbol-boost still fires per chunk despite the per-file grouping.
    let chunk_extra: Vec<serde_json::Value> = chunks
        .iter()
        .map(|c| {
            let mut obj = serde_json::Map::new();
            obj.insert("code_line_start".into(), c.start_line.into());
            obj.insert("code_line_end".into(), c.end_line.into());
            if let Some(name) = c.symbol_name() {
                obj.insert("symbol_name".into(), name.into());
            }
            if let Some(kind) = c.symbol_kind_str() {
                obj.insert("symbol_kind".into(), kind.into());
            }
            serde_json::Value::Object(obj)
        })
        .collect();

    let chunk_texts: Vec<String> = chunks.into_iter().map(|c| c.text).collect();

    let extra = build_github_payload(&GitHubPayloadParams {
        repo: ctx.name.clone(),
        owner: ctx.owner.clone(),
        content_kind: ContentKind::File,
        branch: Some(ctx.default_branch.clone()),
        default_branch: Some(ctx.default_branch.clone()),
        repo_description: ctx.repo_description.clone(),
        pushed_at: ctx.pushed_at.clone(),
        is_private: ctx.is_private,
        file_path: Some(path.to_string()),
        file_language: Some(lang.clone()),
        file_type: Some(ftype.clone()),
        is_test: Some(is_test),
        file_size_bytes: Some(file_size),
        line_start: file_line_start,
        line_end: file_line_end,
        chunking_method: Some(chunk_method.to_string()),
        symbol_name: None, // file-level doc; per-chunk symbols not tracked at this level
        symbol_kind: None,
        symbol_extraction_status: Some(symbol_status.to_string()),
        ..Default::default()
    });

    let mut doc = PreparedDoc::ingest(
        base_url,
        "github.com".to_string(),
        chunk_texts,
        "github",
        Some(path.to_string()),
        Some(extra),
    );
    doc.chunk_extra = chunk_extra;
    Ok(vec![doc])
}

fn code_or_text_chunks(text: &str, ext: &str) -> Vec<CodeChunk> {
    // Fall back to prose chunking both for unsupported extensions (`None`) and
    // for supported-language files that tree-sitter splits into zero non-empty
    // chunks (`Some([])`) — otherwise such a file would be silently dropped.
    match chunk_code_chunks(text, ext) {
        Some(chunks) if !chunks.is_empty() => chunks,
        _ => text_chunks(text),
    }
}

fn text_chunks(text: &str) -> Vec<CodeChunk> {
    // The chunker reports each chunk's true byte offset — never re-discover the
    // position by substring search, which matches the first duplicate
    // occurrence and mislabels line ranges on files with repeated content.
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

fn chunking_method(ext: &str, chunk: &CodeChunk) -> &'static str {
    if chunk.symbol.is_some() || supports_tree_sitter_chunking(ext) {
        "tree_sitter"
    } else {
        "prose"
    }
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

pub(super) fn build_file_embed_ctx(
    cfg: &Config,
    common: &GitHubCommonFields,
    repo_root: PathBuf,
) -> Arc<FileEmbedCtx> {
    Arc::new(FileEmbedCtx {
        cfg: cfg.clone(),
        repo_root,
        owner: common.owner.clone(),
        name: common.name.clone(),
        default_branch: common.default_branch.clone(),
        repo_description: common.repo_description.clone(),
        pushed_at: common.pushed_at.clone(),
        is_private: common.is_private,
    })
}

#[cfg(test)]
#[path = "prepare_tests.rs"]
mod tests;
