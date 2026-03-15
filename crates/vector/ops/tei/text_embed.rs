use crate::crates::core::config::Config;
use std::error::Error;

use super::{EmbedProgress, EmbedSummary, PreparedDoc, prepare};

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
