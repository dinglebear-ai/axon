//! Structured record and metadata chunk builders.

use crate::chunk::DocumentChunk;
use crate::text::{atomic_text, source_range};

pub fn structured_records(
    text: &str,
    structured_payload: Option<&serde_json::Value>,
) -> Vec<DocumentChunk> {
    if let Some(value) = structured_payload {
        return chunks_from_json_value(value);
    }
    serde_json::from_str::<serde_json::Value>(text)
        .map(|value| chunks_from_json_value(&value))
        .unwrap_or_else(|_| atomic_text(text))
}

pub fn atomic_metadata(text: &str) -> Vec<DocumentChunk> {
    atomic_text(text)
}

fn chunks_from_json_value(value: &serde_json::Value) -> Vec<DocumentChunk> {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                DocumentChunk::new(item.to_string(), source_range("", 0, 0))
                    .with_metadata("json_index", idx.into())
            })
            .collect(),
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(key, item)| {
                DocumentChunk::new(item.to_string(), source_range("", 0, 0))
                    .with_metadata("json_key", key.clone().into())
            })
            .collect(),
        _ => vec![DocumentChunk::new(
            value.to_string(),
            source_range("", 0, 0),
        )],
    }
}
