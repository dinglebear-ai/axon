use std::collections::HashSet;

use axon_api::source::{PreparedDocument, SourceRange};
use axon_core::redact::forbidden_field_name;

use crate::source_range::{SourceRangeBounds, validate_source_range};

#[cfg(test)]
pub(crate) fn validate_prepared_document(document: &PreparedDocument) -> Result<(), String> {
    validate_prepared_document_inner(document, None, None)
}

pub(crate) fn validate_prepared_document_with_bounds(
    document: &PreparedDocument,
    bounds: &SourceRangeBounds,
    source_text: &str,
) -> Result<(), String> {
    validate_prepared_document_inner(document, Some(bounds), Some(source_text))
}

fn validate_prepared_document_inner(
    document: &PreparedDocument,
    bounds: Option<&SourceRangeBounds>,
    source_text: Option<&str>,
) -> Result<(), String> {
    let mut errors = Vec::new();
    let mut chunk_ids = HashSet::new();
    let mut chunk_keys = HashSet::new();

    // Reject empty required identifiers up front -- an empty id can still
    // collide/upsert unpredictably downstream even though it "looks" valid
    // to the type system (these are newtype wrappers around `String`).
    if document.document_id.0.trim().is_empty() {
        errors.push("document_id is empty".to_string());
    }
    if document.source_id.0.trim().is_empty() {
        errors.push("source_id is empty".to_string());
    }
    if document.source_item_key.0.trim().is_empty() {
        errors.push("source_item_key is empty".to_string());
    }

    for field in document.metadata.keys() {
        if forbidden_field_name(field) {
            errors.push(format!("sensitive document metadata field: {field}"));
        }
    }

    if document.chunks.is_empty() {
        errors.push("prepared document has no chunks".to_string());
    }

    for chunk in &document.chunks {
        for field in chunk.metadata.keys() {
            if forbidden_field_name(field) {
                errors.push(format!(
                    "sensitive chunk metadata field: {field} (chunk {})",
                    chunk.chunk_id.0
                ));
            }
        }
        if !chunk_ids.insert(chunk.chunk_id.clone()) {
            errors.push(format!("duplicate chunk id: {}", chunk.chunk_id.0));
        }
        if !chunk_keys.insert(chunk.chunk_key.clone()) {
            errors.push(format!("duplicate chunk key: {}", chunk.chunk_key));
        }
        if chunk.content.trim().is_empty() {
            errors.push(format!("empty content after trim: {}", chunk.chunk_id.0));
        }
        range_errors("source_range", &chunk.source_range, &mut errors);
        range_errors("locator range", &chunk.chunk_locator.range, &mut errors);
    }
    let range_result = if let Some(bounds) = bounds {
        validate_prepared_document_ranges_against_bounds(document, bounds, source_text)
    } else {
        validate_prepared_document_ranges(document)
    };
    if let Err(error) = range_result {
        errors.push(error);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn validate_prepared_document_ranges(document: &PreparedDocument) -> Result<(), String> {
    let Some(bounds) = document_bounds(document) else {
        return Ok(());
    };
    validate_prepared_document_ranges_against_bounds(document, &bounds, None)
}

pub(crate) fn validate_prepared_document_ranges_against_bounds(
    document: &PreparedDocument,
    bounds: &SourceRangeBounds,
    source_text: Option<&str>,
) -> Result<(), String> {
    for chunk in &document.chunks {
        validate_source_range(&chunk.source_range, bounds)
            .map_err(|error| format!("chunk {} source_range {error}", chunk.chunk_id.0))?;
        validate_source_range(&chunk.chunk_locator.range, bounds)
            .map_err(|error| format!("chunk {} locator range {error}", chunk.chunk_id.0))?;
        if let Some(value) = chunk.metadata.get("additional_source_ranges") {
            let ranges: Vec<SourceRange> = serde_json::from_value(value.clone())
                .map_err(|error| format!("chunk {} additional ranges {error}", chunk.chunk_id.0))?;
            for range in ranges {
                validate_source_range(&range, bounds).map_err(|error| {
                    format!("chunk {} additional range {error}", chunk.chunk_id.0)
                })?;
            }
        }
    }
    // Parse facts are internal metadata, not published to the graph or vector
    // store with their ranges. A fact range that survived parser-side
    // validation against raw content but falls outside the *normalized*
    // document bounds (e.g. a raw JSONL -> markdown transform shifting offsets,
    // as session sources do) degrades rather than failing the whole document —
    // publish-critical ranges (chunks above, graph-candidate evidence below)
    // stay fail-closed. See docs/reports/2026-07-09-...-audit.md S2-V01.
    for fact in &document.parse_facts {
        if let Some(range) = &fact.range
            && validate_source_range(range, bounds).is_err()
        {
            tracing::debug!(
                fact = %fact.name,
                "dropping out-of-normalized-bounds parse fact range (degraded)"
            );
        }
    }
    let source_lines = source_text.map(SourceLines::new);
    for candidate in &document.graph_candidates {
        validate_graph_candidate_ranges(candidate, bounds, source_lines.as_ref())?;
    }
    Ok(())
}

fn validate_graph_candidate_ranges(
    candidate: &axon_api::source::GraphCandidate,
    bounds: &SourceRangeBounds,
    source_lines: Option<&SourceLines<'_>>,
) -> Result<(), String> {
    for evidence in &candidate.evidence {
        if let Some(range) = &evidence.range {
            validate_source_range(range, bounds).map_err(|error| {
                format!(
                    "graph candidate {} evidence {} range {error}",
                    candidate.candidate_id, evidence.evidence_id
                )
            })?;
            if let (Some(source_lines), Some(quote)) = (source_lines, evidence.quote.as_deref())
                && should_validate_quote(quote)
                && let Some(contains) = source_lines.contains(range, quote)
                && !contains
            {
                return Err(format!(
                    "graph candidate {} evidence {} quote outside source range",
                    candidate.candidate_id, evidence.evidence_id
                ));
            }
        }
    }
    Ok(())
}

fn should_validate_quote(quote: &str) -> bool {
    let trimmed = quote.trim();
    !trimmed.is_empty() && !trimmed.to_ascii_lowercase().contains("redacted")
}

/// One offset per 256 line breaks. Including spare Vec capacity, index memory
/// is bounded by16 + input_bytes/16. Each lookup skips at most255 lines.
struct SourceLines<'a> {
    text: &'a str,
    checkpoints: Vec<usize>,
}
impl<'a> SourceLines<'a> {
    const STRIDE: usize = 256;
    fn new(text: &'a str) -> Self {
        let mut checkpoints = vec![0];
        let mut lines = 0;
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                lines += 1;
                if lines % Self::STRIDE == 0 {
                    checkpoints.push(offset + 1);
                }
            }
        }
        Self { text, checkpoints }
    }
    fn lines_for_range(
        &self,
        range: &SourceRange,
    ) -> Option<std::iter::Take<std::iter::Skip<std::str::Lines<'a>>>> {
        let start = range.line_start? as usize;
        let end = range.line_end.unwrap_or(start as u32) as usize;
        if start == 0 || end < start {
            return None;
        }
        let zero_based = start - 1;
        let offset = *self.checkpoints.get(zero_based / Self::STRIDE)?;
        let text: &'a str = self.text;
        Some(
            text[offset..]
                .lines()
                .skip(zero_based % Self::STRIDE)
                .take(end - start + 1),
        )
    }

    fn contains(&self, range: &SourceRange, quote: &str) -> Option<bool> {
        // Parser byte coordinates and quotes refer to literal source. Joining
        // str::lines normalizes CRLF and can reject valid multiline evidence.
        if let Some((start, end)) = range.byte_start.zip(range.byte_end) {
            let start = usize::try_from(start).ok()?;
            let end = usize::try_from(end).ok()?;
            return self.text.get(start..end).map(|slice| slice.contains(quote));
        }
        if quote.contains('\n') {
            return self.slice(range).map(|slice| slice.contains(quote));
        }
        let mut lines = self.lines_for_range(range)?;
        let first = lines.next()?;
        Some(first.contains(quote) || lines.any(|line| line.contains(quote)))
    }

    fn slice(&self, range: &SourceRange) -> Option<std::borrow::Cow<'a, str>> {
        let mut lines = self.lines_for_range(range)?;
        let first = lines.next()?;
        let Some(second) = lines.next() else {
            return Some(std::borrow::Cow::Borrowed(first));
        };
        let mut joined = first.to_owned();
        joined.push('\n');
        joined.push_str(second);
        for line in lines {
            joined.push('\n');
            joined.push_str(line);
        }
        Some(std::borrow::Cow::Owned(joined))
    }
}

#[cfg(test)]
mod line_index_tests {
    use super::*;

    #[test]
    fn byte_located_multiline_evidence_preserves_literal_crlf() {
        let source = "// é\r\nfn run() {\r\n    complete();\r\n}\r\noutside();";
        let start = source.find("fn run").unwrap();
        let end = source.find("}\r\n").unwrap() + 1;
        let range: SourceRange = serde_json::from_value(serde_json::json!({
            "byte_start": start, "byte_end": end, "line_start": 2, "line_end": 4
        }))
        .unwrap();
        let lines = SourceLines::new(source);
        assert_eq!(lines.contains(&range, &source[start..end]), Some(true));
        assert_eq!(lines.contains(&range, "outside();"), Some(false));
        assert_eq!(
            lines.contains(&range, "fn run() {\n    complete();\n}"),
            Some(false)
        );
    }

    fn legacy(text: &str, range: &SourceRange) -> Option<String> {
        let start = range.line_start? as usize;
        let end = range.line_end.unwrap_or(start as u32) as usize;
        if start == 0 || end < start {
            return None;
        }
        let lines = text
            .lines()
            .enumerate()
            .filter_map(|(index, line)| (index + 1 >= start && index + 1 <= end).then_some(line))
            .collect::<Vec<_>>();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }
    #[test]
    fn indexed_quote_ranges_match_existing_crlf_utf8_and_boundary_semantics() {
        let text = (0..1025)
            .map(|index| format!("{index}: é中{}", if index % 2 == 0 { "\r\n" } else { "\n" }))
            .collect::<String>()
            + "last\r";
        for text in ["", "\n", "a\r", "a\r\n", text.as_str()] {
            let indexed = SourceLines::new(text);
            for start in [
                0, 1, 2, 254, 255, 256, 257, 258, 511, 512, 513, 1024, 1025, 1026, 2048,
            ] {
                for width in [0, 1, 2, 255, 256, 500] {
                    let range: SourceRange = serde_json::from_value(
                        serde_json::json!({"line_start":start,"line_end":start+width}),
                    )
                    .unwrap();
                    assert_eq!(
                        indexed.slice(&range).map(|value| value.into_owned()),
                        legacy(text, &range),
                        "start={start} width={width}"
                    );
                    for quote in ["", "é", "中", "last\r", "é中\n", "outside"] {
                        assert_eq!(
                            indexed.contains(&range, quote),
                            legacy(text, &range).map(|slice| slice.contains(quote))
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn sparse_index_stays_bounded_for_the_worst_line_density() {
        let text = "\n".repeat(1024 * 1024);
        let indexed = SourceLines::new(&text);
        assert_eq!(indexed.checkpoints.len(), 4097);
        assert!(indexed.checkpoints.capacity() * size_of::<usize>() <= 16 + text.len() / 16);
        let range: SourceRange = serde_json::from_value(
            serde_json::json!({"line_start":1_000_000,"line_end":1_000_000}),
        )
        .unwrap();
        assert_eq!(indexed.slice(&range).as_deref(), Some(""));
    }
}

fn document_bounds(document: &PreparedDocument) -> Option<SourceRangeBounds> {
    Some(SourceRangeBounds {
        line_count: document.metadata.get("normalized_line_count")?.as_u64()? as u32,
        byte_len: document.metadata.get("normalized_byte_len")?.as_u64()?,
        char_count: document.metadata.get("normalized_char_count")?.as_u64()?,
    })
}

fn range_errors(label: &str, range: &SourceRange, errors: &mut Vec<String>) {
    if starts_after(range.line_start, range.line_end) {
        errors.push(format!("{label} line_start > line_end"));
    }
    if starts_after(range.byte_start, range.byte_end) {
        errors.push(format!("{label} byte_start > byte_end"));
    }
    if starts_after(range.char_start, range.char_end) {
        errors.push(format!("{label} char_start > char_end"));
    }
    if starts_after(range.time_start_ms, range.time_end_ms) {
        errors.push(format!("{label} time_start_ms > time_end_ms"));
    }
    if range
        .turn_start
        .as_ref()
        .zip(range.turn_end.as_ref())
        .is_some_and(|(start, end)| start > end)
    {
        errors.push(format!("{label} turn_start > turn_end"));
    }
}

fn starts_after<T: Ord>(start: Option<T>, end: Option<T>) -> bool {
    start.zip(end).is_some_and(|(start, end)| start > end)
}
