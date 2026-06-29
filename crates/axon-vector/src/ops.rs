pub mod commands;
pub mod file_ingest;
pub mod input;
pub mod qdrant;
pub mod ranking;
pub mod source_display;
mod source_doc;
pub mod sparse;
pub mod stats;
pub mod tei;
pub mod token_policy;

// Re-export public API — no passthrough wrappers needed.
pub use input::{chunk_markdown, chunk_text, url_lookup_candidates};
#[allow(unused_imports)]
pub use source_doc::{
    LedgerPayload, SourceDocument, SourceOrigin, prepare_plain_text_source,
    prepare_source_document, structured_payload_from_vertical_summary,
};
pub use stats::stats_payload;
pub use tei::{EmbedProgress, EmbedSummary, embed_path_native, embed_path_native_with_progress};
pub use tei::{PreparedDoc, embed_prepared_docs, prepare_path_native_docs};

#[cfg(test)]
#[path = "ops/source_doc_audit_tests.rs"]
mod source_doc_audit_tests;
