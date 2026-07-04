//! API schema chunk builders.

use crate::chunk::DocumentChunk;
use crate::metadata::structured_records;

pub(crate) fn api_schema(
    text: &str,
    structured_payload: Option<&serde_json::Value>,
) -> Result<Vec<DocumentChunk>, String> {
    structured_records(text, structured_payload).map(|chunks| {
        chunks
            .into_iter()
            .map(|chunk| chunk.with_metadata("schema_chunk", true.into()))
            .collect()
    })
}
