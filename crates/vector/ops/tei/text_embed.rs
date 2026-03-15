use crate::crates::core::config::Config;
use crate::crates::vector::ops::input;
use std::error::Error;

use super::{EmbedProgress, EmbedSummary, PreparedDoc, prepare};

/// Shared implementation for text embedding with optional extra payload fields.
pub(crate) async fn embed_text_impl(
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
    super::embed_chunks_impl(cfg, chunks, url, source_type, title, merged_extra.as_ref()).await
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
    super::pipeline::run_embed_pipeline(cfg, docs, progress_tx)
        .await
        .map_err(|e| -> Box<dyn Error> { e.to_string().into() })
}

/// Embed a file or directory from the local filesystem into Qdrant.
pub async fn embed_path_native(cfg: &Config, input: &str) -> Result<EmbedSummary, Box<dyn Error>> {
    embed_path_native_with_progress(cfg, input, None).await
}

/// Like `embed_path_native` but sends progress updates through the given channel.
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
    let summary = super::pipeline::run_embed_pipeline(cfg, prepared, progress_tx)
        .await
        .map_err(|e| -> Box<dyn Error> { e.to_string().into() })?;
    prepare::emit_embed_summary(cfg, summary.chunks_embedded);
    Ok(summary)
}
