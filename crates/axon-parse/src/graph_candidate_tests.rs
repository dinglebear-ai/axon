use axon_api::source::{
    ContentKind, ContentRef, DocumentId, JobId, MetadataMap, SourceDocument, SourceId,
    SourceItemKey, StageId,
};

use crate::graph_candidate::graph_candidate;
use crate::parser::ParseInput;

#[test]
fn graph_candidate_keys_are_source_scoped_and_collision_resistant() {
    let left = graph_candidate(
        &input("src-a", "foo-bar", "file:///repo-a/foo.rs"),
        "test-parser",
        "code_symbol",
        "foo/bar",
        Some(7),
        None,
    );
    let right = graph_candidate(
        &input("src-a", "foo_bar", "file:///repo-a/foo.rs"),
        "test-parser",
        "code_symbol",
        "foo_bar",
        Some(7),
        None,
    );
    let other_source = graph_candidate(
        &input("src-b", "foo-bar", "file:///repo-b/foo.rs"),
        "test-parser",
        "code_symbol",
        "foo/bar",
        Some(7),
        None,
    );

    assert_ne!(left.candidate_id, right.candidate_id);
    assert_ne!(left.nodes[1].stable_key, right.nodes[1].stable_key);
    assert_ne!(left.candidate_id, other_source.candidate_id);
    assert_ne!(left.nodes[0].stable_key, other_source.nodes[0].stable_key);
}

#[test]
fn graph_node_identity_is_stable_when_entity_moves_lines() {
    let first = graph_candidate(
        &input("src-a", "foo.rs", "file:///repo-a/foo.rs"),
        "test-parser",
        "code_symbol",
        "run",
        Some(7),
        Some("fn run() {}".to_string()),
    );
    let moved = graph_candidate(
        &input("src-a", "foo.rs", "file:///repo-a/foo.rs"),
        "test-parser",
        "code_symbol",
        "run",
        Some(20),
        Some("fn run() {}".to_string()),
    );

    assert_eq!(first.candidate_id, moved.candidate_id);
    assert_eq!(first.nodes[1].stable_key, moved.nodes[1].stable_key);
    assert_ne!(first.evidence[0].evidence_id, moved.evidence[0].evidence_id);
}

fn input(source_id: &str, item_key: &str, uri: &str) -> ParseInput {
    ParseInput {
        job_id: JobId(uuid::Uuid::nil()),
        stage_id: StageId(uuid::Uuid::nil()),
        requested_parser: None,
        document: SourceDocument {
            document_id: DocumentId::from(format!("doc-{item_key}")),
            source_id: SourceId::from(source_id),
            source_item_key: SourceItemKey::from(item_key),
            canonical_uri: uri.to_string(),
            content_kind: ContentKind::Code,
            content: ContentRef::InlineText {
                text: "fn foo() {}".to_string(),
            },
            metadata: MetadataMap::new(),
            title: None,
            language: Some("rust".to_string()),
            path: Some("foo.rs".to_string()),
            mime_type: None,
            structured_payload: None,
            artifact_id: None,
            chunk_hints: Vec::new(),
            parser_hints: Vec::new(),
        },
    }
}
