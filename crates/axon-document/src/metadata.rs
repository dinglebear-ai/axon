//! Structured record and metadata chunk builders.

use crate::chunk::DocumentChunk;
use crate::text::{atomic_text, source_range};

pub(crate) fn structured_records(
    text: &str,
    structured_payload: Option<&serde_json::Value>,
) -> Result<Vec<DocumentChunk>, String> {
    if let Some(value) = structured_payload {
        let mut chunks = Vec::new();
        if !text.trim().is_empty() {
            chunks.extend(atomic_text(text).into_iter().map(|chunk| {
                chunk
                    .with_metadata("structured_payload_attached", true.into())
                    .with_metadata("structured_payload_source", "document".into())
            }));
        }
        chunks.extend(chunks_from_json_value(value));
        return Ok(chunks);
    }
    serde_json::from_str::<serde_json::Value>(text)
        .map(|value| chunks_from_json_value(&value))
        .map_err(|error| error.to_string())
}

pub(crate) fn atomic_metadata(text: &str) -> Vec<DocumentChunk> {
    atomic_text(text)
}

fn chunks_from_json_value(value: &serde_json::Value) -> Vec<DocumentChunk> {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let pointer = format!("/{idx}");
                DocumentChunk::new(item.to_string(), json_range(pointer.clone()))
                    .with_metadata("json_index", idx.into())
                    .with_metadata("json_pointer", pointer.into())
                    .with_metadata("synthetic_source_range", true.into())
            })
            .collect(),
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(key, item)| {
                let pointer = format!("/{}", pointer_escape(key));
                DocumentChunk::new(item.to_string(), json_range(pointer.clone()))
                    .with_metadata("json_key", key.clone().into())
                    .with_metadata("json_pointer", pointer.into())
                    .with_metadata("synthetic_source_range", true.into())
            })
            .collect(),
        _ => vec![
            DocumentChunk::new(value.to_string(), json_range("".to_string()))
                .with_metadata("json_pointer", "".into())
                .with_metadata("synthetic_source_range", true.into()),
        ],
    }
}

fn json_range(pointer: String) -> axon_api::source::SourceRange {
    let mut range = source_range("", 0, 0);
    range.json_pointer = Some(pointer);
    range
}

fn pointer_escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}
