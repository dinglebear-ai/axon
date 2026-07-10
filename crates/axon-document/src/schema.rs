//! API schema chunk builders.

use axon_api::source::ContentKind;

use crate::chunk::DocumentChunk;
use crate::metadata::structured_records;

pub(crate) fn api_schema(
    text: &str,
    structured_payload: Option<&serde_json::Value>,
    content_kind: ContentKind,
    path: Option<&str>,
) -> Result<Vec<DocumentChunk>, String> {
    structured_records(text, structured_payload, content_kind, path).map(|chunks| {
        chunks
            .into_iter()
            .map(|chunk| chunk.with_metadata("schema_chunk", true.into()))
            .collect()
    })
}
