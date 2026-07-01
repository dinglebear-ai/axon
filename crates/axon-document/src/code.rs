//! Code-oriented chunk builders.

use crate::chunk::DocumentChunk;
use crate::text::{atomic_text, source_range};

struct LineSpan<'a> {
    start: usize,
    end: usize,
    text: &'a str,
}

pub fn code_manifest(text: &str) -> Vec<DocumentChunk> {
    atomic_text(text)
        .into_iter()
        .map(|chunk| chunk.with_metadata("manifest", true.into()))
        .collect()
}

pub fn code_symbols(text: &str) -> Vec<DocumentChunk> {
    if let Some(chunks) = repomix_packed_code_symbols(text) {
        return chunks;
    }
    code_symbols_with_base(text, text, 0)
}

fn code_symbols_with_base(section: &str, source: &str, base: usize) -> Vec<DocumentChunk> {
    let starts: Vec<usize> = section
        .lines()
        .scan(0usize, |offset, line| {
            let current = *offset;
            *offset += line.len() + 1;
            Some((current, line))
        })
        .filter(|(_, line)| looks_like_symbol(line))
        .map(|(offset, _)| offset)
        .collect();

    if starts.is_empty() {
        let content = section.trim();
        if content.is_empty() {
            return Vec::new();
        }
        return vec![DocumentChunk::new(
            content.to_string(),
            source_range(source, base, base + section.len()),
        )];
    }

    let mut boundaries = starts;
    boundaries.push(section.len());
    boundaries
        .windows(2)
        .filter_map(|pair| {
            let start = pair[0];
            let end = pair[1];
            let content = section[start..end].trim();
            if content.is_empty() {
                return None;
            }
            let symbol = content
                .lines()
                .next()
                .and_then(symbol_name)
                .unwrap_or_else(|| format!("symbol_at_{start}"));
            Some(
                DocumentChunk::new(
                    content.to_string(),
                    source_range(source, base + start, base + end),
                )
                .with_symbol(symbol),
            )
        })
        .collect()
}

fn repomix_packed_code_symbols(text: &str) -> Option<Vec<DocumentChunk>> {
    let lines = line_spans(text);
    let markers: Vec<(usize, String)> = lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            let path = line.text.trim().strip_prefix("File: ")?;
            let has_divider = idx
                .checked_sub(1)
                .and_then(|prev| lines.get(prev))
                .is_some_and(|line| is_divider(line.text))
                || lines.get(idx + 1).is_some_and(|line| is_divider(line.text));
            (has_divider && !path.trim().is_empty()).then(|| (idx, path.trim().to_string()))
        })
        .collect();

    if markers.is_empty() {
        return None;
    }

    let mut chunks = Vec::new();
    for (marker_idx, (line_idx, path)) in markers.iter().enumerate() {
        let next_marker_line = markers
            .get(marker_idx + 1)
            .map(|(idx, _)| *idx)
            .unwrap_or(lines.len());
        let mut first = line_idx + 1;
        if lines.get(first).is_some_and(|line| is_divider(line.text)) {
            first += 1;
        }
        let mut last = next_marker_line;
        while last > first
            && lines
                .get(last - 1)
                .is_some_and(|line| line.text.trim().is_empty() || is_divider(line.text))
        {
            last -= 1;
        }
        if first >= last {
            continue;
        }

        let start = lines[first].start;
        let end = lines[last - 1].end;
        chunks.extend(
            code_symbols_with_base(&text[start..end], text, start)
                .into_iter()
                .map(|chunk| chunk.with_metadata("original_path", path.clone().into())),
        );
    }

    (!chunks.is_empty()).then_some(chunks)
}

fn line_spans(text: &str) -> Vec<LineSpan<'_>> {
    let mut spans = Vec::new();
    let mut start = 0usize;
    for line in text.split_inclusive('\n') {
        let end = start + line.len();
        spans.push(LineSpan {
            start,
            end,
            text: line,
        });
        start = end;
    }
    spans
}

fn is_divider(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= 8 && trimmed.chars().all(|ch| ch == '=' || ch == '-')
}

fn looks_like_symbol(line: &str) -> bool {
    let trimmed = line.trim_start();
    [
        "fn ", "pub fn ", "def ", "class ", "struct ", "enum ", "impl ",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
}

fn symbol_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let after_keyword = [
        "pub fn ", "fn ", "def ", "class ", "struct ", "enum ", "impl ",
    ]
    .iter()
    .find_map(|prefix| trimmed.strip_prefix(prefix))?;
    Some(
        after_keyword
            .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
            .next()
            .unwrap_or("symbol")
            .to_string(),
    )
}
