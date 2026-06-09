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
    CHUNK_OVERLAP, chunk_text,
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

/// Advance the chunk search cursor past the current chunk, walking back one
/// character at a time so the next search begins inside the overlap window.
pub fn next_search_start(text: &str, byte_offset: usize, chunk_len: usize) -> usize {
    let chunk_end = (byte_offset + chunk_len).min(text.len());
    let mut pos = chunk_end;
    for _ in 0..CHUNK_OVERLAP {
        if pos == 0 {
            break;
        }
        pos -= 1;
        while pos > 0 && !text.is_char_boundary(pos) {
            pos -= 1;
        }
    }
    pos
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

/// Read a single file from the cloned repo and build one `PreparedDoc` per chunk.
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

    let attrs = FileDocAttrs {
        base_url,
        path,
        ext: &ext,
        lang: &lang,
        ftype: &ftype,
        is_test,
        file_size,
        symbol_status,
    };
    let docs = chunks
        .into_iter()
        .map(|chunk| prepared_doc_for_chunk(ctx, &attrs, chunk))
        .collect();

    Ok(docs)
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
    chunk_text(text)
        .into_iter()
        .scan(0usize, |search_start, chunk| {
            let byte_offset = text[*search_start..]
                .find(chunk.as_str())
                .map(|pos| *search_start + pos)
                .unwrap_or(*search_start);
            let chunk_len = chunk.len();
            *search_start = next_search_start(text, byte_offset, chunk_len);
            let line_start = line_for_byte(text, byte_offset);
            // Inclusive end so a chunk ending on a newline maps to its own last
            // line, not the next one.
            let line_end = if chunk_len > 0 {
                line_for_byte(text, byte_offset + chunk_len - 1)
            } else {
                line_for_byte(text, byte_offset)
            };
            Some(CodeChunk {
                text: chunk,
                byte_start: byte_offset,
                byte_end: byte_offset + chunk_len,
                start_line: line_start,
                end_line: line_end,
                declaration_start_line: line_start,
                declaration_end_line: line_end,
                symbol: None,
            })
        })
        .collect()
}

struct FileDocAttrs<'a> {
    base_url: String,
    path: &'a str,
    ext: &'a str,
    lang: &'a str,
    ftype: &'a str,
    is_test: bool,
    file_size: usize,
    symbol_status: &'static str,
}

fn prepared_doc_for_chunk(
    ctx: &FileEmbedCtx,
    attrs: &FileDocAttrs<'_>,
    chunk: CodeChunk,
) -> PreparedDoc {
    let line_start = chunk.start_line;
    let line_end = chunk.end_line;
    let extra = build_github_payload(&GitHubPayloadParams {
        repo: ctx.name.clone(),
        owner: ctx.owner.clone(),
        content_kind: ContentKind::File,
        branch: Some(ctx.default_branch.clone()),
        default_branch: Some(ctx.default_branch.clone()),
        repo_description: ctx.repo_description.clone(),
        pushed_at: ctx.pushed_at.clone(),
        is_private: ctx.is_private,
        file_path: Some(attrs.path.to_string()),
        file_language: Some(attrs.lang.to_string()),
        file_type: Some(attrs.ftype.to_string()),
        is_test: Some(attrs.is_test),
        file_size_bytes: Some(attrs.file_size),
        line_start: Some(line_start),
        line_end: Some(line_end),
        chunking_method: Some(chunking_method(attrs.ext, &chunk).to_string()),
        symbol_name: chunk.symbol_name().map(str::to_string),
        symbol_kind: chunk.symbol_kind_str().map(str::to_string),
        symbol_extraction_status: Some(attrs.symbol_status.to_string()),
        ..Default::default()
    });

    PreparedDoc {
        url: format!("{}#L{line_start}-L{line_end}", attrs.base_url),
        domain: "github.com".to_string(),
        chunks: vec![chunk.text],
        source_type: "github".to_string(),
        content_type: "text",
        title: Some(attrs.path.to_string()),
        extra: Some(extra),
        extractor_name: None,
        structured: None,
    }
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
