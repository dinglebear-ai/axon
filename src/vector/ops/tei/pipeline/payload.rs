use super::super::{
    PreparedDoc, build_point,
    qdrant_store::VectorMode,
    tei_client::{EmbedKind, tei_embed_kind},
};
use crate::core::config::Config;
use crate::core::logging::{log_debug, log_warn};
use crate::vector::ops::qdrant::PAYLOAD_SCHEMA_VERSION;
use chrono::Utc;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use uuid::Uuid;

// Aliases used for futures that must be Send to work in FuturesUnordered across await points.
pub(super) type SendError = Box<dyn Error + Send + Sync>;
pub(super) type DocFuture<'a> =
    Pin<Box<dyn Future<Output = Result<EmbeddedDoc, SendError>> + Send + 'a>>;

/// Named result of embedding one `PreparedDoc` through the TEI pipeline.
///
/// Replaces the anonymous `(usize, String, usize, Vec<Value>)` 4-tuple with named
/// fields so destructure sites are self-documenting. (B-H3)
pub(super) struct EmbeddedDoc {
    pub(super) dim: usize,
    pub(super) url: String,
    pub(super) chunk_count: usize,
    pub(super) points: Vec<serde_json::Value>,
    pub(super) local_legacy_fragment_url: Option<String>,
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
pub(crate) fn drop_blank_chunks_aligned(
    chunks: &mut Vec<String>,
    chunk_extra: &mut Vec<serde_json::Value>,
) {
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
    let local_legacy_fragment_url = doc.local_legacy_fragment_url.take();
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
    let chunk_point_ids = std::mem::take(&mut doc.chunk_point_ids);
    for (idx, (chunk, vecv)) in doc.chunks.into_iter().zip(vectors).enumerate() {
        let point_id = chunk_point_ids.get(idx).copied().unwrap_or_else(|| {
            Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("{}:{}", url, idx).as_bytes())
        });
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
        local_legacy_fragment_url,
    })
}

pub(super) async fn embed_prepared_doc_with_timeout(
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
