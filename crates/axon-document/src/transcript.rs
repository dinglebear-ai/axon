//! Transcript chunk builders.

use crate::chunk::DocumentChunk;
use crate::text::source_range;

pub(crate) fn transcript_segments(text: &str) -> Vec<DocumentChunk> {
    split_on_nonempty_lines(text, "transcript_segment")
}

pub(crate) fn tool_output_records(text: &str) -> Vec<DocumentChunk> {
    let mut chunks = Vec::new();
    let mut offset = 0usize;
    for line in text.lines() {
        let end = offset + line.len();
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            chunks.push(tool_output_chunk(text, offset, end, trimmed));
        }
        offset = end + 1;
    }
    if chunks.is_empty() && !text.trim().is_empty() {
        chunks.push(DocumentChunk::new(
            text.trim().to_string(),
            source_range(text, 0, text.len()),
        ));
    }
    chunks
}

pub(crate) fn split_on_nonempty_lines(text: &str, kind: &str) -> Vec<DocumentChunk> {
    let mut chunks = Vec::new();
    let mut offset = 0usize;
    for line in text.lines() {
        let end = offset + line.len();
        if !line.trim().is_empty() {
            chunks.push(
                DocumentChunk::new(line.trim().to_string(), source_range(text, offset, end))
                    .with_metadata("segment_kind", kind.into()),
            );
        }
        offset = end + 1;
    }
    if chunks.is_empty() && !text.trim().is_empty() {
        chunks.push(DocumentChunk::new(
            text.trim().to_string(),
            source_range(text, 0, text.len()),
        ));
    }
    chunks
}

fn tool_output_chunk(source: &str, start: usize, end: usize, trimmed: &str) -> DocumentChunk {
    let mut chunk = DocumentChunk::new(trimmed.to_string(), source_range(source, start, end))
        .with_metadata("segment_kind", "tool_output".into());
    let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return chunk;
    };
    if let Some(tool_name) = string_field(&value, &["tool", "tool_name"]) {
        chunk = chunk.with_metadata("tool_name", tool_name.into());
    }
    if let Some(action) = string_field(&value, &["action", "name"]) {
        chunk = chunk.with_metadata("tool_action", action.into());
    }
    if let Some(side_effect) = string_field(&value, &["side_effect_class"]) {
        chunk = chunk.with_metadata("tool_side_effect_class", side_effect.into());
    }
    if let Some(artifact_id) = output_artifact_id(&value) {
        chunk = chunk.with_metadata("tool_output_artifact_id", artifact_id.into());
    }
    chunk
}

fn string_field<'a>(value: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(serde_json::Value::as_str))
        .filter(|value| !value.trim().is_empty())
}

fn output_artifact_id(value: &serde_json::Value) -> Option<&str> {
    string_field(
        value,
        &[
            "tool_output_artifact_id",
            "output_artifact_id",
            "artifact_id",
        ],
    )
    .or_else(|| {
        value
            .get("output")
            .and_then(|output| string_field(output, &["artifact_id", "output_artifact_id"]))
    })
}
