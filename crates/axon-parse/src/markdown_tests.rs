use axon_api::source::*;
use uuid::Uuid;

use crate::markdown::heading_facts;
use crate::parser::ParseInput;

fn input(path: &str, text: &str) -> ParseInput {
    ParseInput {
        job_id: JobId::new(Uuid::from_u128(1)),
        stage_id: StageId::new(Uuid::from_u128(2)),
        requested_parser: None,
        document: SourceDocument {
            document_id: DocumentId::from("doc_markdown"),
            source_id: SourceId::from("src_docs"),
            source_item_key: SourceItemKey::from(path),
            canonical_uri: format!("https://docs.example.com/{path}"),
            content_kind: ContentKind::Markdown,
            content: ContentRef::InlineText {
                text: text.to_string(),
            },
            metadata: MetadataMap::new(),
            title: None,
            language: None,
            path: Some(path.to_string()),
            mime_type: Some("text/markdown".to_string()),
            structured_payload: None,
            artifact_id: None,
            chunk_hints: Vec::new(),
            parser_hints: Vec::new(),
        },
    }
}

#[test]
fn extracts_markdown_heading_facts_and_candidates() {
    let (facts, candidates) = heading_facts(&input(
        "guide.md",
        "# Axon Guide\n\nbody\n\n## Source Pipeline\n\n### Document Parse & Chunk\n",
    ));

    assert_eq!(facts.len(), 3);
    assert_eq!(candidates.len(), 3);
    assert!(
        facts
            .iter()
            .all(|fact| fact.fact_kind == "markdown_heading")
    );
    assert!(
        facts
            .iter()
            .all(|fact| fact.parser_id == "markdown_headings")
    );
    assert_eq!(facts[0].name, "Axon Guide");
    assert_eq!(facts[0].value["level"], 1);
    assert_eq!(facts[0].value["anchor"], "axon-guide");
    assert_eq!(facts[0].confidence, 0.7);
    assert_eq!(
        facts[0].value["heading_path"],
        serde_json::json!(["Axon Guide"])
    );
    assert_eq!(
        facts[2].value["heading_path"],
        serde_json::json!(["Axon Guide", "Source Pipeline", "Document Parse & Chunk"])
    );
    assert_eq!(facts[2].range.as_ref().unwrap().line_start, Some(7));

    let candidate = &candidates[1];
    assert_eq!(candidate.kind, "markdown_heading");
    assert_eq!(candidate.nodes[1].node_kind, "artifact");
    assert_eq!(candidate.edges[0].edge_kind, "source_indexed_as");
    assert_eq!(
        candidate.evidence[0].quote.as_deref(),
        Some("## Source Pipeline")
    );
}
