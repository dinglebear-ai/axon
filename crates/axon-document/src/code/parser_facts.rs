use axon_api::source::SourceParseFacts;

use crate::chunk::DocumentChunk;
use crate::text::source_range;

struct LineSpan {
    start: usize,
    end: usize,
}

pub(super) fn parser_code_symbol_chunks(
    text: &str,
    parse_facts: &[SourceParseFacts],
) -> Option<Vec<DocumentChunk>> {
    let mut facts: Vec<&SourceParseFacts> = parse_facts
        .iter()
        .filter(|fact| fact.fact_kind == "code_symbol")
        .filter(|fact| fact.range.is_some())
        .collect();
    facts.sort_by_key(|fact| {
        (
            fact.range.as_ref().and_then(|range| range.line_start),
            fact.name.clone(),
        )
    });

    let chunks = facts
        .into_iter()
        .filter_map(|fact| parser_fact_chunk(text, fact))
        .collect::<Vec<_>>();
    (!chunks.is_empty()).then_some(chunks)
}

fn parser_fact_chunk(text: &str, fact: &SourceParseFacts) -> Option<DocumentChunk> {
    let range = fact.range.as_ref()?;
    let (start, end) = byte_span(text, range)
        .or_else(|| line_span_to_byte_span(text, range.line_start?, range.line_end))?;
    let content = &text[start..end];
    if content.is_empty() {
        return None;
    }
    let (chunk_source, parse_status, extraction_status) =
        code_status_for_parser_method(&fact.parser_method);
    let mut chunk = DocumentChunk::new(content.to_string(), source_range(text, start, end))
        .with_symbol(fact.name.clone())
        .with_metadata("code_chunk_source", chunk_source.into())
        .with_metadata("code_parse_status", parse_status.into())
        .with_metadata("symbol_extraction_status", extraction_status.into())
        .with_metadata("actual_chunking_method", fact.parser_method.clone().into())
        .with_metadata("parser_method", fact.parser_method.clone().into());
    if let Some(kind) = fact
        .value
        .get("symbol_kind")
        .and_then(serde_json::Value::as_str)
    {
        chunk = chunk.with_metadata("code_symbol_kind", kind.into());
    }
    Some(chunk)
}

fn byte_span(text: &str, range: &axon_api::source::SourceRange) -> Option<(usize, usize)> {
    let start = usize::try_from(range.byte_start?).ok()?;
    let end = usize::try_from(range.byte_end?).ok()?;
    (start < end && end <= text.len() && text.is_char_boundary(start) && text.is_char_boundary(end))
        .then_some((start, end))
}

fn code_status_for_parser_method(
    parser_method: &str,
) -> (&'static str, &'static str, &'static str) {
    let method = parser_method.to_ascii_lowercase();
    if method.contains("unsupported") {
        ("line_window", "unsupported", "unsupported")
    } else if method.contains("fallback")
        || method.contains("heuristic")
        || method.contains("regex")
        || method.contains("line_scan")
    {
        ("heuristic_symbol", "fallback", "fallback")
    } else {
        ("ast_symbol", "parsed", "parsed")
    }
}

fn line_span_to_byte_span(
    text: &str,
    line_start: u32,
    line_end: Option<u32>,
) -> Option<(usize, usize)> {
    if line_start == 0 {
        return None;
    }
    let lines = line_spans(text);
    let start_idx = line_start.checked_sub(1)? as usize;
    let end_idx = line_end
        .unwrap_or(line_start)
        .max(line_start)
        .checked_sub(1)? as usize;
    let start = lines.get(start_idx)?.start;
    let end = lines.get(end_idx)?.end;
    (start < end).then_some((start, end))
}

fn line_spans(text: &str) -> Vec<LineSpan> {
    let mut spans = Vec::new();
    let mut start = 0usize;
    for line in text.split_inclusive('\n') {
        let end = start + line.len();
        spans.push(LineSpan { start, end });
        start = end;
    }
    spans
}
