use axon_api::source::*;
use uuid::Uuid;

use crate::env::env_example_facts;
use crate::parser::ParseInput;

fn input(text: &str) -> ParseInput {
    ParseInput {
        job_id: JobId::new(Uuid::from_u128(31)),
        stage_id: StageId::new(Uuid::from_u128(32)),
        requested_parser: None,
        document: SourceDocument {
            document_id: DocumentId::from("doc_env"),
            source_id: SourceId::from("src_repo"),
            source_item_key: SourceItemKey::from(".env.example"),
            canonical_uri: "file:///repo/.env.example".to_string(),
            content_kind: ContentKind::PlainText,
            content: ContentRef::InlineText {
                text: text.to_string(),
            },
            metadata: MetadataMap::new(),
            title: None,
            language: None,
            path: Some(".env.example".to_string()),
            mime_type: None,
            structured_payload: None,
            artifact_id: None,
            chunk_hints: Vec::new(),
            parser_hints: Vec::new(),
        },
    }
}

#[test]
fn extracts_env_example_keys_without_secret_values() {
    let facts = env_example_facts(&input(
        "API_URL=https://example.test\nAPI_TOKEN=\n# ignored\n",
    ));

    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].fact_kind, "env_var");
    assert_eq!(facts[0].name, "API_URL");
    assert_eq!(facts[0].value["has_default"], true);
    assert_eq!(facts[1].value["has_default"], false);
    assert!(facts[1].value.get("value").is_none());
}
