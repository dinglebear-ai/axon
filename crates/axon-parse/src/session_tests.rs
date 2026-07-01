use axon_api::source::*;
use uuid::Uuid;

use crate::parser::ParseInput;
use crate::session::{session_facts, session_items};

fn input(text: &str) -> ParseInput {
    ParseInput {
        job_id: JobId::new(Uuid::from_u128(51)),
        stage_id: StageId::new(Uuid::from_u128(52)),
        requested_parser: None,
        document: SourceDocument {
            document_id: DocumentId::from("doc_session"),
            source_id: SourceId::from("src_session"),
            source_item_key: SourceItemKey::from("session.jsonl"),
            canonical_uri: "file:///repo/session.jsonl".to_string(),
            content_kind: ContentKind::Transcript,
            content: ContentRef::InlineText {
                text: text.to_string(),
            },
            metadata: MetadataMap::new(),
            title: None,
            language: None,
            path: Some("session.jsonl".to_string()),
            mime_type: None,
            structured_payload: None,
            artifact_id: None,
            chunk_hints: Vec::new(),
            parser_hints: Vec::new(),
        },
    }
}

#[test]
fn extracts_jsonl_session_turn_facts() {
    let facts = session_facts(&input(
        r#"{"type":"message","role":"user","content":"hello"}"#,
    ));

    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].fact_kind, "session_turn");
    assert_eq!(facts[0].name, "user");
    assert_eq!(facts[0].value["type"], "message");
    assert_eq!(
        facts[0].range.as_ref().unwrap().session_turn_id,
        Some("1".to_string())
    );
}

#[test]
fn extracts_session_tool_skill_and_agent_invocations() {
    let (facts, candidates) = session_items(&input(
        r#"{"type":"message","role":"assistant","tool_calls":[{"id":"call_1","name":"axon.search"}],"skills":["axon:using-axon"],"agents_invoked":[{"name":"researcher"}]}"#,
    ));

    let kinds: Vec<_> = facts.iter().map(|fact| fact.fact_kind.as_str()).collect();
    assert_eq!(
        kinds,
        vec![
            "session_turn",
            "session_tool_call",
            "session_skill_invocation",
            "session_agent_invocation"
        ]
    );
    assert_eq!(facts[1].name, "axon.search");
    assert_eq!(facts[1].value["call_id"], "call_1");
    assert_eq!(facts[2].name, "axon:using-axon");
    assert_eq!(facts[3].name, "researcher");
    assert_eq!(
        facts[1].range.as_ref().unwrap().session_turn_id,
        Some("1".to_string())
    );

    let candidate_kinds: Vec<_> = candidates
        .iter()
        .map(|candidate| candidate.kind.as_str())
        .collect();
    assert_eq!(
        candidate_kinds,
        vec![
            "session_tool_call",
            "session_skill_invocation",
            "session_agent_invocation"
        ]
    );
}
