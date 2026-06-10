use super::{
    EmbedProgress, EmbedSummary, PreparedDoc, build_point, qdrant_store,
    qdrant_store::VectorMode,
    tei_client::{EmbedKind, tei_embed_kind},
};
use crate::core::config::Config;
use crate::core::logging::{log_debug, log_info, log_warn};
use crate::vector::ops::qdrant::{
    PAYLOAD_SCHEMA_VERSION, env_usize_clamped, qdrant_delete_stale_tail,
};
use chrono::Utc;
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};
use uuid::Uuid;

// Aliases used for futures that must be Send to work in FuturesUnordered across await points.
type SendError = Box<dyn Error + Send + Sync>;
type DocFuture<'a> = Pin<Box<dyn Future<Output = Result<EmbeddedDoc, SendError>> + Send + 'a>>;

/// Named result of embedding one `PreparedDoc` through the TEI pipeline.
///
/// Replaces the anonymous `(usize, String, usize, Vec<Value>)` 4-tuple with named
/// fields so destructure sites are self-documenting. (B-H3)
struct EmbeddedDoc {
    dim: usize,
    url: String,
    chunk_count: usize,
    points: Vec<serde_json::Value>,
}

/// Qdrant payload fields owned by the pipeline that `doc.extra` must never overwrite.
///
/// `apply_extra` uses this list as a defensive guard. System fields are authoritative;
/// any extra key that collides is silently dropped. (S-M1 / T-C3)
pub(crate) const RESERVED_PAYLOAD_KEYS: &[&str] = &[
    "url",
    "domain",
    "source_type",
    "content_type",
    "chunk_index",
    "chunk_text",
    "seed_url",
    "scraped_at",
    "payload_schema_version",
    "title",
    "extractor_name",
    "structured_kind",
    "structured_type",
    "structured_id",
    "structured_blob",
];

/// Merge source-specific metadata from `extra` into `payload`, skipping any key that
/// is a reserved system field.
///
/// `extra` is written first so that system fields written by the caller afterwards
/// remain authoritative. The reserved-key guard is a defense-in-depth safeguard
/// against ingest builders accidentally injecting a reserved key. (S-M1 / T-C3)
pub(crate) fn apply_extra(payload: &mut serde_json::Value, extra: &serde_json::Value) {
    let serde_json::Value::Object(map) = extra else {
        return;
    };
    let serde_json::Value::Object(payload_map) = payload else {
        return;
    };
    for (k, v) in map {
        if !RESERVED_PAYLOAD_KEYS.contains(&k.as_str()) {
            payload_map.insert(k.clone(), v.clone());
        }
    }
}

/// Drop whitespace-only chunks, keeping `chunk_extra` (P-H1 per-chunk payload
/// overrides) positionally aligned with `chunks`.
///
/// `chunk_extra[i]` describes `chunks[i]`, but the two are separate vectors.
/// Filtering only `chunks` (e.g. via a bare `retain`) would shift every override
/// after a dropped blank chunk onto the wrong chunk and silently discard the last
/// one — corrupting the per-chunk symbol-boost signal P-H1 exists to preserve. So
/// when overrides are present we filter both by the same predicate in lockstep.
/// When `chunk_extra` is empty (the common crawl/embed/non-code path) only
/// `chunks` is filtered. A non-empty-but-mismatched `chunk_extra` is a producer
/// bug (debug-asserted); in release we filter chunks only and drop the unaligned
/// overrides rather than risk misattributing them.
fn drop_blank_chunks_aligned(chunks: &mut Vec<String>, chunk_extra: &mut Vec<serde_json::Value>) {
    if !chunk_extra.is_empty() && chunk_extra.len() == chunks.len() {
        let paired_chunks = std::mem::take(chunks);
        let paired_extra = std::mem::take(chunk_extra);
        let (kept_chunks, kept_extra): (Vec<String>, Vec<serde_json::Value>) = paired_chunks
            .into_iter()
            .zip(paired_extra)
            .filter(|(c, _)| !c.trim().is_empty())
            .unzip();
        *chunks = kept_chunks;
        *chunk_extra = kept_extra;
    } else {
        debug_assert!(
            chunk_extra.is_empty(),
            "chunk_extra ({}) must be empty or positionally parallel to chunks ({})",
            chunk_extra.len(),
            chunks.len()
        );
        chunks.retain(|c| !c.trim().is_empty());
        chunk_extra.clear();
    }
}

async fn embed_prepared_doc(
    cfg: &Config,
    mut doc: PreparedDoc,
    mode: VectorMode,
) -> Result<EmbeddedDoc, SendError> {
    drop_blank_chunks_aligned(&mut doc.chunks, &mut doc.chunk_extra);
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
    // Origin marker stamped on every chunk: the crawl start URL or ingest target
    // when the job runner set `cfg.seed_url`, otherwise the doc's own URL (direct
    // embed/scrape). `axon refresh` facets on this field to re-enqueue origins.
    let seed_url = cfg
        .seed_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(url.as_str())
        .to_string();
    let timestamp = Utc::now().to_rfc3339();
    let mut points = Vec::with_capacity(vectors.len());
    // Per-chunk payload overrides (P-H1): taken out before `doc.chunks` is moved
    // by `into_iter()` below so the two positionally-parallel vectors zip cleanly.
    let chunk_extra = std::mem::take(&mut doc.chunk_extra);
    for (idx, (chunk, vecv)) in doc.chunks.into_iter().zip(vectors).enumerate() {
        let point_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("{}:{}", url, idx).as_bytes());
        // Apply extra metadata first so that system fields written below always win.
        // RESERVED_PAYLOAD_KEYS in apply_extra() provides a second line of defense. (S-M1)
        let mut payload = serde_json::json!({});
        if let Some(ref extra) = doc.extra {
            apply_extra(&mut payload, extra);
        }
        // Per-chunk overrides win over doc-level extra (reserved system keys excepted).
        if let Some(chunk_override) = chunk_extra.get(idx) {
            apply_extra(&mut payload, chunk_override);
        }
        // System fields — written after extra so they are always authoritative.
        payload["url"] = serde_json::Value::String(url.clone());
        payload["domain"] = serde_json::Value::String(doc.domain.clone());
        payload["source_type"] = serde_json::Value::String(doc.source_type.clone());
        payload["content_type"] = serde_json::Value::String(doc.content_type.to_string());
        payload["chunk_index"] = serde_json::Value::Number(idx.into());
        payload["chunk_text"] = serde_json::Value::String(chunk.clone());
        payload["seed_url"] = serde_json::Value::String(seed_url.clone());
        payload["scraped_at"] = serde_json::Value::String(timestamp.clone());
        // Stamp the schema version so retrieval can opt into version-aware filtering.
        // Existing points without this field are treated as implicit version 1.
        // See `qdrant::PAYLOAD_SCHEMA_VERSION` for the current value. (D-M2)
        payload["payload_schema_version"] =
            serde_json::Value::Number(PAYLOAD_SCHEMA_VERSION.into());
        if let Some(t) = &doc.title {
            payload["title"] = serde_json::Value::String(t.clone());
        }
        // `extractor_name` is OPTIONAL — generic crawl/embed paths leave it
        // None so the field is absent. Vertical extractors set it to a stable
        // keyword; filtering on absence is the agent-native pattern.
        if let Some(name) = &doc.extractor_name
            && !name.is_empty()
        {
            payload["extractor_name"] = serde_json::Value::String(name.clone());
        }
        // Structured-data fields are OPTIONAL — only populated when a page
        // produced JSON-LD / __NEXT_DATA__ / SvelteKit data.
        if let Some(sd) = &doc.structured {
            payload["structured_kind"] = serde_json::Value::String(sd.kind.to_string());
            if let Some(t) = &sd.schema_type {
                payload["structured_type"] = serde_json::Value::String(t.clone());
            }
            if let Some(id) = &sd.schema_id {
                payload["structured_id"] = serde_json::Value::String(id.clone());
            }
            payload["structured_blob"] = sd.blob.clone();
        }
        points.push(build_point(point_id, vecv, &chunk, payload, mode));
    }
    // Return URL and chunk count so the caller can run stale-tail cleanup
    // AFTER the upsert succeeds -- never before.
    Ok(EmbeddedDoc {
        dim,
        url,
        chunk_count,
        points,
    })
}

async fn embed_prepared_doc_with_timeout(
    cfg: &Config,
    doc: PreparedDoc,
    timeout_secs: u64,
    mode: VectorMode,
) -> Result<EmbeddedDoc, SendError> {
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
    // Constant empty slice avoids a per-point heap allocation for the fallback
    // case where a point has no "vector" array. (B-L6)
    const EMPTY: &[serde_json::Value] = &[];
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
            let dense: Vec<f32> = pt["vector"]
                .as_array()
                .map(|v| v.as_slice())
                .unwrap_or(EMPTY)
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
        Ok(EmbeddedDoc {
            dim,
            url,
            chunk_count,
            points,
        }) => {
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
        // chunk_count == 1 means the stale-tail filter is `chunk_index >= 1`,
        // which matches zero points by construction — skip the no-op DELETE. (P-H1)
        if count <= 1 {
            continue;
        }
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
            Ok(EmbeddedDoc {
                url,
                chunk_count,
                mut points,
                ..
            }) => {
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
#[path = "pipeline_tests.rs"]
mod tests;
