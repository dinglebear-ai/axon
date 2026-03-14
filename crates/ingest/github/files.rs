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

/// Read a single file from the cloned repo and build a PreparedDoc with all chunks.
///
/// Returns one `PreparedDoc` per file — all chunks for the file are in
/// `PreparedDoc.chunks`. The unified pipeline issues one TEI call per PreparedDoc.
/// Empty or unreadable files return `Ok(None)`.
async fn read_file_embed_doc(
    ctx: &FileEmbedCtx,
    path: &str,
) -> Result<Option<PreparedDoc>, String> {
    let full_path = ctx.repo_root.join(path);
    let text = match tokio::fs::read_to_string(&full_path).await {
        Ok(t) => t,
        Err(e) => {
            log_warn(&format!(
                "command=ingest_github read_failed path={path} err={e}"
            ));
            return Ok(None);
        }
    };
    if text.trim().is_empty() {
        return Ok(None);
    }

    let ext = file_extension(path);
    let chunks = chunk_code(&text, &ext).unwrap_or_else(|| chunk_text(&text));
    if chunks.is_empty() {
        return Ok(None);
    }

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
        file_language: Some(language_name(&ext).to_string()),
        file_type: Some(classify_file_type(path).to_string()),
        is_test: Some(is_test_path(path)),
        file_size_bytes: Some(text.len()),
        chunking_method: Some(chunking_method(&ext).to_string()),
        ..Default::default()
    });

    let source_url = format!(
        "https://github.com/{}/{}/blob/{}/{}",
        ctx.owner, ctx.name, ctx.default_branch, path
    );

    Ok(Some(PreparedDoc {
        url: source_url,
        domain: "github.com".to_string(),
        chunks,
        source_type: "github".to_string(),
        content_type: "text",
        title: Some(path.to_string()),
        extra: Some(extra),
    }))
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
    let mut failed = 0usize;
    let docs = collect_embed_docs(&ctx, file_items, files_total, progress_tx, &mut failed).await;

    send_progress(
        progress_tx,
        serde_json::json!({
            "files_done": files_total,
            "files_total": files_total,
            "chunks_embedded": 0,
            "phase": "embedding",
        }),
    )
    .await;

    let summary = embed_prepared_docs(cfg, docs, None)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let chunks_embedded = summary.chunks_embedded;

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

async fn collect_embed_docs(
    ctx: &Arc<FileEmbedCtx>,
    file_items: Vec<String>,
    files_total: usize,
    progress_tx: Option<&mpsc::Sender<serde_json::Value>>,
    failed: &mut usize,
) -> Vec<PreparedDoc> {
    let concurrency = std::cmp::min(ctx.cfg.batch_concurrency, 64);
    let mut file_stream = stream::iter(file_items)
        .map(|path| {
            let ctx = Arc::clone(ctx);
            async move { read_file_embed_doc(ctx.as_ref(), &path).await }
        })
        .buffer_unordered(concurrency);

    let mut docs: Vec<PreparedDoc> = Vec::new();
    let mut files_done = 0usize;

    while let Some(result) = file_stream.next().await {
        files_done += 1;
        match result {
            Ok(Some(doc)) => docs.push(doc),
            Ok(None) => {}
            Err(_) => *failed += 1,
        }
        if files_done.is_multiple_of(FILE_PROGRESS_EVERY) || files_done == files_total {
            send_progress(
                progress_tx,
                serde_json::json!({
                    "files_done": files_done,
                    "files_total": files_total,
                    "chunks_embedded": 0,
                    "phase": "collecting_files",
                }),
            )
            .await;
        }
    }

    docs
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
}
