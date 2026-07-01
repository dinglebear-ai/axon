//! API schema chunk builders.

use crate::chunk::DocumentChunk;
use crate::metadata::structured_records;

pub fn api_schema(
    text: &str,
    structured_payload: Option<&serde_json::Value>,
) -> Vec<DocumentChunk> {
    structured_records(text, structured_payload)
        .into_iter()
        .map(|chunk| chunk.with_metadata("schema_chunk", true.into()))
        .collect()
}
