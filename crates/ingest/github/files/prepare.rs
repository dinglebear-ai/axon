use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use crate::crates::core::config::Config;
use crate::crates::core::logging::log_warn;
use crate::crates::ingest::subprocess::MAX_INGEST_FILE_BYTES;
use crate::crates::vector::ops::PreparedDoc;
use crate::crates::vector::ops::input::classify::{
    classify_file_type, is_test_path, language_name, path_extension,
};
use crate::crates::vector::ops::input::{CHUNK_OVERLAP, chunk_text, code::chunk_code};

use super::super::meta::{GitHubPayloadParams, build_github_payload};
use super::super::{GitHubCommonFields, is_indexable_doc_path, is_indexable_source_path};
use super::line_range::line_range_for_chunk;

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
            return Ok(Vec::new());
        }
        _ => {}
    }

    let text = match tokio::fs::read_to_string(&full_path).await {
        Ok(t) => t,
        Err(e) => {
            log_warn(&format!(
                "command=ingest_github read_failed path={path} err={e}"
            ));
            return Ok(Vec::new());
        }
    };
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }

    let ext = file_extension(path);
    let ext_for_chunk = ext.clone();
    let (chunks, text) = tokio::task::spawn_blocking(move || {
        let chunks = chunk_code(&text, &ext_for_chunk).unwrap_or_else(|| chunk_text(&text));
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

    let mut search_start = 0usize;
    let docs = chunks
        .into_iter()
        .map(|chunk| {
            let byte_offset = text[search_start..]
                .find(chunk.as_str())
                .map(|pos| search_start + pos)
                .unwrap_or(search_start);
            search_start = next_search_start(&text, byte_offset, chunk.len());
            let (line_start, line_end) = line_range_for_chunk(&text, &chunk, byte_offset);

            let extra = build_github_payload(&GitHubPayloadParams {
                repo: ctx.name.clone(),
                owner: ctx.owner.clone(),
                content_kind: "file".into(),
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
                gh_line_start: Some(line_start),
                gh_line_end: Some(line_end),
                ..Default::default()
            });

            PreparedDoc {
                url: format!("{base_url}#L{line_start}-L{line_end}"),
                domain: "github.com".to_string(),
                chunks: vec![chunk],
                source_type: "github".to_string(),
                content_type: "text",
                title: Some(path.to_string()),
                extra: Some(extra),
            }
        })
        .collect();

    Ok(docs)
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
