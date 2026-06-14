use super::{EmbedProgress, EmbedSummary, PreparedDoc, qdrant_store::VectorMode};
use crate::core::config::Config;
use crate::core::logging::{log_info, log_warn};
use crate::vector::ops::qdrant::env_usize_clamped;
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::time::Instant;

mod bootstrap;
mod payload;

use bootstrap::{PostUpsertCleanup, bootstrap_first_doc, flush_and_cleanup};
use payload::{DocFuture, EmbeddedDoc, SendError, embed_prepared_doc_with_timeout};

// Re-exported so the pipeline test sidecar can reach these private helpers via
// `super::apply_extra`, `super::drop_blank_chunks_aligned`, and the full
// `crate::vector::ops::tei::pipeline::{apply_extra, RESERVED_PAYLOAD_KEYS}` paths.
// Test-only: these helpers have no consumers outside the pipeline module + its
// sidecar, so the re-export is gated on `cfg(test)` to avoid a dead-export warning.
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use payload::{RESERVED_PAYLOAD_KEYS, apply_extra, drop_blank_chunks_aligned};

/// Immutable pipeline configuration resolved once at startup.
struct PipelineParams {
    doc_concurrency: usize,
    doc_timeout_secs: u64,
    mode: VectorMode,
    flush_point_threshold: usize,
    docs_total: usize,
}

/// Mutable pipeline state accumulated across the concurrent drain loop.
struct PipelineState {
    docs_completed: usize,
    pending_points: Vec<serde_json::Value>,
    cleanup_queue: Vec<PostUpsertCleanup>,
}

/// Drive the concurrent doc-processing loop (Phase 2) after mode is known.
///
/// Drains remaining docs from `work`, processing up to `doc_concurrency` in
/// parallel. Returns `(chunks_embedded, docs_completed, docs_failed)`.
async fn drain_concurrent_docs<'a>(
    cfg: &'a Config,
    work: &mut impl Iterator<Item = PreparedDoc>,
    params: &PipelineParams,
    state: &mut PipelineState,
    progress_tx: &Option<tokio::sync::mpsc::Sender<EmbedProgress>>,
) -> Result<(usize, usize, usize), SendError> {
    let mut inflight: FuturesUnordered<DocFuture<'a>> = FuturesUnordered::new();
    let mut chunks_embedded = 0usize;
    let mut docs_failed = 0usize;

    for _ in 0..params.doc_concurrency {
        if let Some(doc) = work.next() {
            inflight.push(Box::pin(embed_prepared_doc_with_timeout(
                cfg,
                doc,
                params.doc_timeout_secs,
                params.mode,
            )));
        }
    }

    while let Some(result) = inflight.next().await {
        match result {
            Ok(EmbeddedDoc {
                url,
                chunk_count,
                mut points,
                local_legacy_fragment_url,
                ..
            }) => {
                chunks_embedded += points.len();
                state.pending_points.append(&mut points);
                state.cleanup_queue.push(PostUpsertCleanup::StaleTail {
                    url,
                    new_chunk_count: chunk_count,
                });
                if let Some(url) = local_legacy_fragment_url {
                    state
                        .cleanup_queue
                        .push(PostUpsertCleanup::LocalLegacyFragments { file_url: url });
                }
                if state.pending_points.len() >= params.flush_point_threshold {
                    flush_and_cleanup(cfg, &mut state.pending_points, &mut state.cleanup_queue)
                        .await?;
                }
            }
            Err(e) => {
                docs_failed += 1;
                log_warn(&format!("embed_pipeline doc_failed: {e}"));
            }
        }
        state.docs_completed += 1;
        if let Some(tx) = progress_tx {
            // If the receiver has been dropped, ignore the error and continue — embed
            // results are what matter; progress reporting is best-effort. (B-L5)
            let _ = tx
                .send(EmbedProgress {
                    docs_total: params.docs_total,
                    docs_completed: state.docs_completed,
                    chunks_embedded,
                })
                .await;
        }
        if let Some(doc) = work.next() {
            inflight.push(Box::pin(embed_prepared_doc_with_timeout(
                cfg,
                doc,
                params.doc_timeout_secs,
                params.mode,
            )));
        }
    }
    Ok((chunks_embedded, state.docs_completed, docs_failed))
}

pub(super) async fn run_embed_pipeline(
    cfg: &Config,
    prepared: Vec<PreparedDoc>,
    progress_tx: Option<tokio::sync::mpsc::Sender<EmbedProgress>>,
) -> Result<EmbedSummary, SendError> {
    let docs_total = prepared.len();
    let pipeline_start = Instant::now();
    log_info(&format!("embed_pipeline docs={}", docs_total));
    // Sourced from Config (env > TOML > default), already clamped 30..=3600.
    let doc_timeout_secs = cfg.embed_doc_timeout_secs;
    let doc_concurrency = env_usize_clamped(
        "AXON_EMBED_DOC_CONCURRENCY",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8)
            .clamp(2, 8),
        1,
        64,
    );
    let flush_threshold = cfg.qdrant_point_buffer.clamp(128, 16_384);

    let mut work = prepared.into_iter();
    if let Some(tx) = &progress_tx {
        let _ = tx
            .send(EmbedProgress {
                docs_total,
                docs_completed: 0,
                chunks_embedded: 0,
            })
            .await;
    }

    let mut state = PipelineState {
        docs_completed: 0,
        pending_points: Vec::new(),
        cleanup_queue: Vec::new(),
    };

    // Phase 1: process first doc serially to learn dim + VectorMode.
    // Named collections require named vectors with BM42 sparse; Unnamed expects flat arrays.
    let Some(first_doc) = work.next() else {
        return Ok(EmbedSummary {
            docs_embedded: 0,
            docs_failed: 0,
            chunks_embedded: 0,
        });
    };
    let (mode, mut chunks_embedded, mut docs_failed) = bootstrap_first_doc(
        cfg,
        first_doc,
        doc_timeout_secs,
        &mut state.pending_points,
        &mut state.cleanup_queue,
    )
    .await?;
    state.docs_completed = 1;
    if let Some(tx) = &progress_tx {
        let _ = tx
            .send(EmbedProgress {
                docs_total,
                docs_completed: state.docs_completed,
                chunks_embedded,
            })
            .await;
    }

    let params = PipelineParams {
        doc_concurrency,
        doc_timeout_secs,
        mode,
        flush_point_threshold: flush_threshold,
        docs_total,
    };

    // Phase 2: process remaining docs concurrently with the known mode.
    let (phase2_chunks, _phase2_completed, phase2_failed) =
        drain_concurrent_docs(cfg, &mut work, &params, &mut state, &progress_tx).await?;
    chunks_embedded += phase2_chunks;
    docs_failed += phase2_failed;

    flush_and_cleanup(cfg, &mut state.pending_points, &mut state.cleanup_queue).await?;

    let elapsed_secs = pipeline_start.elapsed().as_secs();
    let docs_embedded = docs_total - docs_failed;
    if docs_failed > 0 {
        log_warn(&format!(
            "embed_pipeline completed with {docs_failed}/{docs_total} doc failures"
        ));
    }
    log_info(&format!(
        "embed_pipeline_done docs={docs_total} embedded={docs_embedded} failed={docs_failed} chunks={chunks_embedded} elapsed={elapsed_secs}s"
    ));
    Ok(EmbedSummary {
        docs_embedded,
        docs_failed,
        chunks_embedded,
    })
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
