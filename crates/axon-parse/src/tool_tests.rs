use axon_api::source::*;
use uuid::Uuid;

use crate::parser::ParseInput;
use crate::tool::{tool_parse_items, tool_parse_result};

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

#[test]
fn extracts_tool_call_and_output_facts() {
    let parsed = tool_parse_items(&input(
        r#"{"tool":"axon","action":"search","status":"ok","output":{"count":2}}"#,
    ));

    let facts = parsed.facts;
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].fact_kind, "tool_output");
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
