use serde_json::json;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::events::{ArtifactEntry, CommandContext, WsEventV2, serialize_v2_event};

async fn send_artifact_content_dual(
    tx: &mpsc::Sender<String>,
    ctx: &CommandContext,
    path: String,
    content: String,
) {
    if let Some(v2) = serialize_v2_event(WsEventV2::ArtifactContent {
        ctx: ctx.clone(),
        path,
        content,
    }) {
        let _ = tx.send(v2).await;
    }
}

async fn send_artifact_list_v2(
    tx: &mpsc::Sender<String>,
    ctx: &CommandContext,
    artifacts: Vec<ArtifactEntry>,
) {
    if let Some(v2) = serialize_v2_event(WsEventV2::ArtifactList {
        ctx: ctx.clone(),
        artifacts,
    }) {
        let _ = tx.send(v2).await;
    }
}

/// Resolve the output directory for reading crawl results.
/// Priority: `AXON_OUTPUT_DIR` → `$AXON_DATA_DIR/axon/output` → `.cache/axon-rust/output`.
pub fn output_dir() -> PathBuf {
    std::env::var("AXON_OUTPUT_DIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| crate::crates::core::paths::axon_data_dir().map(|d| d.join("axon/output")))
        .unwrap_or_else(|| PathBuf::from(".cache/axon-rust/output"))
}

/// Find the most recently modified `.md` file in a directory.
async fn newest_md_file(dir: &Path) -> Option<PathBuf> {
    let mut entries = tokio::fs::read_dir(dir).await.ok()?;
    let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "md")
            && let Ok(meta) = entry.metadata().await
            && let Ok(modified) = meta.modified()
            && newest.as_ref().is_none_or(|(_, t)| modified > *t)
        {
            newest = Some((path, modified));
        }
    }
    newest.map(|(p, _)| p)
}

/// Send a single scraped markdown file to the frontend.
///
/// Takes `tx` and `ctx` by owned value so callers can pass them without
/// creating borrows that cross `.await` points in the async state machine.
/// Both types are cheap to clone — `mpsc::Sender` is a reference-counted
/// handle and `CommandContext` contains three short `String` fields.
pub(super) async fn send_scrape_file(tx: mpsc::Sender<String>, ctx: CommandContext) {
    let md_dir = output_dir().join("scrape-markdown");
    match newest_md_file(&md_dir).await {
        Some(path) => match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                send_artifact_content_dual(&tx, &ctx, path.to_string_lossy().into_owned(), content)
                    .await;
            }
            Err(e) => {
                let _ = tx
                    .send(
                        json!({"type": "log", "line": format!("[web] read error: {e}")})
                            .to_string(),
                    )
                    .await;
            }
        },
        None => {
            let _ = tx
                .send(
                    json!({"type": "log", "line": format!("[web] no .md files found in {}", md_dir.display())})
                        .to_string(),
                )
                .await;
        }
    }
}

/// Build a `screenshot_files` message from captured stdout JSON objects.
///
/// Each screenshot JSON has shape `{url, path, size_bytes}`. We extract the
/// filename from `path` and construct a serve URL so the frontend can display
/// the image inline. This is deterministic — no filesystem timestamp scan.
pub(super) async fn send_screenshot_files_from_json(
    jsons: &[serde_json::Value],
    tx: &mpsc::Sender<String>,
    ctx: &CommandContext,
) {
    let mut artifacts = Vec::new();
    for obj in jsons {
        let path_str = obj.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let size_bytes = obj.get("size_bytes").and_then(|v| v.as_u64()).unwrap_or(0);
        if path_str.is_empty() {
            continue;
        }
        let name = Path::new(path_str)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let serve_url = format!("/output/screenshots/{name}");
        artifacts.push(ArtifactEntry {
            kind: Some("screenshot".to_string()),
            path: Some(path_str.to_string()),
            download_url: Some(serve_url),
            mime: Some("image/png".to_string()),
            size_bytes: Some(size_bytes),
        });
    }
    if !artifacts.is_empty() {
        send_artifact_list_v2(tx, ctx, artifacts).await;
    }
}

/// Send the crawl manifest file list to the frontend from a job output directory.
/// When `job_id` is provided, it is included in the `crawl_files` message for download routes.
// Retained for future use when crawl status polling is wired to the manifest.
// Previously called from polling.rs (now removed). Will be reactivated when
// the crawl status command returns manifest info via direct service dispatch.
#[allow(dead_code)]
pub(super) async fn send_crawl_manifest(
    job_dir: &Path,
    tx: &mpsc::Sender<String>,
    job_id: Option<&str>,
    ctx: &CommandContext,
) {
    let manifest = job_dir.join("manifest.jsonl");

    // If the job-specific dir doesn't have a manifest yet, try `latest/`
    // (the reflink is updated after crawl completes, before marking done)
    let manifest = if tokio::fs::metadata(&manifest).await.is_ok() {
        manifest
    } else if let Some(parent) = job_dir.parent() {
        let latest = parent.join("latest").join("manifest.jsonl");
        if tokio::fs::metadata(&latest).await.is_ok() {
            latest
        } else {
            let _ = tx
                .send(
                    json!({"type": "log", "line": format!("[web] no manifest at {}", manifest.display())})
                        .to_string(),
                )
                .await;
            return;
        }
    } else {
        let _ = tx
            .send(json!({"type": "log", "line": "[web] no crawl manifest found"}).to_string())
            .await;
        return;
    };

    let base_dir = manifest
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    match tokio::fs::read_to_string(&manifest).await {
        Ok(raw) => {
            let mut files = Vec::new();
            let mut artifacts = Vec::new();
            for line in raw.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                let url = entry.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let rel = entry
                    .get("relative_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let chars = entry
                    .get("markdown_chars")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                if !rel.is_empty() {
                    let path = base_dir.join(rel).to_string_lossy().into_owned();
                    files.push(json!({
                        "url": url,
                        "relative_path": rel,
                        "markdown_chars": chars,
                    }));
                    artifacts.push(ArtifactEntry {
                        kind: Some("markdown".to_string()),
                        path: Some(path),
                        download_url: job_id.map(|id| format!("/download/{id}/{rel}")),
                        mime: Some("text/markdown".to_string()),
                        size_bytes: None,
                    });
                }
            }

            let mut msg = json!({
                "type": "crawl_files",
                "files": files,
                "output_dir": base_dir.to_string_lossy(),
            });
            if let Some(id) = job_id {
                msg["job_id"] = serde_json::Value::String(id.to_string());
            }
            let _ = tx.send(msg.to_string()).await;
            send_artifact_list_v2(tx, ctx, artifacts).await;

            // Auto-load the first file
            if let Some(first) = files.first()
                && let Some(rel) = first.get("relative_path").and_then(|v| v.as_str())
            {
                let full = base_dir.join(rel);
                if let Ok(content) = tokio::fs::read_to_string(&full).await {
                    send_artifact_content_dual(
                        tx,
                        ctx,
                        full.to_string_lossy().into_owned(),
                        content,
                    )
                    .await;
                }
            }
        }
        Err(e) => {
            let _ = tx
                .send(
                    json!({"type": "log", "line": format!("[web] manifest read error: {e}")})
                        .to_string(),
                )
                .await;
        }
    }
}

/// Read a file on demand from a crawl output directory.
/// Validates the path is within the base directory to prevent traversal attacks.
pub(crate) async fn handle_read_file(
    relative_path: &str,
    base_dir: &Path,
    tx: mpsc::Sender<String>,
) {
    let ctx = CommandContext {
        exec_id: format!("exec-{}", Uuid::new_v4()),
        mode: "read_file".to_string(),
        input: relative_path.to_string(),
    };
    let full_path = base_dir.join(relative_path);
    let Ok(canonical_base) = tokio::fs::canonicalize(base_dir).await else {
        let _ = tx
            .send(json!({"type": "error", "message": "invalid base directory"}).to_string())
            .await;
        return;
    };
    let Ok(canonical_path) = tokio::fs::canonicalize(&full_path).await else {
        let _ = tx
            .send(json!({"type": "error", "message": "file not found"}).to_string())
            .await;
        return;
    };

    if !canonical_path.starts_with(&canonical_base) {
        let _ = tx
            .send(json!({"type": "error", "message": "path outside output directory"}).to_string())
            .await;
        return;
    }

    match tokio::fs::read_to_string(&canonical_path).await {
        Ok(content) => {
            send_artifact_content_dual(
                &tx,
                &ctx,
                canonical_path.to_string_lossy().into_owned(),
                content,
            )
            .await;
        }
        Err(e) => {
            let _ = tx
                .send(json!({"type": "error", "message": format!("read error: {e}")}).to_string())
                .await;
        }
    }
}
