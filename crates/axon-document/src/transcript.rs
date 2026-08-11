//! Transcript chunk builders.

use crate::chunk::DocumentChunk;
use axon_api::source::SourceRange;

use crate::text::{source_range, source_range_from_positions};

pub(crate) fn transcript_segments(text: &str) -> Vec<DocumentChunk> {
    split_on_nonempty_lines(text, "transcript_segment")
}

pub(crate) fn tool_output_records(text: &str) -> Vec<DocumentChunk> {
    let mut chunks = nonempty_line_spans(text)
        .into_iter()
        .map(|line| tool_output_chunk(line.range, line.trimmed))
        .collect::<Vec<_>>();
    if chunks.is_empty() && !text.trim().is_empty() {
        chunks.push(DocumentChunk::new(
            text.trim().to_string(),
            source_range(text, 0, text.len()),
        ));
    }
    chunks
}

pub(crate) fn split_on_nonempty_lines(text: &str, kind: &str) -> Vec<DocumentChunk> {
    let mut chunks = nonempty_line_spans(text)
        .into_iter()
        .map(|line| {
            DocumentChunk::new(line.trimmed.to_string(), line.range)
                .with_metadata("segment_kind", kind.into())
        })
        .collect::<Vec<_>>();
    if chunks.is_empty() && !text.trim().is_empty() {
        chunks.push(DocumentChunk::new(
            text.trim().to_string(),
            source_range(text, 0, text.len()),
        ));
    }
    chunks
}

struct LineSpan<'a> {
    trimmed: &'a str,
    range: SourceRange,
}

/// Build all line ranges in one forward pass. The former implementation
/// called `source_range` for every line, and each call rescanned both document
/// prefixes for line and character offsets, making large transcripts
/// quadratic. `split_inclusive` also preserves the real CRLF byte offsets that
/// `str::lines()` plus `line.len() + 1` lost.
fn nonempty_line_spans(text: &str) -> Vec<LineSpan<'_>> {
    let mut spans = Vec::new();
    let mut byte_start = 0usize;
    let mut char_start = 0u64;
    let mut line_number = 1u32;

    for raw_line in text.split_inclusive('\n') {
        let without_lf = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line = without_lf.strip_suffix('\r').unwrap_or(without_lf);
        let byte_end = byte_start + line.len();
        let char_end = char_start + line.chars().count() as u64;
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            spans.push(LineSpan {
                trimmed,
                range: source_range_from_positions(
                    byte_start,
                    byte_end,
                    line_number,
                    line_number,
                    char_start,
                    char_end,
                ),
            });
        }
        byte_start += raw_line.len();
        char_start += raw_line.chars().count() as u64;
        line_number = line_number.saturating_add(1);
    }
    spans
}

fn tool_output_chunk(range: SourceRange, trimmed: &str) -> DocumentChunk {
    let mut chunk = DocumentChunk::new(trimmed.to_string(), range)
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

#[cfg(test)]
#[path = "transcript_tests.rs"]
mod tests;
