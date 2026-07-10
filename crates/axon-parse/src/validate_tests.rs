use axon_api::source::{
    ContentKind, ContentRef, DocumentId, GraphCandidate, JobId, MetadataMap, SourceDocument,
    SourceId, SourceItemKey, SourceParseFacts, SourceRange, StageId,
};
use uuid::Uuid;

use super::*;
use crate::facts::source_fact;
use crate::graph_candidate::graph_candidate;
use crate::parser::{ParseInput, stage_header};

fn doc() -> SourceDocument {
    SourceDocument {
        document_id: DocumentId::from("doc_1"),
        source_id: SourceId::from("src_1"),
        source_item_key: SourceItemKey::from("lib.rs"),
        canonical_uri: "file:///repo/lib.rs".to_string(),
        content_kind: ContentKind::Code,
        content: ContentRef::InlineText {
            text: "fn a() {}\n".to_string(),
        },
        metadata: MetadataMap::new(),
        title: None,
        language: None,
        path: Some("lib.rs".to_string()),
        mime_type: None,
        structured_payload: None,
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    }
}

fn input() -> ParseInput {
    ParseInput {
        job_id: JobId::new(Uuid::from_u128(1)),
        stage_id: StageId::new(Uuid::from_u128(2)),
        document: doc(),
        requested_parser: None,
    }
}

fn ok_result(
    input: &ParseInput,
    facts: Vec<SourceParseFacts>,
    candidates: Vec<GraphCandidate>,
) -> ParseResult {
    ParseResult {
        header: stage_header(input, LifecycleStatus::Completed, Vec::new(), None),
        document_id: input.document.document_id.clone(),
        facts,
        graph_candidates: candidates,
        parser_id: "test".to_string(),
        parser_version: "1".to_string(),
        warnings: Vec::new(),
        errors: Vec::new(),
    }
}

#[test]
fn range_with_no_bounds_is_valid() {
    let range = SourceRange {
        line_start: None,
        line_end: None,
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
    };
    assert!(is_valid_range(&range));
}

#[test]
fn range_with_ordered_bounds_is_valid() {
    let mut range = crate::facts::line_range(3);
    range.byte_start = Some(10);
    range.byte_end = Some(20);
    assert!(is_valid_range(&range));
}

#[test]
fn range_with_reversed_lines_is_invalid() {
    let mut range = crate::facts::line_range(3);
    range.line_start = Some(10);
    range.line_end = Some(1);
    assert!(!is_valid_range(&range));
}

#[test]
fn range_with_reversed_bytes_is_invalid() {
    let mut range = crate::facts::line_range(3);
    range.byte_start = Some(100);
    range.byte_end = Some(1);
    assert!(!is_valid_range(&range));
}

#[test]
fn sanitize_is_a_no_op_when_every_range_is_valid() {
    let input = input();
    let fact = source_fact(
        &input,
        "code_symbols",
        "line_heuristic",
        "code_symbol",
        "a",
        serde_json::json!({}),
        Some(1),
    );
    let candidate = graph_candidate(&input, "code_symbols", "code_symbol", "a", Some(1), None);
    let result = ok_result(&input, vec![fact], vec![candidate]);

    let sanitized = sanitize_result(result.clone());

    assert_eq!(sanitized.facts.len(), 1);
    assert_eq!(sanitized.graph_candidates.len(), 1);
    assert_eq!(sanitized.header.status, LifecycleStatus::Completed);
    assert!(sanitized.warnings.is_empty());
}

#[test]
fn sanitize_drops_facts_with_invalid_ranges_and_degrades_status() {
    let input = input();
    let mut fact = source_fact(
        &input,
        "code_symbols",
        "line_heuristic",
        "code_symbol",
        "a",
        serde_json::json!({}),
        Some(5),
    );
    let mut range = fact.range.clone().unwrap();
    range.line_end = Some(1);
    fact.range = Some(range);
    let result = ok_result(&input, vec![fact], Vec::new());

    let sanitized = sanitize_result(result);

    assert!(sanitized.facts.is_empty());
    assert_eq!(sanitized.header.status, LifecycleStatus::CompletedDegraded);
    assert_eq!(sanitized.warnings.len(), 1);
    assert_eq!(sanitized.warnings[0].code, INVALID_RANGE_WARNING_CODE);
    assert_eq!(sanitized.header.warnings.len(), 1);
}

#[test]
fn sanitize_drops_candidate_when_its_only_evidence_range_is_invalid() {
    let input = input();
    let mut candidate = graph_candidate(&input, "code_symbols", "code_symbol", "a", Some(5), None);
    let mut range = candidate.evidence[0].range.clone().unwrap();
    range.line_start = Some(10);
    range.line_end = Some(1);
    candidate.evidence[0].range = Some(range);
    let result = ok_result(&input, Vec::new(), vec![candidate]);

    let sanitized = sanitize_result(result);

    assert!(
        sanitized.graph_candidates.is_empty(),
        "a candidate whose only evidence has an impossible range must never publish"
    );
    assert_eq!(sanitized.header.status, LifecycleStatus::CompletedDegraded);
}

#[test]
fn sanitize_does_not_downgrade_an_already_failed_status() {
    let input = input();
    let mut fact = source_fact(
        &input,
        "code_symbols",
        "line_heuristic",
        "code_symbol",
        "a",
        serde_json::json!({}),
        Some(5),
    );
    let mut range = fact.range.clone().unwrap();
    range.line_end = Some(1);
    fact.range = Some(range);
    let mut result = ok_result(&input, vec![fact], Vec::new());
    result.header.status = LifecycleStatus::Failed;

    let sanitized = sanitize_result(result);

    assert_eq!(sanitized.header.status, LifecycleStatus::Failed);
}
