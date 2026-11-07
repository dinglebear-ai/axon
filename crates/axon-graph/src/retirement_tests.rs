use super::*;
fn supported_candidate() -> GraphCandidate {
    let mut value = candidate();
    value.nodes[1].node_kind = "package".into();
    value.edges[0].edge_kind = "repo_declares_dependency".into();
    value.evidence[0].evidence_kind = "dependency_manifest".into();
    value
}
use std::sync::Arc;

#[tokio::test]
async fn retirement_preserves_shared_parser_nodes_and_other_item_evidence() {
    let stores: Vec<Arc<dyn GraphStore>> = vec![
        Arc::new(FakeGraphStore::new()),
        Arc::new(crate::SqliteGraphStore::connect(":memory:").await.unwrap()),
    ];
    for store in stores {
        let first = supported_candidate();
        let mut second = first.clone();
        second.candidate_id = "second-item".into();
        second.source_item_key = SourceItemKey::new("other.toml");
        second.evidence[0].source_item_key = second.source_item_key.clone();
        second.evidence[0].evidence_id = "other-evidence".into();
        second.edges[0].evidence_ids = vec!["other-evidence".into()];
        store.upsert_candidates(vec![first, second]).await.unwrap();
        let deleted = store
            .retire_item_evidence(SourceId::new("src_a"), SourceItemKey::new("Cargo.toml"))
            .await
            .unwrap();
        assert_eq!(deleted.edges_deleted, 0);
        let nodes = store
            .nodes_for_source(SourceId::new("src_a"))
            .await
            .unwrap();
        assert_eq!(nodes.len(), 2);
        let edges = store.node_edges(nodes[0].node_id.clone()).await.unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].evidence.len(), 1);
        assert_eq!(edges[0].evidence[0].source_item_key.0, "other.toml");
        let deleted = store
            .retire_item_evidence(SourceId::new("src_a"), SourceItemKey::new("other.toml"))
            .await
            .unwrap();
        assert_eq!(deleted.edges_deleted, 1);
        assert_eq!(
            deleted.nodes_deleted, 0,
            "shared parser identities are preserved"
        );
        assert_eq!(
            store
                .nodes_for_source(SourceId::new("src_a"))
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            store
                .retire_item_evidence(SourceId::new("src_a"), SourceItemKey::new("other.toml"))
                .await
                .unwrap(),
            GraphDeleteResult::default()
        );
    }
}

#[tokio::test]
async fn retirement_recognizes_legacy_containment_and_removes_unsupported_baseline() {
    let stores: Vec<Arc<dyn GraphStore>> = vec![
        Arc::new(FakeGraphStore::new()),
        Arc::new(crate::SqliteGraphStore::connect(":memory:").await.unwrap()),
    ];
    for store in stores {
        let mut baseline = supported_candidate();
        baseline.nodes[1].stable_key = "README.md".into();
        baseline.nodes[1].node_kind = "repo_file".into();
        baseline.edges[0].to_stable_key = "README.md".into();
        baseline.evidence[0].metadata.insert(
            "contained_source_item_key".into(),
            serde_json::json!("README.md"),
        );
        store.upsert_candidates(vec![baseline]).await.unwrap();
        let deleted = store
            .retire_item_evidence(SourceId::new("src_a"), SourceItemKey::new("README.md"))
            .await
            .unwrap();
        assert_eq!(deleted.nodes_deleted, 1);
        assert_eq!(deleted.edges_deleted, 1);
        assert_eq!(
            store
                .nodes_for_source(SourceId::new("src_a"))
                .await
                .unwrap()
                .len(),
            1
        );
    }
}

#[tokio::test]
async fn containment_item_metadata_overrides_shared_candidate_key() {
    let stores: Vec<Arc<dyn GraphStore>> = vec![
        Arc::new(FakeGraphStore::new()),
        Arc::new(crate::SqliteGraphStore::connect(":memory:").await.unwrap()),
    ];
    for store in stores {
        let mut candidate = supported_candidate();
        candidate.evidence[0].metadata.insert(
            "contained_source_item_key".into(),
            serde_json::json!("other.toml"),
        );
        store.upsert_candidates(vec![candidate]).await.unwrap();
        let result = store
            .retire_item_evidence(SourceId::new("src_a"), SourceItemKey::new("Cargo.toml"))
            .await
            .unwrap();
        assert_eq!(result, GraphDeleteResult::default());
        let nodes = store
            .nodes_for_source(SourceId::new("src_a"))
            .await
            .unwrap();
        assert_eq!(
            store
                .node_edges(nodes[0].node_id.clone())
                .await
                .unwrap()
                .len(),
            1
        );
    }
}

#[tokio::test]
async fn batched_retirement_preserves_evidence_outside_the_group() {
    let stores: Vec<Arc<dyn GraphStore>> = vec![
        Arc::new(FakeGraphStore::new()),
        Arc::new(crate::SqliteGraphStore::connect(":memory:").await.unwrap()),
    ];
    for store in stores {
        let candidates = ["a", "b", "c"]
            .into_iter()
            .map(|item| {
                let mut value = supported_candidate();
                value.candidate_id = item.into();
                value.source_item_key = SourceItemKey::new(item);
                value.evidence[0].source_item_key = value.source_item_key.clone();
                value.evidence[0].evidence_id = item.into();
                value.edges[0].evidence_ids = vec![item.into()];
                value
            })
            .collect();
        store.upsert_candidates(candidates).await.unwrap();
        let result = store
            .retire_items_evidence(
                SourceId::new("src_a"),
                vec![SourceItemKey::new("a"), SourceItemKey::new("b")],
            )
            .await
            .unwrap();
        assert_eq!(result.edges_deleted, 0);
        let nodes = store
            .nodes_for_source(SourceId::new("src_a"))
            .await
            .unwrap();
        let edges = store.node_edges(nodes[0].node_id.clone()).await.unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].evidence.len(), 1);
        assert_eq!(edges[0].evidence[0].source_item_key.0, "c");
        assert_eq!(
            store
                .retire_items_evidence(SourceId::new("src_a"), vec![SourceItemKey::new("c")])
                .await
                .unwrap()
                .edges_deleted,
            1
        );
    }
}

#[tokio::test]
async fn batched_retirement_crosses_transaction_boundaries_idempotently() {
    let store = crate::SqliteGraphStore::connect(":memory:").await.unwrap();
    let items: Vec<_> = (0..65)
        .map(|i| SourceItemKey::new(format!("file-{i}")))
        .collect();
    for item in &items {
        let mut value = supported_candidate();
        value.candidate_id = item.0.clone();
        value.source_item_key = item.clone();
        value.nodes[1].stable_key = item.0.clone();
        value.nodes[1].node_kind = "repo_file".into();
        value.edges[0].to_stable_key = item.0.clone();
        value.evidence[0].evidence_id = item.0.clone();
        value.evidence[0].source_item_key = item.clone();
        value.edges[0].evidence_ids = vec![item.0.clone()];
        store.upsert_candidates(vec![value]).await.unwrap();
    }
    let result = store
        .retire_items_evidence(SourceId::new("src_a"), items.clone())
        .await
        .unwrap();
    assert_eq!(result.nodes_deleted, 65);
    assert_eq!(result.edges_deleted, 65);
    assert_eq!(
        store
            .retire_items_evidence(SourceId::new("src_a"), items)
            .await
            .unwrap(),
        GraphDeleteResult::default()
    );
}
