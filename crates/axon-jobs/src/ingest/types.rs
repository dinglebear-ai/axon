// The transport-neutral ingest-source DTOs moved to `axon_api::ingest`;
// re-exported here so existing `crate::ingest::*` call sites resolve.
pub use axon_api::ingest::{
    IngestJobConfig, IngestSource, RE_INGESTABLE_SOURCE_TYPES, source_type_label, target_label,
};
