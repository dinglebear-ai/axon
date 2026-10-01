//! Plain text chunk builders.

use axon_api::source::SourceRange;

use crate::chunk::DocumentChunk;

pub(crate) const MAX_PLAIN_TEXT_CHUNK_BYTES: usize = 4096;
pub(crate) const MAX_PLAIN_TEXT_CHUNK_CHARS: usize = 2000;

pub(crate) fn plain_text_windows(text: &str) -> Vec<DocumentChunk> {
    plain_text_windows_with_limits(text, MAX_PLAIN_TEXT_CHUNK_BYTES, MAX_PLAIN_TEXT_CHUNK_CHARS)
}

pub(crate) fn plain_text_windows_with_limits(
    text: &str,
    max_bytes: usize,
    max_chars: usize,
) -> Vec<DocumentChunk> {
    // A single UTF-8 char is up to 4 bytes; anything smaller than that as a
    // byte cap could never make progress.
    let max_bytes = max_bytes.max(4);
    let max_chars = max_chars.max(1);
    let positions = SourcePositions::new(text);
    let spans = paragraphs(text);
    let mut packed = Vec::new();
    for (start, end) in spans {
        if let Some(&(previous_start, _)) = packed.last()
            && end - previous_start <= max_bytes
            && text[previous_start..end].chars().count() <= max_chars
        {
            *packed.last_mut().unwrap() = (previous_start, end);
            continue;
        }
        packed.extend(bounded_windows(text, start, end, max_bytes, max_chars));
    }
    packed
        .into_iter()
        .map(|(start, end)| {
            DocumentChunk::new(
                text[start..end].to_string(),
                positions.source_range(start, end),
            )
        })
        .collect()
}

/// Sparse character/line checkpoints bound each position lookup to 256 bytes.
pub(crate) struct SourcePositions<'a> {
    source: &'a str,
    checkpoints: Vec<(usize, u64, u32)>,
}

impl<'a> SourcePositions<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        let mut checkpoints = vec![(0, 0, 1)];
        let mut line = 1;
        for (chars, (byte, character)) in text.char_indices().enumerate() {
            if byte - checkpoints.last().unwrap().0 >= 256 {
                checkpoints.push((byte, chars as u64, line));
            }
            if character == '\n' {
                line += 1;
            }
        }
        Self {
            source: text,
            checkpoints,
        }
    }

    fn position(&self, mut byte: usize) -> (u64, u32) {
        byte = byte.min(self.source.len());
        while !self.source.is_char_boundary(byte) {
            byte -= 1;
        }
        let index = self
            .checkpoints
            .partition_point(|&(offset, _, _)| offset <= byte)
            - 1;
        let (start, chars, line) = self.checkpoints[index];
        let tail = &self.source[start..byte];
        (
            chars + tail.chars().count() as u64,
            line + tail.bytes().filter(|&b| b == b'\n').count() as u32,
        )
    }

    pub(crate) fn source_range(&self, start: usize, end: usize) -> SourceRange {
        let (char_start, line_start) = self.position(start);
        let (char_end, _) = self.position(end);
        let (_, line_end) = self.position(end.saturating_sub(1));
        source_range_from_positions(start, end, line_start, line_end, char_start, char_end)
    }
}

pub(crate) fn atomic_text(text: &str) -> Vec<DocumentChunk> {
    vec![DocumentChunk::new(
        text.to_string(),
        source_range(text, 0, text.len()),
    )]
}

pub(crate) fn source_range(text: &str, start: usize, end: usize) -> SourceRange {
    #[cfg(test)]
    crate::performance_measurement::range_scan(
        start
            .min(text.len())
            .saturating_mul(2)
            .saturating_add(end.min(text.len()))
            .saturating_add(end.saturating_sub(1).min(text.len())),
    );
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

pub(crate) fn source_range_from_positions(
    start: usize,
    end: usize,
    line_start: u32,
    line_end: u32,
    char_start: u64,
    char_end: u64,
) -> SourceRange {
    SourceRange {
        line_start: Some(line_start),
        line_end: Some(line_end),
        byte_start: Some(start as u64),
        byte_end: Some(end as u64),
        char_start: Some(char_start),
        char_end: Some(char_end),
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

fn bounded_windows(
    text: &str,
    start: usize,
    end: usize,
    max_bytes: usize,
    max_chars: usize,
) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut chunk_start = start;
    let mut chars = 0usize;

    for (relative, ch) in text[start..end].char_indices() {
        let pos = start + relative;
        let next = pos + ch.len_utf8();
        if pos > chunk_start && (next - chunk_start > max_bytes || chars + 1 > max_chars) {
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
#[path = "text_tests.rs"]
mod tests;
