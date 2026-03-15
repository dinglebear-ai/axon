use crate::crates::core::config::Config;
use crate::crates::core::logging::log_warn;
use crate::crates::vector::ops::{PreparedDoc, embed_prepared_docs};
use anyhow::Result;
use futures_util::stream::{self, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::FileEmbedCtx;

const FILE_PROGRESS_EVERY: usize = 25;

/// Flush accumulated PreparedDocs to the embed pipeline every N docs to bound memory.
pub(super) const EMBED_BATCH_SIZE: usize = 50;

/// Stream file reads and flush accumulated docs to the embed pipeline every
/// `EMBED_BATCH_SIZE` docs, bounding peak memory instead of buffering all files.
pub(super) async fn collect_and_embed_batched(
    ctx: &Arc<FileEmbedCtx>,
    file_items: Vec<String>,
    files_total: usize,
    progress_tx: Option<&mpsc::Sender<serde_json::Value>>,
) -> Result<(usize, usize)> {
    let concurrency = std::cmp::min(ctx.cfg.batch_concurrency, 64);
    let mut file_stream = stream::iter(file_items)
        .map(|path| {
            let ctx = Arc::clone(ctx);
            async move { super::read_file_embed_docs(ctx.as_ref(), &path).await }
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

/// Send a progress heartbeat if a channel is available.
pub(super) async fn send_progress(
    tx: Option<&mpsc::Sender<serde_json::Value>>,
    progress: serde_json::Value,
) {
    if let Some(tx) = tx
        && let Err(err) = tx.send(progress).await
    {
        log_warn(&format!(
            "command=ingest_github progress_send_failed err={err}"
        ));
    }
}
