//! Plain text chunk builders.

use axon_api::source::SourceRange;

use crate::chunk::DocumentChunk;

pub(crate) const MAX_PLAIN_TEXT_CHUNK_BYTES: usize = 4096;
pub(crate) const MAX_PLAIN_TEXT_CHUNK_CHARS: usize = 2000;

pub fn plain_text_windows(text: &str) -> Vec<DocumentChunk> {
    let normalized = text.replace("\r\n", "\n");
    paragraphs(&normalized)
        .into_iter()
        .flat_map(|(start, end)| bounded_windows(&normalized, start, end))
        .map(|(start, end)| {
            DocumentChunk::new(
                normalized[start..end].trim().to_string(),
                source_range(&normalized, start, end),
            )
        })
        .filter(|chunk| !chunk.content.is_empty())
        .collect()
}

pub fn atomic_text(text: &str) -> Vec<DocumentChunk> {
    vec![DocumentChunk::new(
        text.to_string(),
        source_range(text, 0, text.len()),
    )]
}

pub fn source_range(text: &str, start: usize, end: usize) -> SourceRange {
    let line_start = line_number_at(text, start);
    let line_end = line_number_at(text, end.saturating_sub(1).min(text.len()));
    SourceRange {
        line_start: Some(line_start),
        line_end: Some(line_end),
        byte_start: Some(start as u64),
        byte_end: Some(end as u64),
        char_start: Some(text[..start.min(text.len())].chars().count() as u64),
        char_end: Some(text[..end.min(text.len())].chars().count() as u64),
        time_start_ms: None,
        time_end_ms: None,
        dom_selector: None,
        json_pointer: None,
        yaml_path: None,
        xml_xpath: None,
        csv_row: None,
        session_turn_id: None,
        turn_start: None,
        turn_end: None,
    }
}

fn paragraphs(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = None;
    let mut byte_start = 0usize;
    for line in text.split_inclusive('\n') {
        let byte_end = byte_start + line.len();
        if line.trim().is_empty() {
            if let Some(open) = start.take() {
                spans.push((open, byte_start));
            }
        } else if start.is_none() {
            start = Some(byte_start);
        }
        byte_start = byte_end;
    }
    if let Some(open) = start {
        spans.push((open, text.len()));
    }
    if spans.is_empty() && !text.trim().is_empty() {
        spans.push((0, text.len()));
    }
    spans
}

fn bounded_windows(text: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut chunk_start = start;
    let mut chars = 0usize;

    for (relative, ch) in text[start..end].char_indices() {
        let pos = start + relative;
        let next = pos + ch.len_utf8();
        if pos > chunk_start
            && (next - chunk_start > MAX_PLAIN_TEXT_CHUNK_BYTES
                || chars + 1 > MAX_PLAIN_TEXT_CHUNK_CHARS)
        {
            spans.push((chunk_start, pos));
            chunk_start = pos;
            chars = 0;
        }
        chars += 1;
    }

    if chunk_start < end {
        spans.push((chunk_start, end));
    }
    spans
}

fn line_number_at(text: &str, byte: usize) -> u32 {
    let capped = byte.min(text.len());
    1 + text[..capped].bytes().filter(|b| *b == b'\n').count() as u32
}

#[cfg(test)]
mod tests {
    use super::{MAX_PLAIN_TEXT_CHUNK_BYTES, MAX_PLAIN_TEXT_CHUNK_CHARS, plain_text_windows};

    #[test]
    fn plain_text_windows_splits_single_long_paragraph_into_bounded_chunks() {
        let text = "a".repeat(MAX_PLAIN_TEXT_CHUNK_BYTES * 2 + 17);

        let chunks = plain_text_windows(&text);

        assert!(chunks.len() > 2);
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.content.as_str())
                .collect::<String>(),
            text
        );
        for chunk in chunks {
            assert!(chunk.content.len() <= MAX_PLAIN_TEXT_CHUNK_BYTES);
            assert!(chunk.content.chars().count() <= MAX_PLAIN_TEXT_CHUNK_CHARS);
        }
    }
}
