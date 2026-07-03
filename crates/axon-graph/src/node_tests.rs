use super::*;

#[test]
fn all_node_kinds_roundtrip_through_string() {
    for kind in GraphNodeKind::ALL {
        let s = kind.as_str();
        let parsed = GraphNodeKind::from_str(s).expect("registry kind must parse");
        assert_eq!(*kind, parsed, "roundtrip failed for {s}");
    }
}

#[test]
fn node_kinds_serialize_to_registry_names() {
    // Sanity: a representative kind uses the exact registry snake_case name.
    assert_eq!(GraphNodeKind::WebOrigin.as_str(), "web_origin");
    assert_eq!(GraphNodeKind::RepoFile.as_str(), "repo_file");
    assert_eq!(GraphNodeKind::ApiOperation.as_str(), "api_operation");
    assert_eq!(GraphNodeKind::PullRequest.as_str(), "pull_request");
}

#[test]
fn unknown_node_kind_is_rejected() {
    // The contract forbids alternate names like `site`, `repository`, `file`.
    for bad in ["site", "repository", "file", "api_endpoint", "", "REPO"] {
        assert!(
            GraphNodeKind::from_str(bad).is_err(),
            "expected rejection for {bad:?}"
        );
    }
}

#[test]
fn node_kind_count_matches_registry() {
    // 55 node kinds are defined in source-graph.md "Node Kinds".
    assert_eq!(GraphNodeKind::ALL.len(), 55);
}

#[test]
fn node_kind_all_names_are_unique() {
    let mut names: Vec<&str> = GraphNodeKind::ALL.iter().map(|k| k.as_str()).collect();
    names.sort_unstable();
    let unique = names.len();
    names.dedup();
    assert_eq!(unique, names.len(), "duplicate node kind names");
}

#[test]
fn serde_deserializes_known_and_rejects_unknown() {
    let ok: GraphNodeKind = serde_json::from_str("\"repo\"").unwrap();
    assert_eq!(ok, GraphNodeKind::Repo);
    assert!(serde_json::from_str::<GraphNodeKind>("\"repository\"").is_err());
}
