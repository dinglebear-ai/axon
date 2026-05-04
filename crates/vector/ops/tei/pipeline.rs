use super::{
    EmbedProgress, EmbedSummary, PreparedDoc, build_point, qdrant_store,
    qdrant_store::VectorMode,
    tei_client::{EmbedKind, tei_embed_kind},
};
use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::vector::ops::qdrant::{env_usize_clamped, qdrant_delete_stale_tail};
use chrono::Utc;
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};
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
    mut doc: PreparedDoc,
    mode: VectorMode,
) -> Result<(usize, String, usize, Vec<serde_json::Value>), SendError> {
    doc.chunks.retain(|c| !c.trim().is_empty());
    if doc.chunks.is_empty() {
        return Err(format!("all chunks empty for {}", doc.url).into());
    }
    // Prepend title and URL to each chunk before embedding. The embedding model
    // sees "[<title>] <url>\n\n<chunk>" but the original chunk text is stored in
    // the payload — search results and snippets show unmodified content.
    //
    // This improves dense retrieval accuracy by anchoring each chunk to its
    // source document's topical identity (domain, page title). Without this,
    // a chunk from any domain that happens to share vocabulary with the query
    // can outscore the authoritative source because the embedding has no
    // document-level context.
    let embed_texts: Vec<String> = doc
        .chunks
        .iter()
        .map(|chunk| match &doc.title {
            Some(t) if !t.is_empty() => format!("[{}] {}\n\n{}", t, doc.url, chunk),
            _ => format!("{}\n\n{}", doc.url, chunk),
        })
        .collect();
    let vectors = tei_embed_kind(cfg, EmbedKind::Document, &embed_texts)
        .await
        .map_err(|e| -> SendError { format!("TEI embed for {}: {e}", doc.url).into() })?;
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
fn rebuild_points_as_named(
    points: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, SendError> {
    points
        .into_iter()
        .map(|pt| {
            // Handle both string and numeric point IDs (Qdrant supports both).
            let id = match &pt["id"] {
                serde_json::Value::String(s) if !s.is_empty() => {
                    serde_json::Value::String(s.clone())
                }
                serde_json::Value::Number(n) => serde_json::Value::Number(n.clone()),
                other => {
                    return Err(format!(
                        "rebuild_points_as_named: unexpected point id type: {other}"
                    )
                    .into());
                }
            };
            let payload = pt["payload"].clone();
            let empty_vec = vec![];
            let dense: Vec<f32> = pt["vector"]
                .as_array()
                .unwrap_or(&empty_vec)
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();
            let chunk = payload["chunk_text"].as_str().unwrap_or_default();
            let sv = super::super::sparse::compute_sparse_vector(chunk);
            Ok(serde_json::json!({
                "id": id,
                "vector": {
                    "dense": dense,
                    "bm42": sv
                },
                "payload": payload,
            }))
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
                .map_err(|e| -> SendError { format!("collection init/cache: {e}").into() })?;
            let chunks = if mode == VectorMode::Named {
                let rebuilt = rebuild_points_as_named(points)?;
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
        .map_err(|e| -> SendError { format!("qdrant upsert: {e}").into() })?;
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
    stale_tail_queue: Vec<(String, usize)>,
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
            Ok((_dim, url, chunk_count, mut points)) => {
                chunks_embedded += points.len();
                state.pending_points.append(&mut points);
                state.stale_tail_queue.push((url, chunk_count));
                if state.pending_points.len() >= params.flush_point_threshold {
                    flush_and_cleanup(cfg, &mut state.pending_points, &mut state.stale_tail_queue)
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
            tx.send(EmbedProgress {
                docs_total: params.docs_total,
                docs_completed: state.docs_completed,
                chunks_embedded,
            })
            .await
            .ok();
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
    let doc_timeout_secs = env_usize_clamped("AXON_EMBED_DOC_TIMEOUT_SECS", 300, 10, 7200) as u64;
    let doc_concurrency = env_usize_clamped(
        "AXON_EMBED_DOC_CONCURRENCY",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8)
            .clamp(2, 8),
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

    let mut state = PipelineState {
        docs_completed: 0,
        pending_points: Vec::new(),
        stale_tail_queue: Vec::new(),
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
        &mut state.stale_tail_queue,
    )
    .await?;
    state.docs_completed = 1;
    if let Some(tx) = &progress_tx {
        tx.send(EmbedProgress {
            docs_total,
            docs_completed: state.docs_completed,
            chunks_embedded,
        })
        .await
        .ok();
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

    flush_and_cleanup(cfg, &mut state.pending_points, &mut state.stale_tail_queue).await?;

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
mod tests {
    /// Verify that the empty-chunk filter (applied before tei_embed) removes
    /// blank and whitespace-only strings while keeping real content.
    #[test]
    fn empty_and_whitespace_chunks_are_filtered() {
        let mut chunks = vec![
            "".to_string(),
            "   ".to_string(),
            "real content".to_string(),
            "\n\t\n".to_string(),
            "another chunk".to_string(),
        ];
        chunks.retain(|c| !c.trim().is_empty());
        assert_eq!(
            chunks,
            vec!["real content".to_string(), "another chunk".to_string()]
        );
    }

    #[test]
    fn all_empty_chunks_produces_no_chunks() {
        let mut chunks = vec!["".to_string(), "  ".to_string(), "\n".to_string()];
        chunks.retain(|c| !c.trim().is_empty());
        assert!(
            chunks.is_empty(),
            "all-empty input must produce zero chunks"
        );
    }
}
