use axon_api::source::{DocumentId, JobId, SourceId, SourceItemKey};

use crate::vertical::{
    VERTICAL_GRAPH_CANDIDATES_METADATA_KEY, VERTICAL_PARSE_FACTS_METADATA_KEY, VerticalParseInput,
    parse_artifacts, take_metadata_artifacts,
};

#[test]
fn github_repo_vertical_derives_parse_facts_and_graph_candidates() {
    let source_id = SourceId::from("src_github");
    let document_id = DocumentId::from("doc_github");
    let source_item_key = SourceItemKey::from("https://github.com/jmagar/axon");

    let artifacts = parse_artifacts(VerticalParseInput {
        url: "https://github.com/jmagar/axon",
        title: Some("jmagar/axon"),
        extractor_name: "github_repo",
        extractor_version: 2,
        job_id: serde_json::from_str::<JobId>("\"00000000-0000-0000-0000-00000000000a\"").unwrap(),
        source_id: &source_id,
        document_id: &document_id,
        source_item_key: &source_item_key,
    });

    assert!(artifacts.facts.iter().any(|fact| {
        fact.fact_kind == "repository"
            && fact.name == "jmagar/axon"
            && fact.parser_id == "vertical_github_repo"
    }));
    assert!(artifacts.graph_candidates.iter().any(|candidate| {
        candidate.kind == "github_repo_metadata"
            && candidate
                .edges
                .iter()
                .any(|edge| edge.edge_kind == "official_for")
    }));
    assert!(
        artifacts
            .graph_candidates
            .iter()
            .all(|candidate| axon_graph::candidate::validate_candidate(candidate).is_ok())
    );
}

#[test]
fn non_parser_verticals_do_not_emit_parser_owned_artifacts() {
    let source_id = SourceId::from("src_web");
    let document_id = DocumentId::from("doc_web");
    let source_item_key = SourceItemKey::from("https://example.com/post");

    let artifacts = parse_artifacts(VerticalParseInput {
        url: "https://example.com/post",
        title: Some("Post"),
        extractor_name: "dev_to",
        extractor_version: 1,
        job_id: serde_json::from_str::<JobId>("\"00000000-0000-0000-0000-00000000000b\"").unwrap(),
        source_id: &source_id,
        document_id: &document_id,
        source_item_key: &source_item_key,
    });

    assert!(artifacts.facts.is_empty());
    assert!(artifacts.graph_candidates.is_empty());
}

#[test]
fn takes_serialized_artifacts_from_metadata_and_removes_bridge_fields() {
    let source_id = SourceId::from("src_github");
    let document_id = DocumentId::from("doc_github");
    let source_item_key = SourceItemKey::from("https://github.com/jmagar/axon");
    let artifacts = parse_artifacts(VerticalParseInput {
        url: "https://github.com/jmagar/axon",
        title: Some("jmagar/axon"),
        extractor_name: "github_repo",
        extractor_version: 2,
        job_id: serde_json::from_str::<JobId>("\"00000000-0000-0000-0000-00000000000c\"").unwrap(),
        source_id: &source_id,
        document_id: &document_id,
        source_item_key: &source_item_key,
    });
    let mut metadata = axon_api::source::MetadataMap::new();
    metadata.insert(
        VERTICAL_PARSE_FACTS_METADATA_KEY.to_string(),
        serde_json::to_value(&artifacts.facts).unwrap(),
    );
    metadata.insert(
        VERTICAL_GRAPH_CANDIDATES_METADATA_KEY.to_string(),
        serde_json::to_value(&artifacts.graph_candidates).unwrap(),
    );

    let taken = take_metadata_artifacts(&mut metadata);

    assert_eq!(taken.facts, artifacts.facts);
    assert_eq!(taken.graph_candidates, artifacts.graph_candidates);
    assert!(!metadata.contains_key(VERTICAL_PARSE_FACTS_METADATA_KEY));
    assert!(!metadata.contains_key(VERTICAL_GRAPH_CANDIDATES_METADATA_KEY));
}
