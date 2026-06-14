use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use crate::core::config::Config;
use crate::core::logging::log_warn;
use crate::ingest::subprocess::MAX_INGEST_FILE_BYTES;
use crate::vector::ops::file_ingest::{SelectionPolicy, collect_files};
use crate::vector::ops::input::classify::{
    classify_file_type, is_test_path, language_name, path_extension,
};
use crate::vector::ops::{PreparedDoc, SourceDocument, SourceOrigin, prepare_source_document};

use super::super::GitHubCommonFields;
use super::super::meta::{GitHubPayloadParams, build_github_payload};
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

#[derive(Debug)]
pub(super) enum FileEmbedRead {
    Prepared(Vec<PreparedDoc>),
    SkippedCleanupBlocking,
    Empty,
}

/// Recursively walk `root` and return indexable file paths relative to `root`.
///
/// Thin wrapper over the shared `file_ingest::collect_files` engine so GitHub
/// uses the same walker as all other git providers.
pub(super) async fn collect_indexable_files(
    root: &Path,
    include_source: bool,
    exclude_paths: &[String],
) -> Result<Vec<String>> {
    let abs = collect_files(root, SelectionPolicy::Allowlist { include_source })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(abs
        .into_iter()
        .filter_map(|p| {
            p.strip_prefix(root)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .filter(|rel| !is_path_excluded(rel, exclude_paths))
        .collect())
}

/// Returns true when `rel` (a repo-relative, forward-slash path) contains any of
/// the user-supplied `--exclude-path` substrings. Empty patterns are ignored so
/// an accidental empty value cannot exclude every file.
pub(super) fn is_path_excluded(rel: &str, exclude_paths: &[String]) -> bool {
    exclude_paths
        .iter()
        .any(|pat| !pat.is_empty() && rel.contains(pat.as_str()))
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
) -> Result<FileEmbedRead, String> {
    let full_path = ctx.repo_root.join(path);

    match tokio::fs::metadata(&full_path).await {
        Ok(meta) if meta.len() > MAX_FILE_BYTES => {
            log_warn(&format!(
                "command=ingest_github skip_large_file path={path} size_bytes={}",
                meta.len()
            ));
            return Ok(FileEmbedRead::SkippedCleanupBlocking);
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
            return Ok(FileEmbedRead::SkippedCleanupBlocking);
        }
    };
    if text.trim().is_empty() {
        return Ok(FileEmbedRead::Empty);
    }

    let ext = file_extension(path);
    let base_url = format!(
        "https://github.com/{}/{}/blob/{}/{}",
        ctx.owner, ctx.name, ctx.default_branch, path
    );

    let lang = language_name(&ext).to_string();
    let ftype = classify_file_type(path).to_string();
    let is_test = is_test_path(path);
    let file_size = text.len();

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
        line_start: None,
        line_end: None,
        chunking_method: None,
        symbol_name: None, // file-level doc; per-chunk symbols not tracked at this level
        symbol_kind: None,
        symbol_extraction_status: None,
        ..Default::default()
    });

    let source_doc = SourceDocument::try_new_file(
        SourceOrigin::GitFile,
        base_url,
        path.to_string(),
        ext,
        text,
        "github",
        Some(path.to_string()),
        Some(extra),
    )
    .map_err(|err| format!("invalid source document for {path}: {err}"))?;
    let doc = prepare_source_document(source_doc)
        .await
        .map_err(|err| format!("prepare source document failed for {path}: {err}"))?;
    Ok(FileEmbedRead::Prepared(vec![doc]))
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
