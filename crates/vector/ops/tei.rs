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
    // Text paths always use prose chunking; merge into extra payload
    let merged_extra = {
        let method_val = serde_json::json!({"chunking_method": "prose"});
        match extra {
            Some(serde_json::Value::Object(map)) => {
                let mut combined = map.clone();
                combined.insert(
                    "chunking_method".to_string(),
                    serde_json::Value::String("prose".to_string()),
                );
                Some(serde_json::Value::Object(combined))
            }
            _ => Some(method_val),
        }
    };
    embed_chunks_impl(cfg, chunks, url, source_type, title, merged_extra.as_ref()).await
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
        // source_command removed — duplicated source_type
        let mut payload = serde_json::json!({
            "url": url,
            "domain": domain,
            "source_type": source_type,
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
    // chunk_code() already filters empty chunks internally
    let tree_sitter_chunks = input::code::chunk_code(content, file_extension);
    let chunking_method = if tree_sitter_chunks.is_some() {
        "tree-sitter"
    } else {
        "prose"
    };
    let chunks = tree_sitter_chunks.unwrap_or_else(|| input::chunk_text(content));
    if chunks.is_empty() {
        return Ok(0);
    }
    // Merge chunking_method into extra payload so every chunk carries it
    let merged_extra = {
        let method_val = serde_json::json!({"chunking_method": chunking_method});
        match extra {
            Some(serde_json::Value::Object(map)) => {
                let mut combined = map.clone();
                combined.insert(
                    "chunking_method".to_string(),
                    serde_json::Value::String(chunking_method.to_string()),
                );
                Some(serde_json::Value::Object(combined))
            }
            _ => Some(method_val),
        }
    };
    embed_chunks_impl(cfg, chunks, url, source_type, title, merged_extra.as_ref()).await
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
