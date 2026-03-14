pub mod commands;
pub mod input;
pub mod qdrant;
pub mod ranking;
pub mod source_display;
pub mod sparse;
pub mod stats;
pub mod tei;

// Re-export public API — no passthrough wrappers needed.
pub use input::{chunk_text, url_lookup_candidates};
pub use stats::stats_payload;
// PreparedDoc and embed_prepared_docs are consumed by ingest callers added in subsequent tasks.
#[allow(unused_imports)]
pub(crate) use tei::PreparedDoc;
#[allow(unused_imports)]
pub(crate) use tei::embed_prepared_docs;
pub use tei::{
    EmbedDocument, EmbedProgress, EmbedSummary, embed_code_with_metadata, embed_documents_batch,
    embed_path_native, embed_path_native_with_progress, embed_text_with_extra_payload,
    embed_text_with_metadata,
};
