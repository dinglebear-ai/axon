//! Resolved configuration for shared document preparation.
use axon_core::config::Config;
use axon_document::{DocumentPreparer, DocumentPreparerConfig};

pub(super) fn configured_document_preparer(cfg: &Config) -> DocumentPreparer {
    DocumentPreparer::new(DocumentPreparerConfig {
        max_content_bytes: axon_document::content_policy::DEFAULT_CONTENT_BYTE_LIMIT,
        markdown_max_chars: cfg.chunking_markdown_max_chars,
        markdown_min_chars: cfg.chunking_markdown_min_chars,
        markdown_overlap_chars: cfg.chunking_overlap_chars,
        minimum_chunk_chars: cfg.chunking_min_chars,
    })
}
