use axon_api::source::*;
use uuid::Uuid;

use crate::parser::ParseInput;
use crate::tool::tool_facts;

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
    let facts = tool_facts(&input(
        r#"{"tool":"axon","action":"search","status":"ok","output":{"count":2}}"#,
    ));

    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].fact_kind, "tool_output");
    assert_eq!(facts[0].name, "axon.search");
    assert_eq!(facts[0].value["status"], "ok");
    assert_eq!(facts[0].value["output_kind"], "object");
}
