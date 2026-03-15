use super::{
    EmbedProgress, EmbedSummary, PreparedDoc, build_point, qdrant_store, qdrant_store::VectorMode,
    tei_client::tei_embed,
};
use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::vector::ops::qdrant::{env_usize_clamped, qdrant_delete_stale_tail};
use chrono::Utc;
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use uuid::Uuid;

// Aliases used for futures that must be Send to work in FuturesUnordered across await points.
type SendError = Box<dyn Error + Send + Sync>;
type DocFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<(usize, String, usize, Vec<serde_json::Value>), SendError>>
            + Send
            + 'a,
    >,
>;

async fn embed_prepared_doc(
    cfg: &Config,
    doc: PreparedDoc,
    mode: VectorMode,
) -> Result<(usize, String, usize, Vec<serde_json::Value>), SendError> {
    let vectors = tei_embed(cfg, &doc.chunks)
        .await
        .map_err(|e| -> SendError { e.to_string().into() })?;
    if vectors.is_empty() {
        return Err(format!("TEI returned no vectors for {}", doc.url).into());
    }
    if vectors.len() != doc.chunks.len() {
        return Err(format!(
            "TEI vector count mismatch for {}: {} vectors for {} chunks",
            doc.url,
            vectors.len(),
            doc.chunks.len()
        )
        .into());
    }
    log_debug(&format!(
        "embed_doc url={} chunk_count={}",
        doc.url,
        doc.chunks.len()
    ));
    let dim = vectors[0].len();
    let chunk_count = doc.chunks.len();
    let url = doc.url.clone();
    let timestamp = Utc::now().to_rfc3339();
    let mut points = Vec::with_capacity(vectors.len());
    for (idx, (chunk, vecv)) in doc.chunks.into_iter().zip(vectors.into_iter()).enumerate() {
        let point_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("{}:{}", url, idx).as_bytes());
        let mut payload = serde_json::json!({
            "url": url,
            "domain": doc.domain,
            "source_type": doc.source_type,
            "content_type": doc.content_type,
            "chunk_index": idx,
            "chunk_text": chunk,
            "scraped_at": timestamp,
        });
        if let Some(t) = &doc.title {
            payload["title"] = serde_json::Value::String(t.clone());
        }
        if let Some(serde_json::Value::Object(map)) = &doc.extra {
            for (k, v) in map {
                payload[k] = v.clone();
            }
        }
        points.push(build_point(point_id, vecv, &chunk, payload, mode));
    }
    // Return URL and chunk count so the caller can run stale-tail cleanup
    // AFTER the upsert succeeds -- never before.
    Ok((dim, url, chunk_count, points))
}

async fn embed_prepared_doc_with_timeout(
    cfg: &Config,
    doc: PreparedDoc,
    timeout_secs: u64,
    mode: VectorMode,
) -> Result<(usize, String, usize, Vec<serde_json::Value>), SendError> {
    let url = doc.url.clone();
    match tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        embed_prepared_doc(cfg, doc, mode),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            log_warn(&format!("embed timed out after {timeout_secs}s for {url}"));
            Err(format!("embed timed out after {timeout_secs}s while processing {url}").into())
        }
    }
}

/// Rebuild Unnamed-format points into Named format by adding BM42 sparse vectors.
///
/// The first doc in a pipeline is embedded before the collection mode is known.
/// If the collection turns out to be Named, these points need the `"dense"` +
/// `"bm42"` named vector structure instead of a flat `"vector": [...]` array.
fn rebuild_points_as_named(points: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    points
        .into_iter()
        .map(|pt| {
            let id = pt["id"].as_str().unwrap_or_default().to_string();
            let payload = pt["payload"].clone();
            let dense: Vec<f32> = pt["vector"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();
            let chunk = payload["chunk_text"].as_str().unwrap_or_default();
            let sv = super::super::sparse::compute_sparse_vector(chunk);
            serde_json::json!({
                "id": id,
                "vector": {
                    "dense": dense,
                    "bm42": sv.to_json()
                },
                "payload": payload,
            })
        })
        .collect()
}

/// Process the first doc to determine the embedding dimension and collection VectorMode.
///
/// Returns `(mode, chunks_embedded, docs_failed)`. On success the resulting points
/// (rebuilt if necessary) are appended to `pending_points` and the URL is queued
/// for stale-tail cleanup. On failure, falls back to `get_or_fetch_vector_mode`.
async fn bootstrap_first_doc(
    cfg: &Config,
    doc: PreparedDoc,
    doc_timeout_secs: u64,
    pending_points: &mut Vec<serde_json::Value>,
    stale_tail_queue: &mut Vec<(String, usize)>,
) -> Result<(VectorMode, usize, usize), SendError> {
    match embed_prepared_doc_with_timeout(cfg, doc, doc_timeout_secs, VectorMode::Unnamed).await {
        Ok((dim, url, chunk_count, points)) => {
            let mode = qdrant_store::collection_init_or_cached(cfg, dim)
                .await
                .map_err(|e| -> SendError { e.to_string().into() })?;
            let chunks = if mode == VectorMode::Named {
                let rebuilt = rebuild_points_as_named(points);
                let n = rebuilt.len();
                pending_points.extend(rebuilt);
                n
            } else {
                let n = points.len();
                pending_points.extend(points);
                n
            };
            stale_tail_queue.push((url, chunk_count));
            Ok((mode, chunks, 0))
        }
        Err(e) => {
            log_warn(&format!("embed_pipeline first_doc_failed: {e}"));
            let mode = qdrant_store::get_or_fetch_vector_mode(cfg)
                .await
                .unwrap_or(VectorMode::Unnamed);
            Ok((mode, 0, 1))
        }
    }
}

async fn flush_and_cleanup(
    cfg: &Config,
    points: &mut Vec<serde_json::Value>,
    stale_tail_queue: &mut Vec<(String, usize)>,
) -> Result<(), SendError> {
    if points.is_empty() {
        return Ok(());
    }
    qdrant_store::qdrant_upsert(cfg, points)
        .await
        .map_err(|e| -> SendError { e.to_string().into() })?;
    points.clear();
    for (tail_url, count) in stale_tail_queue.drain(..) {
        if let Err(e) = qdrant_delete_stale_tail(cfg, &tail_url, count).await {
            log_warn(&format!(
                "embed stale-tail cleanup failed for {tail_url}: {e}"
            ));
        }
    }
    Ok(())
}

/// Drive the concurrent doc-processing loop (Phase 2) after mode is known.
///
/// Drains remaining docs from `work`, processing up to `doc_concurrency` in
/// parallel. Returns `(chunks_embedded, docs_completed, docs_failed)`.
#[allow(clippy::too_many_arguments)]
async fn drain_concurrent_docs<'a>(
    cfg: &'a Config,
    work: &mut impl Iterator<Item = PreparedDoc>,
    doc_concurrency: usize,
    doc_timeout_secs: u64,
    mode: VectorMode,
    flush_point_threshold: usize,
    docs_total: usize,
    mut docs_completed: usize,
    progress_tx: &Option<tokio::sync::mpsc::Sender<EmbedProgress>>,
    pending_points: &mut Vec<serde_json::Value>,
    stale_tail_queue: &mut Vec<(String, usize)>,
) -> Result<(usize, usize, usize), SendError> {
    let mut inflight: FuturesUnordered<DocFuture<'a>> = FuturesUnordered::new();
    let mut chunks_embedded = 0usize;
    let mut docs_failed = 0usize;

    for _ in 0..doc_concurrency {
        if let Some(doc) = work.next() {
            inflight.push(Box::pin(embed_prepared_doc_with_timeout(
                cfg,
                doc,
                doc_timeout_secs,
                mode,
            )));
        }
    }

    while let Some(result) = inflight.next().await {
        match result {
            Ok((_dim, url, chunk_count, mut points)) => {
                chunks_embedded += points.len();
                pending_points.append(&mut points);
                stale_tail_queue.push((url, chunk_count));
                if pending_points.len() >= flush_point_threshold {
                    flush_and_cleanup(cfg, pending_points, stale_tail_queue).await?;
                }
            }
            Err(e) => {
                docs_failed += 1;
                log_warn(&format!("embed_pipeline doc_failed: {e}"));
            }
        }
        docs_completed += 1;
        if let Some(tx) = progress_tx {
            tx.send(EmbedProgress {
                docs_total,
                docs_completed,
                chunks_embedded,
            })
            .await
            .ok();
        }
        if let Some(doc) = work.next() {
            inflight.push(Box::pin(embed_prepared_doc_with_timeout(
                cfg,
                doc,
                doc_timeout_secs,
                mode,
            )));
        }
    }
    Ok((chunks_embedded, docs_completed, docs_failed))
}

pub(super) async fn run_embed_pipeline(
    cfg: &Config,
    prepared: Vec<PreparedDoc>,
    progress_tx: Option<tokio::sync::mpsc::Sender<EmbedProgress>>,
) -> Result<EmbedSummary, SendError> {
    let docs_total = prepared.len();
    log_info(&format!("embed_pipeline docs={}", docs_total));
    let doc_timeout_secs = env_usize_clamped("AXON_EMBED_DOC_TIMEOUT_SECS", 300, 10, 7200) as u64;
    let doc_concurrency = env_usize_clamped(
        "AXON_EMBED_DOC_CONCURRENCY",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8)
            .clamp(2, 16),
        1,
        64,
    );
    let flush_threshold = env_usize_clamped("AXON_QDRANT_POINT_BUFFER", 256, 128, 16384);

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

    let mut pending_points: Vec<serde_json::Value> = Vec::new();
    let mut stale_tail_queue: Vec<(String, usize)> = Vec::new();

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
        &mut pending_points,
        &mut stale_tail_queue,
    )
    .await?;
    let docs_completed = 1usize;
    if let Some(tx) = &progress_tx {
        tx.send(EmbedProgress {
            docs_total,
            docs_completed,
            chunks_embedded,
        })
        .await
        .ok();
    }

    // Phase 2: process remaining docs concurrently with the known mode.
    let (phase2_chunks, _phase2_completed, phase2_failed) = drain_concurrent_docs(
        cfg,
        &mut work,
        doc_concurrency,
        doc_timeout_secs,
        mode,
        flush_threshold,
        docs_total,
        docs_completed,
        &progress_tx,
        &mut pending_points,
        &mut stale_tail_queue,
    )
    .await?;
    chunks_embedded += phase2_chunks;
    docs_failed += phase2_failed;

    flush_and_cleanup(cfg, &mut pending_points, &mut stale_tail_queue).await?;

    if docs_failed > 0 {
        log_warn(&format!(
            "embed_pipeline completed with {docs_failed}/{docs_total} doc failures"
        ));
    }
    Ok(EmbedSummary {
        docs_embedded: docs_total - docs_failed,
        docs_failed,
        chunks_embedded,
    })
}
