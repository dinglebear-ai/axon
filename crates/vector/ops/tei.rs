use crate::crates::core::config::Config;
use crate::crates::vector::ops::qdrant::qdrant_delete_stale_tail;
use std::error::Error;

mod code_embed;
mod pipeline;
mod prepare;
mod qdrant_store;
mod tei_client;
mod tei_manifest;
#[cfg(test)]
mod tests;
mod text_embed;

pub(crate) use tei_client::tei_embed;

// Re-export public API so callers outside this module see no change.
pub use code_embed::embed_code_with_metadata;
pub(crate) use text_embed::embed_prepared_docs;
pub use text_embed::{
    embed_path_native, embed_path_native_with_progress, embed_text_with_extra_payload,
    embed_text_with_metadata,
};

#[derive(Debug, Clone, Copy)]
pub struct EmbedSummary {
    pub docs_embedded: usize,
    pub docs_failed: usize,
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
    // Point IDs are deterministic (UUID v5 over "url:chunk_idx"), so upserting
    // the new batch automatically overwrites any chunks at the same indices.
    // After a successful upsert, delete stale tail chunks — orphan points with
    // chunk_index >= new_count that survived from a previous larger ingest.
    let new_count = points.len();
    qdrant_store::qdrant_upsert(cfg, &points).await?;
    // Stale-tail cleanup: remove old chunks for this URL with index >= new_count.
    qdrant_delete_stale_tail(cfg, url, new_count).await?;
    Ok(new_count)
}
