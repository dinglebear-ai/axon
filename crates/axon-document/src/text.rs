//! Plain text chunk builders.

use axon_api::source::SourceRange;

use crate::chunk::DocumentChunk;

pub(crate) const MAX_PLAIN_TEXT_CHUNK_BYTES: usize = 4096;
pub(crate) const MAX_PLAIN_TEXT_CHUNK_CHARS: usize = 2000;

pub(crate) fn plain_text_windows(text: &str) -> Vec<DocumentChunk> {
    paragraphs(text)
        .into_iter()
        .flat_map(|(start, end)| bounded_windows(text, start, end))
        .map(|(start, end)| {
            DocumentChunk::new(text[start..end].to_string(), source_range(text, start, end))
        })
        .filter(|chunk| !chunk.content.is_empty())
        .collect()
}

pub(crate) fn atomic_text(text: &str) -> Vec<DocumentChunk> {
    vec![DocumentChunk::new(
        text.to_string(),
        source_range(text, 0, text.len()),
    )]
}

pub(crate) fn source_range(text: &str, start: usize, end: usize) -> SourceRange {
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
                spans.push(trim_span(text, open, byte_start));
            }
        } else if start.is_none() {
            start = Some(byte_start);
        }
        byte_start = byte_end;
    }
    if let Some(open) = start {
        spans.push(trim_span(text, open, text.len()));
    }
    if spans.is_empty() && !text.trim().is_empty() {
        spans.push(trim_span(text, 0, text.len()));
    }
    spans
        .into_iter()
        .filter(|(start, end)| start < end)
        .collect()
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
    // `byte` may land mid-UTF-8-char (e.g. non-ASCII feed/web content), which
    // would panic slicing `text[..capped]`. Back off to the nearest char
    // boundary at or below the cap; newlines are ASCII so the count is exact.
    let mut capped = byte.min(text.len());
    while capped > 0 && !text.is_char_boundary(capped) {
        capped -= 1;
    }
    1 + text[..capped].bytes().filter(|b| *b == b'\n').count() as u32
}

fn trim_span(text: &str, start: usize, end: usize) -> (usize, usize) {
    let mut trimmed_start = start;
    for (relative, ch) in text[start..end].char_indices() {
        if !ch.is_whitespace() {
            trimmed_start = start + relative;
            break;
        }
    }

    let mut trimmed_end = end;
    for (relative, ch) in text[start..end].char_indices().rev() {
        if !ch.is_whitespace() {
            trimmed_end = start + relative + ch.len_utf8();
            break;
        }
    }

    (trimmed_start, trimmed_end)
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

    #[test]
    fn plain_text_windows_preserves_original_crlf_ranges() {
        let text = " alpha\r\n\r\nbeta ";

        let chunks = plain_text_windows(text);

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].content, "alpha");
        assert_eq!(chunks[0].range.byte_start, Some(1));
        assert_eq!(chunks[0].range.byte_end, Some(6));
        assert_eq!(chunks[1].content, "beta");
        assert_eq!(chunks[1].range.byte_start, Some(10));
        assert_eq!(chunks[1].range.byte_end, Some(14));
    }
}
