use super::super::{PreparedDoc, qdrant_store, qdrant_store::VectorMode};
use super::payload::{EmbeddedDoc, SendError, embed_prepared_doc_with_timeout};
use crate::core::config::Config;
use crate::core::logging::log_warn;
use crate::vector::ops::qdrant::{qdrant_delete_local_file_fragments, qdrant_delete_stale_tail};

pub(super) enum PostUpsertCleanup {
    StaleTail { url: String, new_chunk_count: usize },
    LocalLegacyFragments { file_url: String },
}

pub(super) struct BootstrapResult {
    pub(super) mode: VectorMode,
    pub(super) chunks_embedded: usize,
    pub(super) docs_failed: usize,
    pub(super) skip_stale_tail_cleanup: bool,
    pub(super) restore_indexing_threshold: bool,
}

/// Rebuild Unnamed-format points into Named format by adding BM42 sparse vectors.
///
/// The first doc in a pipeline is embedded before the collection mode is known.
/// If the collection turns out to be Named, these points need the `"dense"` +
/// `"bm42"` named vector structure instead of a flat `"vector": [...]` array.
///
/// NOTE (code-L2): this re-parses floats out of `serde_json::Value` because the
/// first-doc bootstrap only has the JSON points (built by `build_point`), not the
/// raw `Vec<f32>`. Threading raw dense vectors through `EmbeddedDoc` to avoid the
/// re-parse would change the `EmbeddedDoc`/`build_point` data shape, so it is left
/// as-is to preserve behavior. This runs once per pipeline (first doc only).
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
            let sv = crate::vector::ops::sparse::compute_sparse_vector_for_indexing(chunk);
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
pub(super) async fn bootstrap_first_doc(
    cfg: &Config,
    doc: PreparedDoc,
    doc_timeout_secs: u64,
    pending_points: &mut Vec<serde_json::Value>,
    cleanup_queue: &mut Vec<PostUpsertCleanup>,
) -> Result<BootstrapResult, SendError> {
    match embed_prepared_doc_with_timeout(cfg, doc, doc_timeout_secs, VectorMode::Unnamed).await {
        Ok(EmbeddedDoc {
            dim,
            url,
            chunk_count,
            points,
            local_legacy_fragment_url,
        }) => {
            let init = qdrant_store::collection_init_or_cached(cfg, dim)
                .await
                .map_err(|e| -> SendError { format!("collection init/cache: {e}").into() })?;
            let mode = init.mode;
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
            if !init.created_now {
                cleanup_queue.push(PostUpsertCleanup::StaleTail {
                    url,
                    new_chunk_count: chunk_count,
                });
                if let Some(url) = local_legacy_fragment_url {
                    cleanup_queue.push(PostUpsertCleanup::LocalLegacyFragments { file_url: url });
                }
            }
            Ok(BootstrapResult {
                mode,
                chunks_embedded: chunks,
                docs_failed: 0,
                skip_stale_tail_cleanup: init.created_now,
                restore_indexing_threshold: init.restore_indexing_threshold,
            })
        }
        Err(e) => {
            log_warn(&format!("embed_pipeline first_doc_failed: {e}"));
            // code-L4: distinguish "first doc embed failed" (above) from "lost the
            // collection mode". When mode detection ALSO errors here we silently
            // degraded the whole batch to dense-only Unnamed — emit a distinct warning
            // so operators can tell the two failure modes apart.
            let mode = match qdrant_store::get_or_fetch_vector_mode(cfg).await {
                Ok(mode) => mode,
                Err(mode_err) => {
                    log_warn(&format!(
                        "embed_pipeline first_doc_mode_lost: failed to resolve collection vector mode after first-doc failure ({mode_err}); degrading entire batch to dense-only Unnamed"
                    ));
                    VectorMode::Unnamed
                }
            };
            Ok(BootstrapResult {
                mode,
                chunks_embedded: 0,
                docs_failed: 1,
                skip_stale_tail_cleanup: false,
                restore_indexing_threshold: false,
            })
        }
    }
}

pub(super) async fn restore_indexing_threshold_after_load(cfg: &Config) -> Result<(), SendError> {
    qdrant_store::restore_bulk_indexing_threshold_after_load(cfg)
        .await
        .map_err(|e| -> SendError { format!("qdrant restore indexing threshold: {e}").into() })
}

pub(super) async fn flush_and_cleanup(
    cfg: &Config,
    points: &mut Vec<serde_json::Value>,
    cleanup_queue: &mut Vec<PostUpsertCleanup>,
) -> Result<(), SendError> {
    if points.is_empty() {
        return Ok(());
    }
    qdrant_store::qdrant_upsert(cfg, points)
        .await
        .map_err(|e| -> SendError { format!("qdrant upsert: {e}").into() })?;
    points.clear();
    for cleanup in cleanup_queue.drain(..) {
        match cleanup {
            PostUpsertCleanup::StaleTail {
                url,
                new_chunk_count,
            } => {
                // chunk_count == 0 would delete the whole URL; successful docs
                // should never reach here empty, but keep the guard explicit.
                if new_chunk_count == 0 {
                    continue;
                }
                if let Err(e) = qdrant_delete_stale_tail(cfg, &url, new_chunk_count).await {
                    return Err(format!("embed stale-tail cleanup failed for {url}: {e}").into());
                }
            }
            PostUpsertCleanup::LocalLegacyFragments { file_url } => {
                if let Err(e) = qdrant_delete_local_file_fragments(cfg, &file_url).await {
                    return Err(
                        format!("embed local-fragment cleanup failed for {file_url}: {e}").into(),
                    );
                }
            }
        }
    }
    Ok(())
}
