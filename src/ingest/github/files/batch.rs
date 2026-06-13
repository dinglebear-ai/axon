use crate::core::config::Config;
use crate::core::logging::{log_info, log_warn};
use crate::ingest::progress::PhaseReporter;
use crate::vector::ops::qdrant::qdrant_delete_stale_repo_file_urls;
use crate::vector::ops::{EmbedSummary, PreparedDoc, embed_prepared_docs};
use anyhow::Result;
use futures_util::stream::{self, StreamExt};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;

use super::prepare::{FileEmbedCtx, FileEmbedRead, read_file_embed_docs};

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
    pub cleanup_blocking_skips: usize,
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

    fn record_cleanup_blocking_skip(&mut self) {
        self.cleanup_blocking_skips += 1;
    }

    /// Any condition that must skip stale cleanup: a skipped file's chunks are
    /// not in `embedded_urls`, so cleaning up would wrongly delete its prior
    /// chunks. Read skips and embed-batch failures both qualify.
    pub(super) fn has_failures(&self) -> bool {
        self.failed_file_reads > 0 || self.failed_batches > 0 || self.cleanup_blocking_skips > 0
    }

    /// Only genuine embed-pipeline failures should abort the whole ingest. A
    /// single unreadable file is logged and skipped (it still blocks cleanup via
    /// [`has_failures`]) rather than failing the run.
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
    include_source: bool,
    reporter: &PhaseReporter,
) -> Result<GitHubFileEmbedStats> {
    let concurrency = std::cmp::min(ctx.cfg.batch_concurrency, 16);
    let mut file_stream = stream::iter(file_items)
        .map(|path| {
            let ctx = Arc::clone(ctx);
            async move { read_file_embed_docs(ctx.as_ref(), &path).await }
        })
        .buffer_unordered(concurrency);

    log_info(&format!(
        "github collect_start files_total={files_total} batch_concurrency={concurrency} embed_batch_size={EMBED_BATCH_SIZE}"
    ));

    let mut batch: Vec<PreparedDoc> = Vec::with_capacity(EMBED_BATCH_SIZE);
    let mut embedded_urls = HashSet::new();
    let mut files_done = 0usize;
    let mut stats = GitHubFileEmbedStats::default();

    while let Some(result) = file_stream.next().await {
        files_done += 1;
        match result {
            Ok(FileEmbedRead::Prepared(docs)) => batch.extend(docs),
            Ok(FileEmbedRead::Empty) => {}
            Ok(FileEmbedRead::SkippedCleanupBlocking) => stats.record_cleanup_blocking_skip(),
            Err(_) => stats.record_failed_file_read(),
        }

        // Flush when the batch is large enough to keep memory bounded.
        if batch.len() >= EMBED_BATCH_SIZE {
            let batch_files = unique_file_count(&batch);
            let batch_docs = batch.len();
            let batch_chunks = chunk_count(&batch);
            let batch_urls = urls_for_docs(&batch);
            match flush_batch(&ctx.cfg, &mut batch, reporter).await {
                Ok(summary) if summary.docs_failed == 0 => {
                    stats.chunks_embedded += summary.chunks_embedded;
                    embedded_urls.extend(batch_urls);
                }
                Ok(summary) => {
                    stats.chunks_embedded += summary.chunks_embedded;
                    stats.record_failed_batch(batch_files, batch_docs, batch_chunks);
                    log_warn(&format!(
                        "github flush_batch_partial_failure files_done={files_done} failed_files={batch_files} failed_docs={batch_docs} failed_chunks={batch_chunks} docs_failed={} chunks_embedded={}",
                        summary.docs_failed, summary.chunks_embedded
                    ));
                }
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
                    "cleanup_blocking_skips": stats.cleanup_blocking_skips,
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
        let batch_chunks = chunk_count(&batch);
        let batch_urls = urls_for_docs(&batch);
        match flush_batch(&ctx.cfg, &mut batch, reporter).await {
            Ok(summary) if summary.docs_failed == 0 => {
                stats.chunks_embedded += summary.chunks_embedded;
                embedded_urls.extend(batch_urls);
            }
            Ok(summary) => {
                stats.chunks_embedded += summary.chunks_embedded;
                stats.record_failed_batch(batch_files, batch_docs, batch_chunks);
                log_warn(&format!(
                    "github flush_batch_final_partial_failure files_done={files_done} failed_files={batch_files} failed_docs={batch_docs} failed_chunks={batch_chunks} docs_failed={} chunks_embedded={}",
                    summary.docs_failed, summary.chunks_embedded
                ));
            }
            Err(e) => {
                stats.record_failed_batch(batch_files, batch_docs, batch_chunks);
                log_warn(&format!(
                    "github flush_batch_final_error files_done={files_done} failed_files={batch_files} failed_docs={batch_docs} failed_chunks={batch_chunks} err={e}"
                ));
                batch.clear();
            }
        }
    }

    cleanup_stale_repo_file_urls(ctx, &stats, include_source, &embedded_urls).await?;

    Ok(stats)
}

async fn cleanup_stale_repo_file_urls(
    ctx: &FileEmbedCtx,
    stats: &GitHubFileEmbedStats,
    include_source: bool,
    embedded_urls: &HashSet<String>,
) -> Result<()> {
    if stats.has_failures() {
        log_warn(&format!(
            "github repo_file_stale_cleanup_skipped owner={} repo={} reason=prior_failures read_failed={} cleanup_blocking_skips={} batches_failed={}",
            ctx.owner,
            ctx.name,
            stats.failed_file_reads,
            stats.cleanup_blocking_skips,
            stats.failed_batches
        ));
        return Ok(());
    }
    if !include_source {
        log_info(&format!(
            "github repo_file_stale_cleanup_skipped owner={} repo={} reason=partial_no_source",
            ctx.owner, ctx.name
        ));
        return Ok(());
    }
    log_info(&format!(
        "github repo_file_stale_cleanup_start owner={} repo={} current_urls={}",
        ctx.owner,
        ctx.name,
        embedded_urls.len()
    ));
    let deleted = qdrant_delete_stale_repo_file_urls(
        &ctx.cfg,
        "github",
        &ctx.owner,
        &ctx.name,
        embedded_urls,
    )
    .await?;
    log_info(&format!(
        "github repo_file_stale_cleanup_done owner={} repo={} stale_urls_deleted={deleted}",
        ctx.owner, ctx.name
    ));
    Ok(())
}

fn unique_file_count(docs: &[PreparedDoc]) -> usize {
    docs.iter()
        .filter_map(|doc| doc.title())
        .collect::<HashSet<_>>()
        .len()
}

fn chunk_count(docs: &[PreparedDoc]) -> usize {
    docs.iter().map(|doc| doc.chunks().len()).sum()
}

fn urls_for_docs(docs: &[PreparedDoc]) -> HashSet<String> {
    docs.iter().map(|doc| doc.url().to_string()).collect()
}

/// Send a batch of docs to the embed pipeline and clear the buffer.
async fn flush_batch(
    cfg: &Config,
    batch: &mut Vec<PreparedDoc>,
    reporter: &PhaseReporter,
) -> Result<EmbedSummary> {
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
        "github embed_batch_done batch_size={count} chunks={} docs_failed={} elapsed_ms={elapsed_ms}",
        summary.chunks_embedded, summary.docs_failed
    ));
    summary
        .require_success("github file batch embed")
        .map_err(|e| anyhow::anyhow!("{e}"))
}

#[cfg(test)]
#[path = "batch_tests.rs"]
mod tests;
