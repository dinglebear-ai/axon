use crate::crates::core::config::Config;
use crate::crates::vector::ops::input;
use crate::crates::vector::ops::qdrant::qdrant_delete_stale_tail;
use std::error::Error;

mod pipeline;
mod prepare;
mod qdrant_store;
mod tei_client;
mod tei_manifest;
#[cfg(test)]
mod tests;
pub(crate) use tei_client::tei_embed;

#[derive(Debug, Clone, Copy)]
pub struct EmbedSummary {
    pub docs_embedded: usize,
    pub chunks_embedded: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct EmbedProgress {
    pub docs_total: usize,
    pub docs_completed: usize,
    pub chunks_embedded: usize,
}

#[derive(Debug)]
pub(crate) struct PreparedDoc {
    pub(crate) url: String,
    pub(crate) domain: String,
    pub(crate) chunks: Vec<String>,
    /// "embed" for crawl path, "github"/"reddit"/"youtube"/"sessions" for ingest.
    pub(crate) source_type: String,
    /// "markdown" for crawl path, "text" for ingest sources.
    pub(crate) content_type: &'static str,
    pub(crate) title: Option<String>,
    /// Source-specific metadata fields (gh_*, reddit_*, yt_*).
    pub(crate) extra: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct EmbedDocument {
    pub content: String,
    pub url: String,
    pub source_type: String,
    pub title: Option<String>,
    pub extra: Option<serde_json::Value>,
    pub file_extension: Option<String>,
}

#[derive(Debug)]
struct PreparedBatchDocument {
    url: String,
    domain: String,
    source_type: String,
    title: Option<String>,
    extra: Option<serde_json::Value>,
    content_type: &'static str,
    scraped_at: String,
    chunks: Vec<String>,
}

/// Shared implementation for text embedding with optional extra payload fields.
async fn embed_text_impl(
    cfg: &Config,
    content: &str,
    url: &str,
    source_type: &str,
    title: Option<&str>,
    extra: Option<&serde_json::Value>,
) -> Result<usize, Box<dyn Error>> {
    if content.trim().is_empty() {
        return Ok(0);
    }
    let chunks = input::chunk_text(content);
    if chunks.is_empty() {
        return Ok(0);
    }
    embed_chunks_impl(cfg, chunks, url, source_type, title, extra).await
}

/// Embed pre-chunked text into Qdrant. Shared by text and code embedding paths.
async fn embed_chunks_impl(
    cfg: &Config,
    chunks: Vec<String>,
    url: &str,
    source_type: &str,
    title: Option<&str>,
    extra: Option<&serde_json::Value>,
) -> Result<usize, Box<dyn Error>> {
    let vectors = tei_embed(cfg, &chunks).await?;
    if vectors.is_empty() {
        return Err(format!("TEI returned no vectors for {url}").into());
    }
    if vectors.len() != chunks.len() {
        return Err(format!(
            "TEI vector count mismatch for {url}: {} vectors for {} chunks",
            vectors.len(),
            chunks.len()
        )
        .into());
    }
    let dim = vectors[0].len();
    if qdrant_store::collection_needs_init(&cfg.collection) {
        qdrant_store::ensure_collection(cfg, dim).await?;
    }
    let domain = spider::url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    let timestamp = chrono::Utc::now().to_rfc3339();
    let mut points = Vec::with_capacity(vectors.len());
    for (idx, (chunk, vecv)) in chunks.into_iter().zip(vectors.into_iter()).enumerate() {
        let point_id = uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_URL,
            format!("{url}:{idx}").as_bytes(),
        );
        let mut payload = serde_json::json!({
            "url": url,
            "domain": domain,
            "source_type": source_type,
            "source_command": source_type,
            "content_type": "text",
            "chunk_index": idx,
            "chunk_text": chunk,
            "scraped_at": timestamp,
        });
        if let Some(t) = title {
            payload["title"] = serde_json::Value::String(t.to_string());
        }
        // Merge source-specific extra fields (e.g. YouTube channel, upload date)
        if let Some(serde_json::Value::Object(map)) = extra {
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
    // Upsert FIRST so the fresh document is always available in the index.
    // Never delete before the upsert succeeds — a pre-delete followed by a
    // failed upsert permanently destroys the previously-indexed content.
    //
    // Point IDs are deterministic (UUID v5 over "url:chunk_idx"), so upserting
    // the new batch automatically overwrites any chunks at the same indices.
    // After a successful upsert, delete stale tail chunks — orphan points with
    // chunk_index >= new_count that survived from a previous larger ingest.
    let new_count = points.len();
    qdrant_store::qdrant_upsert(cfg, &points).await?;
    // Stale-tail cleanup: remove any old chunks for this URL with index >=
    // new_count. Uses a range filter so we only touch genuinely orphaned points.
    // If the prior ingest produced the same number of chunks or fewer, Qdrant
    // will match zero points and the call is a cheap no-op.
    qdrant_delete_stale_tail(cfg, url, new_count).await?;
    Ok(new_count)
}

/// Embed arbitrary text content with explicit source metadata into Qdrant.
pub async fn embed_text_with_metadata(
    cfg: &Config,
    content: &str,
    url: &str,
    source_type: &str,
    title: Option<&str>,
) -> Result<usize, Box<dyn Error>> {
    embed_text_impl(cfg, content, url, source_type, title, None).await
}

/// Like `embed_text_with_metadata` but merges `extra` fields into every chunk's Qdrant payload.
/// `extra` must be a JSON object; non-object values are ignored.
/// Use this for source-specific metadata (e.g. YouTube channel, upload date, tags).
pub async fn embed_text_with_extra_payload(
    cfg: &Config,
    content: &str,
    url: &str,
    source_type: &str,
    title: Option<&str>,
    extra: &serde_json::Value,
) -> Result<usize, Box<dyn Error>> {
    embed_text_impl(cfg, content, url, source_type, title, Some(extra)).await
}

/// Embed a batch of pre-prepared documents through the unified concurrent pipeline.
///
/// Each `PreparedDoc` must already be chunked. The pipeline processes documents
/// concurrently (AXON_EMBED_DOC_CONCURRENCY), one TEI call per document, and
/// batches Qdrant upserts at 256 points. This is the single entry point for all
/// ingest sources and the crawl path.
// Allow dead_code: callers will be added in subsequent tasks when ingest sources migrate.
#[allow(dead_code)]
pub(crate) async fn embed_prepared_docs(
    cfg: &Config,
    docs: Vec<PreparedDoc>,
    progress_tx: Option<tokio::sync::mpsc::Sender<EmbedProgress>>,
) -> Result<EmbedSummary, Box<dyn Error>> {
    if docs.is_empty() {
        return prepare::emit_empty_embed(progress_tx);
    }
    pipeline::run_embed_pipeline(cfg, docs, progress_tx)
        .await
        .map_err(|e| -> Box<dyn Error> { e.to_string().into() })
}

/// Embed source code with AST-aware chunking, falling back to plain text chunking
/// when the file extension is unsupported or AST chunking produces no chunks.
pub async fn embed_code_with_metadata(
    cfg: &Config,
    content: &str,
    url: &str,
    source_type: &str,
    title: Option<&str>,
    file_extension: &str,
    extra: Option<&serde_json::Value>,
) -> Result<usize, Box<dyn Error>> {
    if content.trim().is_empty() {
        return Ok(0);
    }
    let chunks = input::code::chunk_code(content, file_extension)
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| input::chunk_text(content));
    if chunks.is_empty() {
        return Ok(0);
    }
    embed_chunks_impl(cfg, chunks, url, source_type, title, extra).await
}

fn prepare_batch_document(doc: &EmbedDocument) -> Option<PreparedBatchDocument> {
    if doc.content.trim().is_empty() {
        return None;
    }
    let chunks = match doc.file_extension.as_deref() {
        Some(ext) if !ext.is_empty() => input::code::chunk_code(&doc.content, ext)
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| input::chunk_text(&doc.content)),
        _ => input::chunk_text(&doc.content),
    };
    if chunks.is_empty() {
        return None;
    }
    let domain = spider::url::Url::parse(&doc.url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    Some(PreparedBatchDocument {
        url: doc.url.clone(),
        domain,
        source_type: doc.source_type.clone(),
        title: doc.title.clone(),
        extra: doc.extra.clone(),
        content_type: "text",
        scraped_at: chrono::Utc::now().to_rfc3339(),
        chunks,
    })
}

fn validate_batch_vectors(
    vectors_len: usize,
    chunks_embedded: usize,
) -> Result<(), Box<dyn Error>> {
    if vectors_len == 0 {
        return Err("TEI returned no vectors for batch embed".into());
    }
    if vectors_len != chunks_embedded {
        return Err(format!(
            "TEI vector count mismatch for batch embed: {} vectors for {} chunks",
            vectors_len, chunks_embedded
        )
        .into());
    }
    Ok(())
}

fn build_batch_points(
    prepared: &[PreparedBatchDocument],
    vectors: Vec<Vec<f32>>,
) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    let chunks_embedded: usize = prepared.iter().map(|doc| doc.chunks.len()).sum();
    let mut points: Vec<serde_json::Value> = Vec::with_capacity(chunks_embedded);
    let mut vectors_iter = vectors.into_iter();
    for doc in prepared {
        for (idx, chunk) in doc.chunks.iter().enumerate() {
            let vector = vectors_iter
                .next()
                .ok_or_else(|| "internal vector iterator underflow".to_string())?;
            let point_id = uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_URL,
                format!("{}:{}", doc.url, idx).as_bytes(),
            );
            let mut payload = serde_json::json!({
                "url": doc.url,
                "domain": doc.domain,
                "source_type": doc.source_type,
                "source_command": doc.source_type,
                "content_type": doc.content_type,
                "chunk_index": idx,
                "chunk_text": chunk,
                "scraped_at": doc.scraped_at,
            });
            if let Some(title) = &doc.title {
                payload["title"] = serde_json::Value::String(title.clone());
            }
            if let Some(serde_json::Value::Object(map)) = &doc.extra {
                for (k, v) in map {
                    payload[k] = v.clone();
                }
            }
            points.push(serde_json::json!({
                "id": point_id.to_string(),
                "vector": vector,
                "payload": payload,
            }));
        }
    }
    Ok(points)
}

async fn cleanup_batch_stale_tails(
    cfg: &Config,
    prepared: &[PreparedBatchDocument],
) -> Result<(), Box<dyn Error>> {
    let unique_urls: std::collections::HashMap<&str, usize> = prepared
        .iter()
        .map(|doc| (doc.url.as_str(), doc.chunks.len()))
        .collect();
    for (url, chunk_count) in unique_urls {
        qdrant_delete_stale_tail(cfg, url, chunk_count).await?;
    }
    Ok(())
}

pub async fn embed_documents_batch(
    cfg: &Config,
    docs: &[EmbedDocument],
) -> Result<EmbedSummary, Box<dyn Error>> {
    let prepared: Vec<PreparedBatchDocument> =
        docs.iter().filter_map(prepare_batch_document).collect();
    if prepared.is_empty() {
        return Ok(EmbedSummary {
            docs_embedded: 0,
            chunks_embedded: 0,
        });
    }

    let all_chunks: Vec<String> = prepared
        .iter()
        .flat_map(|doc| doc.chunks.iter().cloned())
        .collect();
    let chunks_embedded = all_chunks.len();
    let vectors = tei_embed(cfg, &all_chunks).await?;
    validate_batch_vectors(vectors.len(), chunks_embedded)?;

    let dim = vectors[0].len();
    if qdrant_store::collection_needs_init(&cfg.collection) {
        qdrant_store::ensure_collection(cfg, dim).await?;
    }

    let points = build_batch_points(&prepared, vectors)?;
    qdrant_store::qdrant_upsert(cfg, &points).await?;
    cleanup_batch_stale_tails(cfg, &prepared).await?;

    Ok(EmbedSummary {
        docs_embedded: prepared.len(),
        chunks_embedded,
    })
}

pub async fn embed_path_native(cfg: &Config, input: &str) -> Result<EmbedSummary, Box<dyn Error>> {
    embed_path_native_with_progress(cfg, input, None).await
}

pub async fn embed_path_native_with_progress(
    cfg: &Config,
    input: &str,
    progress_tx: Option<tokio::sync::mpsc::Sender<EmbedProgress>>,
) -> Result<EmbedSummary, Box<dyn Error>> {
    if cfg.tei_url.is_empty() {
        return Err("TEI_URL not configured".into());
    }
    if cfg.qdrant_url.is_empty() {
        return Err("QDRANT_URL not configured".into());
    }
    let prepared = prepare::prepare_embed_docs(input, &cfg.exclude_path_prefix).await?;
    if prepared.is_empty() {
        return prepare::emit_empty_embed(progress_tx);
    }
    let summary = pipeline::run_embed_pipeline(cfg, prepared, progress_tx)
        .await
        .map_err(|e| -> Box<dyn Error> { e.to_string().into() })?;
    prepare::emit_embed_summary(cfg, summary.chunks_embedded);
    Ok(summary)
}
