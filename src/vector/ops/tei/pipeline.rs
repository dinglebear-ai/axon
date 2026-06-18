use super::{EmbedProgress, EmbedSummary, PreparedDoc, qdrant_store::VectorMode};
use crate::core::config::Config;
use crate::core::logging::{log_info, log_warn};
use crate::vector::ops::qdrant::env_usize_clamped;
use std::time::Instant;

mod bootstrap;
mod payload;

use super::tei_client::{EmbedKind, is_openai_compatible_embedding_url, tei_embed_kind};
use bootstrap::{
    PostUpsertCleanup, bootstrap_first_doc, flush_and_cleanup,
    restore_indexing_threshold_after_load,
};
use payload::{
    DocEmbeddingPlan, EmbeddedDoc, SendError, build_embedded_doc_from_vectors,
    prepare_doc_for_embedding,
};

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
    mode: VectorMode,
    flush_point_threshold: usize,
    pool_input_limit: usize,
    docs_total: usize,
}

/// Mutable pipeline state accumulated across the concurrent drain loop.
struct PipelineState {
    docs_completed: usize,
    pending_points: Vec<serde_json::Value>,
    cleanup_queue: Vec<PostUpsertCleanup>,
    skip_stale_tail_cleanup: bool,
}

fn push_embedded_doc(embedded: EmbeddedDoc, state: &mut PipelineState) -> usize {
    let EmbeddedDoc {
        url,
        chunk_count,
        mut points,
        local_legacy_fragment_url,
        ..
    } = embedded;
    let point_count = points.len();
    state.pending_points.append(&mut points);
    if !state.skip_stale_tail_cleanup {
        state.cleanup_queue.push(PostUpsertCleanup::StaleTail {
            url,
            new_chunk_count: chunk_count,
        });
        if let Some(url) = local_legacy_fragment_url {
            state
                .cleanup_queue
                .push(PostUpsertCleanup::LocalLegacyFragments { file_url: url });
        }
    }
    point_count
}

async fn drain_pooled_docs(
    cfg: &Config,
    docs: &mut impl Iterator<Item = PreparedDoc>,
    params: &PipelineParams,
    state: &mut PipelineState,
    progress_tx: &Option<tokio::sync::mpsc::Sender<EmbedProgress>>,
) -> Result<(usize, usize, usize), SendError> {
    let mut docs_failed = 0usize;
    let mut chunks_embedded = 0usize;
    let mut group_plans = Vec::new();
    let mut all_texts = Vec::new();
    let mut ranges = Vec::new();

    for doc in docs {
        match prepare_doc_for_embedding(doc) {
            Ok(plan) => {
                let next_len = plan.embed_texts.len();
                if !group_plans.is_empty()
                    && all_texts.len().saturating_add(next_len) > params.pool_input_limit
                {
                    chunks_embedded += embed_pooled_group(
                        cfg,
                        PooledGroup {
                            group_plans: std::mem::take(&mut group_plans),
                            all_texts: std::mem::take(&mut all_texts),
                            ranges: std::mem::take(&mut ranges),
                            base_chunks_embedded: chunks_embedded,
                        },
                        params,
                        state,
                        progress_tx,
                    )
                    .await?;
                }

                let start = all_texts.len();
                all_texts.extend(plan.embed_texts.iter().cloned());
                ranges.push(start..all_texts.len());
                group_plans.push(plan);

                if all_texts.len() >= params.pool_input_limit {
                    chunks_embedded += embed_pooled_group(
                        cfg,
                        PooledGroup {
                            group_plans: std::mem::take(&mut group_plans),
                            all_texts: std::mem::take(&mut all_texts),
                            ranges: std::mem::take(&mut ranges),
                            base_chunks_embedded: chunks_embedded,
                        },
                        params,
                        state,
                        progress_tx,
                    )
                    .await?;
                }
            }
            Err(e) => {
                docs_failed += 1;
                state.docs_completed += 1;
                log_warn(&format!("embed_pipeline doc_failed: {e}"));
                emit_progress(progress_tx, params, state.docs_completed, chunks_embedded).await;
            }
        }
    }

    if !group_plans.is_empty() {
        chunks_embedded += embed_pooled_group(
            cfg,
            PooledGroup {
                group_plans,
                all_texts,
                ranges,
                base_chunks_embedded: chunks_embedded,
            },
            params,
            state,
            progress_tx,
        )
        .await?;
    }

    Ok((chunks_embedded, state.docs_completed, docs_failed))
}

struct PooledGroup {
    group_plans: Vec<DocEmbeddingPlan>,
    all_texts: Vec<String>,
    ranges: Vec<std::ops::Range<usize>>,
    base_chunks_embedded: usize,
}

async fn embed_pooled_group(
    cfg: &Config,
    group: PooledGroup,
    params: &PipelineParams,
    state: &mut PipelineState,
    progress_tx: &Option<tokio::sync::mpsc::Sender<EmbedProgress>>,
) -> Result<usize, SendError> {
    log_info(&format!(
        "embed_pipeline_pooled docs={} chunks={}",
        group.group_plans.len(),
        group.all_texts.len()
    ));
    let vectors = tei_embed_kind(cfg, EmbedKind::Document, &group.all_texts)
        .await
        .map_err(|e| format!("pooled TEI embed failed: {e}"))?;
    if vectors.len() != group.all_texts.len() {
        return Err(format!(
            "pooled TEI vector count mismatch: {} vectors for {} chunks",
            vectors.len(),
            group.all_texts.len()
        )
        .into());
    }

    let mut chunks_embedded = 0usize;
    for (plan, range) in group.group_plans.into_iter().zip(group.ranges) {
        let doc_vectors = vectors[range].to_vec();
        let embedded = build_embedded_doc_from_vectors(plan.doc, doc_vectors, cfg, params.mode)?;
        chunks_embedded += push_embedded_doc(embedded, state);
        state.docs_completed += 1;
        if state.pending_points.len() >= params.flush_point_threshold {
            flush_and_cleanup(cfg, &mut state.pending_points, &mut state.cleanup_queue).await?;
        }
        emit_progress(
            progress_tx,
            params,
            state.docs_completed,
            group.base_chunks_embedded + chunks_embedded,
        )
        .await;
    }

    Ok(chunks_embedded)
}

async fn emit_progress(
    progress_tx: &Option<tokio::sync::mpsc::Sender<EmbedProgress>>,
    params: &PipelineParams,
    docs_completed: usize,
    chunks_embedded: usize,
) {
    if let Some(tx) = progress_tx {
        // If the receiver has been dropped, ignore the error and continue — embed
        // results are what matter; progress reporting is best-effort. (B-L5)
        let _ = tx
            .send(EmbedProgress {
                docs_total: params.docs_total,
                docs_completed,
                chunks_embedded,
            })
            .await;
    }
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
    let flush_threshold = cfg.qdrant_point_buffer.clamp(128, 16_384);
    let pool_input_limit = if is_openai_compatible_embedding_url(cfg) {
        env_usize_clamped("AXON_OPENAI_EMBED_POOL_MAX_INPUTS", 1024, 64, 65_536)
    } else {
        env_usize_clamped("AXON_EMBED_POOL_MAX_INPUTS", 512, 64, 65_536)
    };

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
        skip_stale_tail_cleanup: false,
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
    let bootstrap = bootstrap_first_doc(
        cfg,
        first_doc,
        doc_timeout_secs,
        &mut state.pending_points,
        &mut state.cleanup_queue,
    )
    .await?;
    let mode = bootstrap.mode;
    let restore_indexing_threshold = bootstrap.restore_indexing_threshold;
    let mut chunks_embedded = bootstrap.chunks_embedded;
    let mut docs_failed = bootstrap.docs_failed;
    state.skip_stale_tail_cleanup = bootstrap.skip_stale_tail_cleanup;
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
        mode,
        flush_point_threshold: flush_threshold,
        pool_input_limit,
        docs_total,
    };

    // Phase 2: pool remaining docs' chunks into shared TEI batches with the
    // known mode. This avoids one partially-filled TEI request per document.
    let (phase2_chunks, _phase2_completed, phase2_failed) =
        drain_pooled_docs(cfg, &mut work, &params, &mut state, &progress_tx).await?;
    chunks_embedded += phase2_chunks;
    docs_failed += phase2_failed;

    flush_and_cleanup(cfg, &mut state.pending_points, &mut state.cleanup_queue).await?;
    if restore_indexing_threshold {
        restore_indexing_threshold_after_load(cfg).await?;
    }

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
