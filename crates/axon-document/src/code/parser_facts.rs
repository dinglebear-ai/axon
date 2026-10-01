use axon_api::source::SourceParseFacts;

use crate::chunk::DocumentChunk;
use crate::text::SourcePositions;

struct LineSpan {
    start: usize,
    end: usize,
}

pub(super) fn parser_code_symbol_chunks(
    text: &str,
    parse_facts: &[SourceParseFacts],
) -> Option<Vec<DocumentChunk>> {
    let positions = SourcePositions::new(text);
    let mut lines = None;
    let mut spans = parse_facts
        .iter()
        .filter(|fact| fact.fact_kind == "code_symbol")
        .filter_map(|fact| {
            let range = fact.range.as_ref()?;
            let span = byte_span(text, range).or_else(|| {
                let lines = lines.get_or_insert_with(|| line_spans(text));
                line_span_to_byte_span(lines, range.line_start?, range.line_end)
            })?;
            Some((span.0, span.1, fact))
        })
        .collect::<Vec<_>>();
    spans.sort_by(|a, b| (a.0, a.1, &a.2.name).cmp(&(b.0, b.1, &b.2.name)));
    let mut aliases = std::collections::BTreeMap::<(usize, usize), Vec<String>>::new();
    for (start, end, fact) in &spans {
        let names = aliases.entry((*start, *end)).or_default();
        if names.last() != Some(&fact.name) {
            names.push(fact.name.clone());
        }
    }
    spans.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    if spans.is_empty() {
        let outcome = parse_facts
            .iter()
            .find(|fact| fact.fact_kind == "code_parse_outcome")?;
        let observation = crate::PreparationObservation::from_value(&outcome.value);
        let status = observation.ast_status()?;
        if !matches!(status, "parsed" | "partial") || text.is_empty() {
            return None;
        }
        return Some(vec![
            DocumentChunk::new(text.to_string(), positions.source_range(0, text.len()))
                .with_metadata("code_chunk_source", "module_remainder".into())
                .with_metadata("code_parse_status", status.into())
                .with_metadata("actual_chunking_method", "tree_sitter".into())
                .with_metadata("parser_method", "tree_sitter".into())
                .with_metadata("code_syntax_recovered", (status == "partial").into())
                .with_metadata("symbol_extraction_status", "none".into()),
        ]);
    }

    let module_method = spans[0].2.parser_method.clone();
    let recovered = spans.iter().any(|(_, _, fact)| {
        fact.value
            .get("code_syntax_recovered")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    });
    let module_status = if recovered {
        "partial"
    } else {
        code_status_for_parser_method(&module_method).1
    };

    // Partition the complete source. The innermost symbol owns each interval,
    // so containers retain their headers/remainders without duplicating children.
    let mut boundaries = vec![0, text.len()];
    for (start, end, _) in &spans {
        boundaries.extend([*start, *end]);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut events = Vec::with_capacity(spans.len() * 2);
    for (index, (start, end, _)) in spans.iter().enumerate() {
        events.extend([(*start, true, index), (*end, false, index)]);
    }
    events.sort_unstable();
    let mut active = std::collections::BTreeSet::new();
    let mut event_index = 0;
    let mut chunks = Vec::new();
    for pair in boundaries.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start == end {
            continue;
        }
        while let Some(&(position, opening, index)) = events.get(event_index) {
            if position > start {
                break;
            }
            let (left, right, _) = spans[index];
            let key = (right - left, left, index);
            if opening {
                active.insert(key);
            } else {
                active.remove(&key);
            }
            event_index += 1;
        }
        let owner = active.first().map(|&(_, _, index)| spans[index]);
        let chunk = if let Some((left, right, fact)) = owner {
            parser_fact_chunk(text, fact, start, end, &positions).with_metadata(
                "code_symbol_aliases",
                serde_json::json!(aliases[&(left, right)]),
            )
        } else {
            DocumentChunk::new(
                text[start..end].to_string(),
                positions.source_range(start, end),
            )
            .with_metadata("code_chunk_source", "module_remainder".into())
            .with_metadata("code_parse_status", module_status.into())
            .with_metadata("actual_chunking_method", module_method.clone().into())
            .with_metadata("parser_method", module_method.clone().into())
            .with_metadata("code_syntax_recovered", recovered.into())
            .with_metadata("symbol_extraction_status", "none".into())
        };
        chunks.push(chunk);
    }
    Some(chunks)
}

fn parser_fact_chunk(
    text: &str,
    fact: &SourceParseFacts,
    start: usize,
    end: usize,
    positions: &SourcePositions<'_>,
) -> DocumentChunk {
    let (chunk_source, parse_status, extraction_status) =
        code_status_for_parser_method(&fact.parser_method);
    let parse_status = fact
        .value
        .get("code_parse_status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(parse_status);
    let mut chunk = DocumentChunk::new(
        text[start..end].to_string(),
        positions.source_range(start, end),
    )
    .with_symbol(fact.name.clone())
    .with_metadata("code_chunk_source", chunk_source.into())
    .with_metadata("code_parse_status", parse_status.into())
    .with_metadata("symbol_extraction_status", extraction_status.into())
    .with_metadata("actual_chunking_method", fact.parser_method.clone().into())
    .with_metadata("parser_method", fact.parser_method.clone().into())
    .with_metadata(
        "code_symbol_source_range",
        serde_json::to_value(&fact.range).unwrap_or_default(),
    );
    if let Some(kind) = fact
        .value
        .get("symbol_kind")
        .and_then(serde_json::Value::as_str)
    {
        chunk = chunk.with_metadata("code_symbol_kind", kind.into());
    }
    if let Some(recovered) = fact.value.get("code_syntax_recovered") {
        chunk = chunk.with_metadata("code_syntax_recovered", recovered.clone());
    }
    chunk
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
    lines: &[LineSpan],
    line_start: u32,
    line_end: Option<u32>,
) -> Option<(usize, usize)> {
    if line_start == 0 {
        return None;
    }
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

#[cfg(test)]
#[path = "parser_facts_tests.rs"]
mod tests;
