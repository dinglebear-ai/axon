use super::{EmbedProgress, EmbedSummary, PreparedDoc, qdrant_store, tei_client::tei_embed};
use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::vector::ops::qdrant::{env_usize_clamped, qdrant_delete_by_url_filter};
use chrono::Utc;
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;
use std::time::Duration;
use uuid::Uuid;

fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => match v.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

fn strict_predelete() -> bool {
    static STRICT: OnceLock<bool> = OnceLock::new();
    *STRICT.get_or_init(|| env_bool("AXON_EMBED_STRICT_PREDELETE", true))
}

// Aliases used for futures that must be Send to work in FuturesUnordered across await points.
type SendError = Box<dyn Error + Send + Sync>;
type DocFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(usize, Vec<serde_json::Value>), SendError>> + Send + 'a>>;

async fn embed_prepared_doc(
    cfg: &Config,
    doc: PreparedDoc,
) -> Result<(usize, Vec<serde_json::Value>), SendError> {
    if let Err(err) = qdrant_delete_by_url_filter(cfg, &doc.url).await {
        if strict_predelete() {
            return Err(format!("embed pre-delete failed for {}: {}", doc.url, err).into());
        }
        log_warn(&format!(
            "embed pre-delete skipped for {} due to qdrant error (strict=false): {}",
            doc.url, err
        ));
    }
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
    let timestamp = Utc::now().to_rfc3339();
    let mut points = Vec::with_capacity(vectors.len());
    for (idx, (chunk, vecv)) in doc.chunks.into_iter().zip(vectors.into_iter()).enumerate() {
        let point_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("{}:{}", doc.url, idx).as_bytes(),
        );
        // source_command removed — duplicated source_type
        let mut payload = serde_json::json!({
            "url": doc.url,
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
    Ok((dim, points))
}

async fn embed_prepared_doc_with_timeout(
    cfg: &Config,
    doc: PreparedDoc,
    timeout_secs: u64,
) -> Result<(usize, Vec<serde_json::Value>), SendError> {
    let url = doc.url.clone();
    match tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        embed_prepared_doc(cfg, doc),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            log_warn(&format!(
                "embed timed out after {timeout_secs}s for {url}; \
                 pre-delete may have run — Qdrant index is incomplete for this URL until re-embed"
            ));
            Err(format!("embed timed out after {timeout_secs}s while processing {url}").into())
        }
    }
}

pub(super) async fn run_embed_pipeline(
    cfg: &Config,
    prepared: Vec<PreparedDoc>,
    progress_tx: Option<tokio::sync::mpsc::Sender<EmbedProgress>>,
) -> Result<EmbedSummary, SendError> {
    let docs_embedded = prepared.len();
    log_info(&format!("embed_pipeline docs={}", docs_embedded));
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
                docs_total: docs_embedded,
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
    let mut pending_points: Vec<serde_json::Value> = Vec::new();
    let mut collection_dim: Option<usize> = None;

    while let Some(result) = inflight.next().await {
        let (dim, mut points) = result?;
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
        docs_completed += 1;
        if let Some(tx) = &progress_tx {
            tx.send(EmbedProgress {
                docs_total: docs_embedded,
                docs_completed,
                chunks_embedded,
            })
            .await
            .ok();
        }
        pending_points.append(&mut points);
        if pending_points.len() >= flush_point_threshold {
            qdrant_store::qdrant_upsert(cfg, &pending_points)
                .await
                .map_err(|e| -> SendError { e.to_string().into() })?;
            pending_points.clear();
        }

        if let Some(doc) = work.next() {
            inflight.push(Box::pin(embed_prepared_doc_with_timeout(
                cfg,
                doc,
                doc_timeout_secs,
            )));
        }
    }
    if !pending_points.is_empty() {
        qdrant_store::qdrant_upsert(cfg, &pending_points)
            .await
            .map_err(|e| -> SendError { e.to_string().into() })?;
    }

    Ok(EmbedSummary {
        docs_embedded,
        chunks_embedded,
    })
}
