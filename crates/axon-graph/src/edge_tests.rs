use super::*;

#[test]
fn all_edge_kinds_roundtrip_through_string() {
    for kind in GraphEdgeKind::ALL {
        let s = kind.as_str();
        let parsed = GraphEdgeKind::from_str(s).expect("registry edge kind must parse");
        assert_eq!(*kind, parsed, "roundtrip failed for {s}");
    }
}

#[test]
fn edge_kinds_serialize_to_registry_names() {
    assert_eq!(GraphEdgeKind::RepoHasDocs.as_str(), "repo_has_docs");
    assert_eq!(
        GraphEdgeKind::SessionMentionsRepo.as_str(),
        "session_mentions_repo"
    );
    assert_eq!(
        GraphEdgeKind::ToolCallTouchedFile.as_str(),
        "tool_call_touched_file"
    );
    assert_eq!(GraphEdgeKind::AliasOf.as_str(), "alias_of");
}

#[test]
fn unknown_edge_kind_is_rejected() {
    for bad in ["depends_on", "contains", "links_to", "", "RepoHasDocs"] {
        assert!(
            GraphEdgeKind::from_str(bad).is_err(),
            "expected rejection for {bad:?}"
        );
    }
}

#[test]
fn edge_kind_count_matches_registry() {
    // 83 edge kinds are defined in source-graph.md "Edge Kinds".
    assert_eq!(GraphEdgeKind::ALL.len(), 83);
}

#[test]
fn edge_kind_all_names_are_unique() {
    let mut names: Vec<&str> = GraphEdgeKind::ALL.iter().map(|k| k.as_str()).collect();
    names.sort_unstable();
    let unique = names.len();
    names.dedup();
    assert_eq!(unique, names.len(), "duplicate edge kind names");
}
