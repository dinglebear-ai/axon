use axon_api::source::{DocumentId, JobId, SourceId, SourceItemKey};

use crate::ScrapedDoc;

#[test]
fn github_repo_vertical_derives_parse_facts_and_graph_candidates() {
    let doc = ScrapedDoc {
        url: "https://github.com/jmagar/axon".to_string(),
        markdown: "# jmagar/axon".to_string(),
        title: Some("jmagar/axon".to_string()),
        extractor_name: "github_repo",
        extractor_version: 2,
        structured: Some(serde_json::json!({ "full_name": "jmagar/axon" })),
        follow_crawl_urls: Vec::new(),
        extra: Some(serde_json::json!({
            "git_provider": "github",
            "git_owner": "jmagar",
            "git_repo": "axon"
        })),
    };

    let artifacts = doc.parse_artifacts(
        serde_json::from_str::<JobId>("\"00000000-0000-0000-0000-00000000000a\"").unwrap(),
        SourceId::from("src_github"),
        DocumentId::from("doc_github"),
        SourceItemKey::from("https://github.com/jmagar/axon"),
    );

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
