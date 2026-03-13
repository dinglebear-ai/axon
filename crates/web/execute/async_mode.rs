//! Fire-and-forget async job dispatch for the WebSocket execution bridge.
//!
//! Handles `crawl`, `extract`, `embed`, `github`, `reddit`, and `youtube` modes by calling the services layer
//! directly to enqueue a background job, then returning immediately with the job
//! ID.  No subprocess is spawned; no polling loop is run.

use super::context::ExecCommandContext;
use super::events::{CommandContext, WsEventV2, serialize_v2_event};
use super::ws_send::{send_done_dual, send_error_dual};
use crate::crates::core::config::{Config, ConfigOverrides};
use crate::crates::services;
use crate::crates::services::ingest::IngestSource;
use serde_json::json;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

// ── Enqueue result ────────────────────────────────────────────────────────────

/// Outcome of a fire-and-forget job enqueue.
enum EnqueueResult {
    /// Multiple job IDs (crawl can enqueue one per URL).
    JobIds(Vec<String>),
    /// A single job ID (embed, extract).
    JobId(String),
}

// ── Box::pin call wrappers ────────────────────────────────────────────────────
//
// Service functions take `&Config` and `&str` references that span `.await`
// points.  The outer `handle_command` is spawned with `tokio::spawn`, so its
// future must be `Send + 'static`.
//
// Solution: wrap each service call in `Box::pin(async move {...})`.  The block
// captures `Arc<Config>` (which is `Send + 'static`) and owned `String`s by
// value; internal borrows (`&*cfg`, `url.as_str()`) are local to the block and
// type-erased by the box, eliminating lifetime parameters from the outer future.

fn call_crawl_start(
    cfg: Arc<Config>,
    urls: Vec<String>,
) -> Pin<Box<dyn Future<Output = Result<EnqueueResult, String>> + Send + 'static>> {
    Box::pin(async move {
        services::crawl::crawl_start(&cfg, &urls, None)
            .await
            .map(|r| EnqueueResult::JobIds(r.job_ids))
            .map_err(|e| e.to_string())
    })
}

fn call_extract_start(
    cfg: Arc<Config>,
    urls: Vec<String>,
) -> Pin<Box<dyn Future<Output = Result<EnqueueResult, String>> + Send + 'static>> {
    Box::pin(async move {
        services::extract::extract_start(&cfg, &urls, None)
            .await
            .map(|r| EnqueueResult::JobId(r.job_id))
            .map_err(|e| e.to_string())
    })
}

fn call_embed_start(
    cfg: Arc<Config>,
) -> Pin<Box<dyn Future<Output = Result<EnqueueResult, String>> + Send + 'static>> {
    Box::pin(async move {
        services::embed::embed_start(&cfg, None)
            .await
            .map(|r| EnqueueResult::JobId(r.job_id))
            .map_err(|e| e.to_string())
    })
}

fn call_ingest_start(
    cfg: Arc<Config>,
    source: IngestSource,
) -> Pin<Box<dyn Future<Output = Result<EnqueueResult, String>> + Send + 'static>> {
    Box::pin(async move {
        services::ingest::ingest_start(&cfg, source)
            .await
            .map(|r| EnqueueResult::JobId(r.job_id))
            .map_err(|e| e.to_string())
    })
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

async fn dispatch_async(
    context: &ExecCommandContext,
    tx: &mpsc::Sender<String>,
    ws_ctx: &CommandContext,
    crawl_job_id: Arc<Mutex<Option<String>>>,
) -> Result<(), String> {
    let mode = context.mode.as_str();
    let input = context.input.trim().to_string();

    // Apply flag overrides (collection, etc.) to produce a per-request Config.
    let mut overrides = ConfigOverrides::default();
    if let Some(col) = context
        .flags
        .get("collection")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        overrides.collection = Some(col.to_string());
    }
    let cfg = Arc::new(context.cfg.apply_overrides(&overrides));

    let result = match mode {
        "crawl" => {
            if input.is_empty() {
                return Err("crawl requires a non-empty URL input".to_string());
            }
            let urls: Vec<String> = input.split_whitespace().map(str::to_string).collect();
            call_crawl_start(cfg, urls).await?
        }
        "extract" => {
            if input.is_empty() {
                return Err("extract requires a non-empty URL input".to_string());
            }
            let urls: Vec<String> = input.split_whitespace().map(str::to_string).collect();
            call_extract_start(cfg, urls).await?
        }
        "embed" => call_embed_start(cfg).await?,
        "github" => {
            if input.is_empty() {
                return Err("github requires a non-empty repo input".to_string());
            }
            let include_source = context
                .flags
                .get("include_source")
                .and_then(|value| value.as_bool())
                .unwrap_or(cfg.github_include_source);
            call_ingest_start(
                cfg,
                IngestSource::Github {
                    repo: input,
                    include_source,
                },
            )
            .await?
        }
        "reddit" => {
            if input.is_empty() {
                return Err("reddit requires a non-empty target input".to_string());
            }
            call_ingest_start(cfg, IngestSource::Reddit { target: input }).await?
        }
        "youtube" => {
            if input.is_empty() {
                return Err("youtube requires a non-empty target input".to_string());
            }
            call_ingest_start(cfg, IngestSource::Youtube { target: input }).await?
        }
        _ => return Err(format!("unknown async mode: {mode}")),
    };

    // Emit the job ID to the browser and store it for cancel support.
    //
    // NOTE (H-5): As of 2026-03, async job progress is POLL-ONLY.
    // Job lifecycle updates (pending → running → completed/failed) are
    // observed by clients polling the HTTP /api/jobs/:id endpoint, not via
    // WebSocket push events.
    match result {
        EnqueueResult::JobIds(ids) => {
            let first = ids.first().cloned();
            let payload = json!({
                "job_id": first.as_deref().unwrap_or(""),
                "job_ids": ids,
                "mode": mode,
                "enqueued": true,
            });
            emit_output_json(tx, ws_ctx, payload).await;

            if let Some(id) = first {
                *crawl_job_id.lock().await = Some(id.clone());
                emit_log(tx, &format!("[web] {mode} job enqueued: {id}")).await;
            }
        }
        EnqueueResult::JobId(id) => {
            let payload = json!({
                "job_id": id,
                "mode": mode,
                "enqueued": true,
            });
            emit_output_json(tx, ws_ctx, payload).await;
            *crawl_job_id.lock().await = Some(id.clone());
            emit_log(tx, &format!("[web] {mode} job enqueued: {id}")).await;
        }
    }

    Ok(())
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Enqueue an async job via direct service dispatch and return immediately.
///
/// Calls the appropriate service function, emits a `CommandOutputJson` event
/// with the job ID, and completes without polling.
pub(super) async fn handle_async_command(
    context: ExecCommandContext,
    tx: mpsc::Sender<String>,
    crawl_job_id: Arc<Mutex<Option<String>>>,
) {
    let ws_ctx = context.to_ws_ctx();
    let start = std::time::Instant::now();

    match dispatch_async(&context, &tx, &ws_ctx, crawl_job_id).await {
        Ok(()) => {
            let elapsed = start.elapsed().as_millis() as u64;
            send_done_dual(&tx, &ws_ctx, 0, Some(elapsed)).await;
        }
        Err(msg) => {
            let elapsed = start.elapsed().as_millis() as u64;
            send_error_dual(&tx, &ws_ctx, msg, Some(elapsed)).await;
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn emit_output_json(
    tx: &mpsc::Sender<String>,
    ctx: &CommandContext,
    data: serde_json::Value,
) {
    if let Some(v2) = serialize_v2_event(WsEventV2::CommandOutputJson {
        ctx: ctx.clone(),
        data,
    }) {
        let _ = tx.send(v2).await;
    }
}

async fn emit_log(tx: &mpsc::Sender<String>, line: &str) {
    let _ = tx
        .send(json!({"type": "log", "line": line}).to_string())
        .await;
}
