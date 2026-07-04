use axon_api::source::*;
use uuid::Uuid;

use crate::parser::ParseInput;
use crate::tool::{MAX_TOOL_JSONL_LINE_BYTES, tool_parse_items, tool_parse_result};

fn input(text: &str) -> ParseInput {
    ParseInput {
        job_id: JobId::new(Uuid::from_u128(61)),
        stage_id: StageId::new(Uuid::from_u128(62)),
        requested_parser: None,
        document: SourceDocument {
            document_id: DocumentId::from("doc_tool"),
            source_id: SourceId::from("src_session"),
            source_item_key: SourceItemKey::from("tool-output.jsonl"),
            canonical_uri: "file:///repo/tool-output.jsonl".to_string(),
            content_kind: ContentKind::Structured,
            content: ContentRef::InlineText {
                text: text.to_string(),
            },
            metadata: MetadataMap::new(),
            title: None,
            language: None,
            path: Some("tool-output.jsonl".to_string()),
            mime_type: None,
            structured_payload: None,
            artifact_id: None,
            chunk_hints: Vec::new(),
            parser_hints: Vec::new(),
        },
    }
}

fn parse_fixture(_path: &str, text: &str) -> crate::parser::ParseResult {
    tool_parse_result(&input(text))
}

fn has_fact(result: &crate::parser::ParseResult, fact_kind: &str, name: &str) -> bool {
    result
        .facts
        .iter()
        .any(|fact| fact.fact_kind == fact_kind && fact.name == name)
}

#[test]
fn tool_output_parser_defaults_to_metadata_only_and_redacts_io() {
    let result = parse_fixture(
        "tool-output.jsonl",
        r#"{"tool":"shell","action":"exec","execution_requested":true,"execution_allowed":false,"side_effect_class":"read","argv":["curl","-H","Authorization: Bearer abc","https://api.example.com"],"env":{"OPENAI_API_KEY":"sk-proj-secret","PATH":"/usr/bin"},"stdout":"token=ghp_secret","stderr":"password=secret","output":{"artifact_id":"art_1","size_bytes":70000,"reason":"oversized stdout"},"resources":[{"kind":"github_issue","uri":"https://github.com/jmagar/axon/issues/298"}]}"#,
    );

    assert!(has_fact(&result, "tool_observed_claim", "shell.exec"));
    assert!(has_fact(&result, "tool_artifact_ref", "art_1"));
    assert!(has_fact(
        &result,
        "external_resource",
        "https://github.com/jmagar/axon/issues/298"
    ));
    let serialized = serde_json::to_string(&result).expect("serialize parse result");
    assert!(!serialized.contains("sk-proj-secret"));
    assert!(!serialized.contains("Bearer abc"));
    assert!(!serialized.contains("ghp_secret"));
    assert!(!serialized.contains("password=secret"));
    assert!(
        result
            .graph_candidates
            .iter()
            .all(|candidate| axon_graph::candidate::validate_candidate(candidate).is_ok())
    );
}

#[test]
fn tool_output_parser_degrades_oversized_jsonl_before_parsing() {
    let huge = format!(
        "{{\"tool\":\"mcp\",\"output\":\"{}\"}}",
        "x".repeat(MAX_TOOL_JSONL_LINE_BYTES + 1)
    );
    let result = parse_fixture("tool-output.jsonl", &huge);

    assert_eq!(result.header.status, LifecycleStatus::CompletedDegraded);
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.code == "tool.jsonl.line_too_large")
    );
    assert!(result.facts.is_empty());
    assert!(result.graph_candidates.is_empty());
}

#[test]
fn extracts_tool_call_and_output_facts() {
    let parsed = tool_parse_items(&input(
        r#"{"tool":"axon","action":"search","status":"ok","output":{"count":2}}"#,
    ));

    let facts = parsed.facts;
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].fact_kind, "tool_observed_claim");
    assert_eq!(facts[0].name, "axon.search");
    assert_eq!(facts[0].value["status"], "ok");
    assert_eq!(facts[0].value["output_kind"], "object");
}

#[test]
fn malformed_jsonl_degrades_with_warning() {
    let result = tool_parse_result(&input(
        "{\"tool\":\"axon\",\"action\":\"search\"}\nnot-json\n",
    ));

    assert_eq!(result.header.status, LifecycleStatus::CompletedDegraded);
    assert_eq!(result.facts.len(), 1);
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(result.warnings[0].code, "parse.jsonl.invalid_line");
    assert!(result.warnings[0].message.contains("line 2"));
}

#[test]
fn reports_redacted_fields_as_facts_and_warnings() {
    let parsed = tool_parse_items(&input(
        r#"{"tool":"shell","action":"run","arguments":{"token":"[REDACTED]"},"output":"ok"}"#,
    ));

    assert_eq!(parsed.facts.len(), 2);
    assert_eq!(parsed.facts[1].fact_kind, "tool_redacted_field");
    assert_eq!(parsed.facts[1].name, "/arguments/token");
    assert_eq!(parsed.facts[1].value["tool"], "shell");
    assert_eq!(parsed.warnings.len(), 1);
    assert_eq!(parsed.warnings[0].code, "tool.redacted_field");
}

#[test]
fn extracts_artifact_reference_for_oversized_output() {
    let parsed = tool_parse_items(&input(
        r#"{"tool":"axon","action":"crawl","status":"ok","output":{"artifact_id":"artifact_123","uri":"artifact://tool-output/artifact_123","size_bytes":90000,"reason":"oversized_output"}}"#,
    ));

    assert_eq!(parsed.warnings.len(), 1);
    assert_eq!(parsed.warnings[0].code, "tool.output_artifact");

    let artifact_fact = parsed
        .facts
        .iter()
        .find(|fact| fact.fact_kind == "tool_artifact_ref")
        .expect("artifact fact");
    assert_eq!(artifact_fact.name, "artifact_123");
    assert_eq!(artifact_fact.value["tool"], "axon");
    assert_eq!(
        artifact_fact.value["uri"],
        "artifact://tool-output/artifact_123"
    );
    assert_eq!(artifact_fact.value["size_bytes"], 90000);
}
