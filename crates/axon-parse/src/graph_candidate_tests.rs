use axon_api::source::{
    ContentKind, ContentRef, DocumentId, JobId, MetadataMap, SourceDocument, SourceId,
    SourceItemKey, StageId,
};

use crate::graph_candidate::graph_candidate;
use crate::parser::ParseInput;

#[test]
fn parser_graph_candidate_uses_closed_source_graph_registry() {
    let input = input("src-a", "Dockerfile", "file:///repo/Dockerfile");

    let candidate = crate::graph_candidate::candidate_edge(
        &input,
        "docker_manifest",
        "container_manifest",
        "local_checkout",
        "local://repo",
        "container_image",
        "docker:library/postgres",
        "repo_uses_container_image",
        "container_manifest",
        Some(1),
        Some("FROM postgres:16".to_string()),
    );

    axon_graph::candidate::validate_candidate(&candidate).expect("candidate is contract-valid");
}

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
    axon_graph::candidate::validate_candidate(&left).expect("legacy helper is contract-valid");
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

#[test]
fn encoded_identity_preserves_long_unicode_controls_and_full_evidence() {
    let source = input(
        "src-a",
        "tab\tfile",
        &format!("file:///{}", "深".repeat(300)),
    );
    for name in [
        "NL_DEBUG\t(name)\nsecond".to_string(),
        "深い関数".repeat(300),
    ] {
        let candidate = graph_candidate(
            &source,
            "test",
            "code_symbol",
            &name,
            Some(1),
            Some(name.clone()),
        );
        axon_graph::candidate::validate_candidate(&candidate).unwrap();
        let key = candidate.merge_key.as_ref().unwrap();
        assert!(key.len() <= 512);
        assert!(!key.chars().any(char::is_control));
        assert_eq!(candidate.nodes[1].label, name);
        assert_eq!(candidate.evidence[0].quote.as_deref(), Some(name.as_str()));
        assert_eq!(
            candidate,
            graph_candidate(
                &source,
                "test",
                "code_symbol",
                &name,
                Some(1),
                Some(name.clone())
            )
        );
    }
}

#[test]
fn identity_uses_unambiguous_fields_and_source_scope_for_merge_and_evidence() {
    let left = input("a|item=b", "c", "file:///same");
    let right = input("a", "b|item=c", "file:///same");
    let make =
        |input: &ParseInput| graph_candidate(input, "test", "code_symbol", "run", Some(1), None);
    let a = make(&left);
    let b = make(&right);
    assert_ne!(a.candidate_id, b.candidate_id);
    assert_ne!(a.merge_key, b.merge_key);
    assert_ne!(a.evidence[0].evidence_id, b.evidence[0].evidence_id);
    let mut other = left.clone();
    other.document.source_id = SourceId::from("other");
    assert_ne!(a.merge_key, make(&other).merge_key);
    other = left.clone();
    other.job_id = JobId(uuid::Uuid::from_u128(42));
    assert_eq!(a.merge_key, make(&other).merge_key);
}

#[test]
fn relationship_identity_includes_both_endpoints_and_provenance() {
    let make = |input: &ParseInput, from: &str, line: u32| {
        crate::graph_candidate::candidate_edge(
            input,
            "docker_manifest",
            "container_manifest",
            "local_checkout",
            from,
            "container_image",
            "docker:postgres",
            "repo_uses_container_image",
            "container_manifest",
            Some(line),
            Some("FROM postgres".to_string()),
        )
    };
    let source = input("src-a", "Dockerfile", "file:///same");
    let a = make(&source, "local:a", 1);
    let moved = make(&source, "local:a", 9);
    assert_eq!(a.candidate_id, moved.candidate_id);
    assert_eq!(a.merge_key, moved.merge_key);
    assert_ne!(a.evidence[0].evidence_id, moved.evidence[0].evidence_id);
    let other_from = make(&source, "local:b", 1);
    assert_ne!(a.candidate_id, other_from.candidate_id);
    let other = make(&input("src-b", "Dockerfile", "file:///same"), "local:a", 1);
    assert_ne!(a.candidate_id, other.candidate_id);
    assert_ne!(a.merge_key, other.merge_key);
    assert_ne!(a.evidence[0].evidence_id, other.evidence[0].evidence_id);
    for candidate in [a, moved, other_from, other] {
        axon_graph::candidate::validate_candidate(&candidate).unwrap();
    }
}
