//! Shared helpers for git-backed ingest providers.
//!
//! Provides the `embed_docs` thin wrapper shared by `generic_git`,
//! `gitlab/files`, and the gitea embed module.  The repo-tree BFS walk has
//! been consolidated into `src/vector/ops/file_ingest.rs` (`collect_files`),
//! which all git providers now call directly.

use anyhow::{Result, anyhow};

use crate::core::config::Config;
use crate::vector::ops::{EmbedSummary, PreparedDoc, embed_prepared_docs};

/// Thin wrapper around `embed_prepared_docs` used by all git ingest embed
/// helpers — avoids duplicating the `map_err` / `.chunks_embedded` extraction
/// in every provider embed module (Q-M7).
pub(crate) async fn embed_docs(cfg: &Config, docs: Vec<PreparedDoc>) -> Result<usize> {
    Ok(embed_doc_summary(cfg, docs).await?.chunks_embedded)
}

pub(crate) async fn embed_doc_summary(
    cfg: &Config,
    docs: Vec<PreparedDoc>,
) -> Result<EmbedSummary> {
    let summary = embed_prepared_docs(cfg, docs, None)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    Ok(summary)
}
