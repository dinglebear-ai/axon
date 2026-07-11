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
    for candidate in &document.graph_candidates {
        validate_graph_candidate_ranges(candidate, bounds, source_text)?;
    }
    Ok(())
}

fn validate_graph_candidate_ranges(
    candidate: &axon_api::source::GraphCandidate,
    bounds: &SourceRangeBounds,
    source_text: Option<&str>,
) -> Result<(), String> {
    for evidence in &candidate.evidence {
        if let Some(range) = &evidence.range {
            validate_source_range(range, bounds).map_err(|error| {
                format!(
                    "graph candidate {} evidence {} range {error}",
                    candidate.candidate_id, evidence.evidence_id
                )
            })?;
            if let (Some(source_text), Some(quote)) = (source_text, evidence.quote.as_deref())
                && should_validate_quote(quote)
                && let Some(slice) = line_slice_for_range(source_text, range)
                && !slice.contains(quote)
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

fn line_slice_for_range(text: &str, range: &SourceRange) -> Option<String> {
    let start = range.line_start? as usize;
    let end = range.line_end.unwrap_or(start as u32) as usize;
    if start == 0 || end < start {
        return None;
    }
    let lines = text
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let line_no = idx + 1;
            (line_no >= start && line_no <= end).then_some(line)
        })
        .collect::<Vec<_>>();
    (!lines.is_empty()).then(|| lines.join("\n"))
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
}

fn starts_after<T: Ord>(start: Option<T>, end: Option<T>) -> bool {
    start.zip(end).is_some_and(|(start, end)| start > end)
}
