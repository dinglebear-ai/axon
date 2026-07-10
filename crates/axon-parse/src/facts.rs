use axon_api::source::{ContentRef, MetadataMap, SourceParseFacts, SourceRange};
use serde_json::Value;

pub const MODULE_NAME: &str = "facts";
pub const PARSER_VERSION: &str = "pr8-baseline";

use crate::parser::ParseInput;

pub fn inline_text(input: &ParseInput) -> &str {
    match &input.document.content {
        ContentRef::InlineText { text } => text,
        _ => "",
    }
}

pub fn line_range(line: u32) -> SourceRange {
    SourceRange {
        line_start: Some(line),
        line_end: Some(line),
        byte_start: None,
        byte_end: None,
        char_start: None,
        char_end: None,
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

pub fn turn_range(line: u32, turn_id: String) -> SourceRange {
    let mut range = line_range(line);
    range.session_turn_id = Some(turn_id);
    range
}

/// A line span `[start, end]`. `end` is clamped up to `start` so the range is
/// always ordered — callers must not publish an impossible/unordered range
/// (see `validate::is_valid_range`).
pub fn span_range(start: u32, end: u32) -> SourceRange {
    let mut range = line_range(start);
    range.line_end = Some(end.max(start));
    range
}

pub fn source_fact(
    input: &ParseInput,
    parser_id: &str,
    parser_method: &str,
    fact_kind: &str,
    name: impl Into<String>,
    value: Value,
    line: Option<u32>,
) -> SourceParseFacts {
    source_fact_ranged(
        input,
        parser_id,
        parser_method,
        fact_kind,
        name,
        value,
        line.map(line_range),
    )
}

/// Same as `source_fact`, but takes a fully-formed `SourceRange` (e.g. a
/// `span_range` covering a symbol's whole body) instead of a single line.
pub fn source_fact_ranged(
    input: &ParseInput,
    parser_id: &str,
    parser_method: &str,
    fact_kind: &str,
    name: impl Into<String>,
    value: Value,
    range: Option<SourceRange>,
) -> SourceParseFacts {
    SourceParseFacts {
        document_id: input.document.document_id.clone(),
        source_item_key: input.document.source_item_key.clone(),
        fact_kind: fact_kind.to_string(),
        name: name.into(),
        value,
        parser_id: parser_id.to_string(),
        parser_version: PARSER_VERSION.to_string(),
        parser_method: parser_method.to_string(),
        range,
        confidence: confidence_for_method(parser_method),
        metadata: MetadataMap::new(),
    }
}

fn confidence_for_method(parser_method: &str) -> f32 {
    if parser_method.contains("heuristic")
        || parser_method.contains("line_scan")
        || parser_method.contains("fallback")
    {
        0.7
    } else {
        0.9
    }
}
