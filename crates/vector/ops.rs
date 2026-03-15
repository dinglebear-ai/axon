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
pub use tei::{EmbedProgress, EmbedSummary, embed_path_native, embed_path_native_with_progress};
pub(crate) use tei::{PreparedDoc, embed_prepared_docs};
