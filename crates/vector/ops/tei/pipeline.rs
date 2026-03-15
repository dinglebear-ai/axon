use super::{EmbedProgress, EmbedSummary, PreparedDoc, qdrant_store, tei_client::tei_embed};
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
        points.push(serde_json::json!({
            "id": point_id.to_string(),
            "vector": vecv,
            "payload": payload,
        }));
    }
    // Return URL and chunk count so the caller can run stale-tail cleanup
    // AFTER the upsert succeeds — never before.
    Ok((dim, url, chunk_count, points))
}

async fn embed_prepared_doc_with_timeout(
    cfg: &Config,
    doc: PreparedDoc,
    timeout_secs: u64,
) -> Result<(usize, String, usize, Vec<serde_json::Value>), SendError> {
    let url = doc.url.clone();
    match tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        embed_prepared_doc(cfg, doc),
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
    let flush_point_threshold = env_usize_clamped("AXON_QDRANT_POINT_BUFFER", 256, 128, 16384);

    let mut work = prepared.into_iter();
    let mut inflight: FuturesUnordered<DocFuture<'_>> = FuturesUnordered::new();
    if let Some(tx) = &progress_tx {
        let _ = tx
            .send(EmbedProgress {
                docs_total,
                docs_completed: 0,
                chunks_embedded: 0,
            })
            .await;
    }
    for _ in 0..doc_concurrency {
        if let Some(doc) = work.next() {
            inflight.push(Box::pin(embed_prepared_doc_with_timeout(
                cfg,
                doc,
                doc_timeout_secs,
            )));
        }
    }

    let mut chunks_embedded = 0usize;
    let mut docs_completed = 0usize;
    let mut docs_failed = 0usize;
    let mut pending_points: Vec<serde_json::Value> = Vec::new();
    // Track URLs and their chunk counts for stale-tail cleanup after upsert.
    let mut stale_tail_queue: Vec<(String, usize)> = Vec::new();
    let mut collection_dim: Option<usize> = None;

    while let Some(result) = inflight.next().await {
        match result {
            Ok((dim, url, chunk_count, mut points)) => {
                match collection_dim {
                    None => {
                        if qdrant_store::collection_needs_init(&cfg.collection) {
                            qdrant_store::ensure_collection(cfg, dim)
                                .await
                                .map_err(|e| -> SendError { e.to_string().into() })?;
                        }
                        collection_dim = Some(dim);
                    }
                    Some(existing) if existing != dim => {
                        return Err(format!(
                            "TEI embedding dimension mismatch: expected {}, got {}",
                            existing, dim
                        )
                        .into());
                    }
                    _ => {}
                }
                chunks_embedded += points.len();
                pending_points.append(&mut points);
                stale_tail_queue.push((url, chunk_count));

                if pending_points.len() >= flush_point_threshold {
                    flush_and_cleanup(cfg, &mut pending_points, &mut stale_tail_queue).await?;
                }
            }
            Err(e) => {
                docs_failed += 1;
                log_warn(&format!("embed_pipeline doc_failed: {e}"));
            }
        }

        docs_completed += 1;
        if let Some(tx) = &progress_tx {
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
            )));
        }
    }

    // Flush remaining points.
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
