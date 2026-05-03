use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_info, log_warn};
use crate::crates::ingest::progress::PhaseReporter;
use crate::crates::vector::ops::{PreparedDoc, embed_prepared_docs};
use anyhow::Result;
use futures_util::stream::{self, StreamExt};
use std::collections::HashSet;
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

#[derive(Debug, Default, Clone)]
pub(super) struct GitHubFileEmbedStats {
    pub chunks_embedded: usize,
    pub failed_file_reads: usize,
    pub failed_batches: usize,
    pub failed_files: usize,
    pub failed_docs: usize,
    pub failed_chunks: usize,
}

impl GitHubFileEmbedStats {
    fn record_failed_file_read(&mut self) {
        self.failed_file_reads += 1;
    }

    fn record_failed_batch(&mut self, files: usize, docs: usize, chunks: usize) {
        self.failed_batches += 1;
        self.failed_files += files;
        self.failed_docs += docs;
        self.failed_chunks += chunks;
    }

    pub(super) fn has_failed_batches(&self) -> bool {
        self.failed_batches > 0
    }
}

/// Stream file reads and flush accumulated docs to the embed pipeline every
/// `EMBED_BATCH_SIZE` docs, bounding peak memory instead of buffering all files.
pub(super) async fn collect_and_embed_batched(
    ctx: &Arc<FileEmbedCtx>,
    file_items: Vec<String>,
    files_total: usize,
    reporter: &PhaseReporter,
) -> Result<GitHubFileEmbedStats> {
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
    let mut stats = GitHubFileEmbedStats::default();

    while let Some(result) = file_stream.next().await {
        files_done += 1;
        match result {
            Ok(docs) => batch.extend(docs),
            Err(_) => stats.record_failed_file_read(),
        }

        // Flush when the batch is large enough to keep memory bounded.
        if batch.len() >= EMBED_BATCH_SIZE {
            let batch_files = unique_file_count(&batch);
            let batch_docs = batch.len();
            let batch_chunks = batch.len();
            match flush_batch(&ctx.cfg, &mut batch, reporter).await {
                Ok(n) => stats.chunks_embedded += n,
                Err(e) => {
                    stats.record_failed_batch(batch_files, batch_docs, batch_chunks);
                    log_warn(&format!(
                        "github flush_batch_error files_done={files_done} failed_files={batch_files} failed_docs={batch_docs} failed_chunks={batch_chunks} err={e}"
                    ));
                    batch.clear();
                }
            }
        }

        if files_done.is_multiple_of(FILE_PROGRESS_EVERY) || files_done == files_total {
            log_info(&format!(
                "github files_progress files_done={files_done} files_total={files_total} chunks_embedded={} batches_failed={} files_failed={} chunks_failed={}",
                stats.chunks_embedded,
                stats.failed_batches,
                stats.failed_files,
                stats.failed_chunks
            ));
            reporter
                .report(serde_json::json!({
                    "files_done": files_done,
                    "files_total": files_total,
                    "chunks_embedded": stats.chunks_embedded,
                    "file_read_failures": stats.failed_file_reads,
                    "embed_batches_failed": stats.failed_batches,
                    "embed_files_failed": stats.failed_files,
                    "embed_docs_failed": stats.failed_docs,
                    "embed_chunks_failed": stats.failed_chunks,
                    "phase": PHASE_COLLECTING_FILES,
                }))
                .await;
        }
    }

    // Flush any remaining docs.
    if !batch.is_empty() {
        let batch_files = unique_file_count(&batch);
        let batch_docs = batch.len();
        let batch_chunks = batch.len();
        match flush_batch(&ctx.cfg, &mut batch, reporter).await {
            Ok(n) => stats.chunks_embedded += n,
            Err(e) => {
                stats.record_failed_batch(batch_files, batch_docs, batch_chunks);
                log_warn(&format!(
                    "github flush_batch_final_error files_done={files_done} failed_files={batch_files} failed_docs={batch_docs} failed_chunks={batch_chunks} err={e}"
                ));
                batch.clear();
            }
        }
    }

    Ok(stats)
}

fn unique_file_count(docs: &[PreparedDoc]) -> usize {
    docs.iter()
        .filter_map(|doc| doc.title.as_deref())
        .collect::<HashSet<_>>()
        .len()
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

#[cfg(test)]
mod tests {
    use super::GitHubFileEmbedStats;

    #[test]
    fn failed_batch_accounting_counts_batches_docs_and_chunks() {
        let mut stats = GitHubFileEmbedStats::default();

        stats.record_failed_batch(2, 3, 7);
        stats.record_failed_batch(1, 2, 5);

        assert_eq!(stats.failed_batches, 2);
        assert_eq!(stats.failed_files, 3);
        assert_eq!(stats.failed_docs, 5);
        assert_eq!(stats.failed_chunks, 12);
        assert!(stats.has_failed_batches());
    }
}
