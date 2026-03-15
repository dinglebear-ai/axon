use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_info, log_warn};
use crate::crates::vector::ops::input::classify::{
    classify_file_type, is_test_path, language_name,
};
use crate::crates::vector::ops::input::{chunk_text, code::chunk_code};
use crate::crates::vector::ops::{PreparedDoc, embed_prepared_docs};
use anyhow::{Result, bail};
use futures_util::stream::{self, StreamExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::meta::{GitHubPayloadParams, build_github_payload};
use super::{GitHubCommonFields, is_indexable_doc_path, is_indexable_source_path};

const FILE_PROGRESS_EVERY: usize = 25;

/// Skip files larger than 5 MB to avoid memory spikes from generated/minified blobs.
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// Flush accumulated PreparedDocs to the embed pipeline every N docs to bound memory.
const EMBED_BATCH_SIZE: usize = 50;

/// Extensions that have tree-sitter grammar support for AST-aware chunking.
const TREE_SITTER_EXTENSIONS: &[&str] = &["rs", "py", "js", "jsx", "ts", "tsx", "go", "sh", "bash"];

/// Determine the chunking method based on file extension.
fn chunking_method(ext: &str) -> &'static str {
    if TREE_SITTER_EXTENSIONS.contains(&ext) {
        "tree-sitter"
    } else {
        "prose"
    }
}

/// Extract the file extension from a path (lowercase, no dot).
fn file_extension(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Run `git clone --depth=1` into a temp directory.
///
/// Tries authenticated clone first (if token provided), then falls back to
/// unauthenticated for public repos. Uses `token` prefix (not `Bearer`) for
/// the Authorization header — works with both classic (`ghp_`) and
/// fine-grained (`github_pat_`) GitHub PATs.
async fn clone_repo(
    common: &GitHubCommonFields,
    branch: &str,
    token: Option<&str>,
) -> Result<tempfile::TempDir> {
    let tmp = tempfile::tempdir()?;
    let tmp_path = tmp.path().to_string_lossy().to_string();
    let clone_url = format!("https://github.com/{}/{}.git", common.owner, common.name);

    let base_args = [
        "clone",
        "--depth=1",
        "--branch",
        branch,
        "--single-branch",
        "--",
        &clone_url,
        &tmp_path,
    ];

    // Try authenticated first, fall back to unauthenticated for public repos.
    if let Some(t) = token {
        let output = tokio::process::Command::new("git")
            .args(base_args)
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.extraHeader")
            .env("GIT_CONFIG_VALUE_0", format!("Authorization: token {t}"))
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("git not found or failed to start: {e}"))?;

        if output.status.success() {
            return Ok(tmp);
        }

        // Auth failed — retry without token (public repos don't need it).
        log_warn(&format!(
            "command=ingest_github auth_clone_failed repo={}/{} retrying_unauthenticated",
            common.owner, common.name
        ));
        // Clean up the failed partial clone before retrying.
        let _ = tokio::fs::remove_dir_all(tmp.path()).await;
        tokio::fs::create_dir_all(tmp.path()).await.map_err(|e| {
            anyhow::anyhow!("failed to recreate tmp dir for unauthenticated retry: {e}")
        })?;
    }

    let output = tokio::process::Command::new("git")
        .args(base_args)
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("git not found or failed to start: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git clone failed (exit {}): {}",
            output.status,
            stderr.trim()
        );
    }

    Ok(tmp)
}

/// Recursively walk a directory and collect file paths relative to `root`.
async fn collect_indexable_files(root: &Path, include_source: bool) -> Result<Vec<String>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip .git and common non-indexable directories
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

/// Context for per-file embed tasks, built once from the outer scope.
struct FileEmbedCtx {
    cfg: Config,
    repo_root: PathBuf,
    owner: String,
    name: String,
    default_branch: String,
    repo_description: Option<String>,
    pushed_at: Option<String>,
    is_private: Option<bool>,
}

/// Compute line range (1-indexed, inclusive) for a chunk within content.
///
/// Finds the chunk's byte offset via substring search, then counts newlines
/// preceding the start and within the chunk to derive start/end lines.
fn line_range_for_chunk(content: &str, chunk: &str) -> (u32, u32) {
    let byte_offset = content.find(chunk).unwrap_or(0);
    // Lines before this chunk (1-indexed).
    let start_line = content[..byte_offset]
        .bytes()
        .filter(|&b| b == b'\n')
        .count() as u32
        + 1;
    let lines_in_chunk = chunk.bytes().filter(|&b| b == b'\n').count() as u32;
    let end_line = start_line + lines_in_chunk;
    (start_line, end_line)
}

/// Read a single file from the cloned repo and build one `PreparedDoc` per chunk.
///
/// Each doc carries its own `gh_line_start`/`gh_line_end` metadata so the embed
/// pipeline writes per-chunk line ranges into Qdrant. This enables linking chunks
/// directly to the GitHub source view (`#L<start>-L<end>`).
///
/// Empty or unreadable files return `Ok(vec![])`.
async fn read_file_embed_docs(ctx: &FileEmbedCtx, path: &str) -> Result<Vec<PreparedDoc>, String> {
    let full_path = ctx.repo_root.join(path);

    // Guard against huge generated/minified files that would spike memory.
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
    let chunks = chunk_code(&text, &ext).unwrap_or_else(|| chunk_text(&text));
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
    let method = chunking_method(&ext).to_string();
    let file_size = text.len();

    // One PreparedDoc per chunk so each carries its own line range metadata.
    let docs = chunks
        .iter()
        .map(|chunk| {
            let (line_start, line_end) = line_range_for_chunk(&text, chunk);

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
                chunking_method: Some(method.clone()),
                gh_line_start: Some(line_start),
                gh_line_end: Some(line_end),
                ..Default::default()
            });

            // Append GitHub line-range fragment for direct linking.
            let url = format!("{base_url}#L{line_start}-L{line_end}");

            PreparedDoc {
                url,
                domain: "github.com".to_string(),
                chunks: vec![chunk.clone()],
                source_type: "github".to_string(),
                content_type: "text",
                title: Some(path.to_string()),
                extra: Some(extra),
            }
        })
        .collect();

    Ok(docs)
}

/// Clone the repo and embed all indexable files concurrently.
///
/// Uses `git clone --depth=1` to get all files in one operation instead of
/// fetching each file individually via the GitHub API. Files are read from
/// disk and embedded with AST-aware chunking (tree-sitter) where supported.
///
/// If `progress_tx` is provided, sends `{"files_done", "files_total", "chunks_embedded"}`
/// after every file completes so the worker can persist live progress to the DB.
pub async fn embed_files(
    cfg: &Config,
    common: &GitHubCommonFields,
    include_source: bool,
    token: Option<&str>,
    progress_tx: Option<&mpsc::Sender<serde_json::Value>>,
) -> Result<usize> {
    // Heartbeat: signal activity before git clone (may take minutes on large repos)
    send_progress(
        progress_tx,
        serde_json::json!({
            "phase": "cloning",
            "repo": common.repo_slug,
        }),
    )
    .await;

    let tmp = clone_repo(common, &common.default_branch, token).await?;
    let repo_root = tmp.path().to_path_buf();

    // Heartbeat: clone complete, about to enumerate files
    send_progress(
        progress_tx,
        serde_json::json!({
            "phase": "enumerating_files",
            "repo": common.repo_slug,
        }),
    )
    .await;

    let file_items = collect_indexable_files(&repo_root, include_source).await?;
    let files_total = file_items.len();

    log_info(&format!(
        "github clone complete indexable={files_total} repo={}",
        common.repo_slug
    ));

    let ctx = Arc::new(FileEmbedCtx {
        cfg: cfg.clone(),
        repo_root,
        owner: common.owner.clone(),
        name: common.name.clone(),
        default_branch: common.default_branch.clone(),
        repo_description: common.repo_description.clone(),
        pushed_at: common.pushed_at.clone(),
        is_private: common.is_private,
    });

    let (chunks_embedded, failed) =
        collect_and_embed_batched(&ctx, file_items, files_total, progress_tx).await?;

    send_progress(
        progress_tx,
        serde_json::json!({
            "files_done": files_total,
            "files_total": files_total,
            "chunks_embedded": chunks_embedded,
            "phase": "embedded_files",
        }),
    )
    .await;

    log_info(&format!(
        "github files_embedded total={files_total} failed={failed} chunks={chunks_embedded}"
    ));
    Ok(chunks_embedded)
}

/// Stream file reads and flush accumulated docs to the embed pipeline every
/// `EMBED_BATCH_SIZE` docs, bounding peak memory instead of buffering all files.
async fn collect_and_embed_batched(
    ctx: &Arc<FileEmbedCtx>,
    file_items: Vec<String>,
    files_total: usize,
    progress_tx: Option<&mpsc::Sender<serde_json::Value>>,
) -> Result<(usize, usize)> {
    let concurrency = std::cmp::min(ctx.cfg.batch_concurrency, 64);
    let mut file_stream = stream::iter(file_items)
        .map(|path| {
            let ctx = Arc::clone(ctx);
            async move { read_file_embed_docs(ctx.as_ref(), &path).await }
        })
        .buffer_unordered(concurrency);

    let mut batch: Vec<PreparedDoc> = Vec::with_capacity(EMBED_BATCH_SIZE);
    let mut files_done = 0usize;
    let mut failed = 0usize;
    let mut total_chunks = 0usize;

    while let Some(result) = file_stream.next().await {
        files_done += 1;
        match result {
            Ok(docs) => batch.extend(docs),
            Err(_) => failed += 1,
        }

        // Flush when the batch is large enough to keep memory bounded.
        if batch.len() >= EMBED_BATCH_SIZE {
            total_chunks += flush_batch(&ctx.cfg, &mut batch, progress_tx).await?;
        }

        if files_done.is_multiple_of(FILE_PROGRESS_EVERY) || files_done == files_total {
            send_progress(
                progress_tx,
                serde_json::json!({
                    "files_done": files_done,
                    "files_total": files_total,
                    "chunks_embedded": total_chunks,
                    "phase": "collecting_files",
                }),
            )
            .await;
        }
    }

    // Flush any remaining docs.
    if !batch.is_empty() {
        total_chunks += flush_batch(&ctx.cfg, &mut batch, progress_tx).await?;
    }

    Ok((total_chunks, failed))
}

/// Send a batch of docs to the embed pipeline and clear the buffer.
async fn flush_batch(
    cfg: &Config,
    batch: &mut Vec<PreparedDoc>,
    progress_tx: Option<&mpsc::Sender<serde_json::Value>>,
) -> Result<usize> {
    let docs = std::mem::take(batch);
    let count = docs.len();

    send_progress(
        progress_tx,
        serde_json::json!({
            "phase": "embedding_batch",
            "batch_size": count,
        }),
    )
    .await;

    let summary = embed_prepared_docs(cfg, docs, None)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(summary.chunks_embedded)
}

async fn send_progress(tx: Option<&mpsc::Sender<serde_json::Value>>, progress: serde_json::Value) {
    if let Some(tx) = tx
        && let Err(err) = tx.send(progress).await
    {
        log_warn(&format!(
            "command=ingest_github progress_send_failed err={err}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::line_range_for_chunk;
    use crate::crates::vector::ops::input::{chunk_text, code::chunk_code};

    /// Pre-chunking must produce bounded content per chunk.
    /// chunk_text uses 2000-char windows with 200-char overlap.
    #[test]
    fn chunk_text_produces_bounded_content() {
        let long = "x".repeat(5000);
        let chunks = chunk_text(&long);
        for chunk in &chunks {
            assert!(chunk.len() <= 2200, "chunk too large: {}", chunk.len());
        }
        assert!(
            chunks.len() > 1,
            "expected multiple chunks for 5000-char input"
        );
    }

    /// Empty / whitespace-only files must produce no chunks (safe no-op).
    #[test]
    fn empty_content_produces_no_panic() {
        let chunks = chunk_text("   ");
        // Just verify no panic — caller filters empty results via trim check
        let _ = chunks;
    }

    /// chunk_code falls back to chunk_text for unknown extensions.
    #[test]
    fn chunk_code_unknown_ext_falls_back() {
        let text = "hello world ".repeat(200);
        let result = chunk_code(&text, "unknownext");
        // Either None (no grammar) or Some with bounded chunks
        if let Some(chunks) = result {
            for chunk in &chunks {
                assert!(chunk.len() <= 2200, "chunk too large: {}", chunk.len());
            }
        }
        // None is also valid — caller uses chunk_text fallback
    }

    #[test]
    fn line_range_first_line() {
        let content = "hello world";
        let (start, end) = line_range_for_chunk(content, "hello world");
        assert_eq!(start, 1);
        assert_eq!(end, 1);
    }

    #[test]
    fn line_range_multi_line_content() {
        let content = "line1\nline2\nline3\nline4\nline5";
        // Chunk spanning lines 3-4
        let (start, end) = line_range_for_chunk(content, "line3\nline4");
        assert_eq!(start, 3);
        assert_eq!(end, 4);
    }

    #[test]
    fn line_range_chunk_not_found_defaults_to_start() {
        let content = "fn main() {}";
        let (start, end) = line_range_for_chunk(content, "not_in_content");
        // Falls back to byte_offset=0, so line 1
        assert_eq!(start, 1);
        assert_eq!(end, 1);
    }
}
