//! Transcript chunk builders.

use crate::chunk::DocumentChunk;
use crate::text::source_range;

pub(crate) fn transcript_segments(text: &str) -> Vec<DocumentChunk> {
    split_on_nonempty_lines(text, "transcript_segment")
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
