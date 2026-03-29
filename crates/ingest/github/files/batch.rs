use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_info, log_warn};
use crate::crates::ingest::progress::PhaseReporter;
use crate::crates::vector::ops::{PreparedDoc, embed_prepared_docs};
use anyhow::Result;
use futures_util::stream::{self, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;

use super::FileEmbedCtx;

const FILE_PROGRESS_EVERY: usize = 25;
const PHASE_COLLECTING_FILES: &str = "collecting_files";
const PHASE_EMBEDDING_BATCH: &str = "embedding_batch";
const FLUSH_BATCH_TIMEOUT_SECS: u64 = 120;

/// Flush accumulated PreparedDocs to the embed pipeline every N docs to bound memory.
pub(super) const EMBED_BATCH_SIZE: usize = 50;

/// Stream file reads and flush accumulated docs to the embed pipeline every
/// `EMBED_BATCH_SIZE` docs, bounding peak memory instead of buffering all files.
pub(super) async fn collect_and_embed_batched(
    ctx: &Arc<FileEmbedCtx>,
    file_items: Vec<String>,
    files_total: usize,
    reporter: &PhaseReporter,
) -> Result<(usize, usize)> {
    let concurrency = std::cmp::min(ctx.cfg.batch_concurrency, 16);
    let mut file_stream = stream::iter(file_items)
        .map(|path| {
            let ctx = Arc::clone(ctx);
            async move { super::read_file_embed_docs(ctx.as_ref(), &path).await }
        })
        .buffer_unordered(concurrency);

    log_info(&format!(
        "github collect_start files_total={files_total} batch_concurrency={concurrency} embed_batch_size={EMBED_BATCH_SIZE}"
    ));

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
            match flush_batch(&ctx.cfg, &mut batch, reporter).await {
                Ok(n) => total_chunks += n,
                Err(e) => log_warn(&format!(
                    "github flush_batch_error files_done={files_done} err={e} — discarding batch, continuing"
                )),
            }
        }

        if files_done.is_multiple_of(FILE_PROGRESS_EVERY) || files_done == files_total {
            log_info(&format!(
                "github files_progress files_done={files_done} files_total={files_total} chunks_embedded={total_chunks}"
            ));
            reporter
                .report(serde_json::json!({
                    "files_done": files_done,
                    "files_total": files_total,
                    "chunks_embedded": total_chunks,
                    "phase": PHASE_COLLECTING_FILES,
                }))
                .await;
        }
    }

    // Flush any remaining docs.
    if !batch.is_empty() {
        match flush_batch(&ctx.cfg, &mut batch, reporter).await {
            Ok(n) => total_chunks += n,
            Err(e) => log_warn(&format!(
                "github flush_batch_final_error files_done={files_done} err={e} — discarding final batch"
            )),
        }
    }

    Ok((total_chunks, failed))
}

/// Send a batch of docs to the embed pipeline and clear the buffer.
async fn flush_batch(
    cfg: &Config,
    batch: &mut Vec<PreparedDoc>,
    reporter: &PhaseReporter,
) -> Result<usize> {
    let docs = std::mem::take(batch);
    let count = docs.len();

    log_info(&format!("github embed_batch_start batch_size={count}"));
    reporter
        .report(serde_json::json!({
            "phase": PHASE_EMBEDDING_BATCH,
            "batch_size": count,
        }))
        .await;

    let batch_start = Instant::now();
    let summary = timeout(
        Duration::from_secs(FLUSH_BATCH_TIMEOUT_SECS),
        embed_prepared_docs(cfg, docs, None),
    )
    .await
    .map_err(|_| anyhow::anyhow!("flush_batch timed out after {FLUSH_BATCH_TIMEOUT_SECS}s"))?
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    let elapsed_ms = batch_start.elapsed().as_millis();
    log_info(&format!(
        "github embed_batch_done batch_size={count} chunks={} elapsed_ms={elapsed_ms}",
        summary.chunks_embedded
    ));
    Ok(summary.chunks_embedded)
}
