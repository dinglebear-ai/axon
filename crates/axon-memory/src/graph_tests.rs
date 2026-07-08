use std::sync::Arc;

use axon_api::source::*;
use axon_graph::SqliteGraphStore;
use axon_graph::merge::{edge_id_for, node_id_for};
use axon_graph::store::GraphStore;

use super::{GraphBackedMemoryMirror, GraphBackedMemoryStore, MemoryGraphMirror};
use crate::store::{FakeMemoryStore, MemoryStore};

async fn store() -> Arc<SqliteGraphStore> {
    Arc::new(SqliteGraphStore::connect(":memory:").await.unwrap())
}

fn remember_request(body: &str) -> MemoryRequest {
    MemoryRequest {
        memory_type: MemoryType::Fact,
        body: body.to_string(),
        confidence: 0.8,
        salience: 0.7,
        scope: MemoryScope {
            kind: "project".to_string(),
            value: "axon".to_string(),
        },
        title: Some("fact".to_string()),
        tags: Vec::new(),
        links: Vec::new(),
        decay: None,
        embed: false,
        visibility: Some(Visibility::Internal),
    }
}

fn memory_node_id(memory_id: &str) -> GraphNodeId {
    node_id_for("memory", &format!("memory:{memory_id}"))
}

fn record(id: &str, status: MemoryStatus) -> MemoryRecord {
    MemoryRecord {
        memory_id: MemoryId::new(id),
        memory_type: MemoryType::Fact,
        status,
        body: format!("body for {id}"),
        confidence: 0.8,
        salience: 0.6,
        scope: MemoryScope {
            kind: "project".to_string(),
            value: "axon".to_string(),
        },
        history: Vec::new(),
        title: Some(format!("title-{id}")),
        links: Vec::new(),
        decay: None,
        embedding_refs: Vec::new(),
        superseded_by: None,
        contradicts: None,
    }
}

#[tokio::test]
async fn upsert_memory_node_writes_a_memory_kind_node() {
    let graph = store().await;
    let mirror = GraphBackedMemoryMirror::new(graph.clone());
    let rec = record("mem_1", MemoryStatus::Active);

    mirror.upsert_memory_node(&rec).await.unwrap();

    let node = graph
        .get_node(memory_node_id("mem_1"))
        .await
        .unwrap()
        .expect("memory node exists");
    assert_eq!(node.kind, "memory");
    assert_eq!(node.metadata["memory_status"], "active");
}

#[tokio::test]
async fn supersedes_writes_memory_supersedes_edge() {
    let graph = store().await;
    let mirror = GraphBackedMemoryMirror::new(graph.clone());
    let old = record("mem_old", MemoryStatus::Superseded);
    let replacement = record("mem_new", MemoryStatus::Active);

    mirror
        .supersedes(&replacement, &old, Some("decision changed"))
        .await
        .unwrap();

    let edge_id = edge_id_for(
        "memory_supersedes",
        &memory_node_id("mem_new"),
        &memory_node_id("mem_old"),
    );
    let edge = graph
        .get_edge(edge_id)
        .await
        .unwrap()
        .expect("supersedes edge exists");
    assert_eq!(edge.kind, "memory_supersedes");
    assert_eq!(edge.evidence.len(), 1);
    assert_eq!(edge.evidence[0].evidence_kind, "user_pinned");
}

#[tokio::test]
async fn contradicts_writes_memory_contradicts_edge() {
    let graph = store().await;
    let mirror = GraphBackedMemoryMirror::new(graph.clone());
    let a = record("mem_a", MemoryStatus::Contradicted);
    let b = record("mem_b", MemoryStatus::Contradicted);

    mirror.contradicts(&a, &b, Some("conflict")).await.unwrap();

    let edge_id = edge_id_for(
        "memory_contradicts",
        &memory_node_id("mem_a"),
        &memory_node_id("mem_b"),
    );
    let edge = graph
        .get_edge(edge_id)
        .await
        .unwrap()
        .expect("contradicts edge exists");
    assert_eq!(edge.kind, "memory_contradicts");
}

#[tokio::test]
async fn derived_from_writes_one_compacts_edge_per_source() {
    let graph = store().await;
    let mirror = GraphBackedMemoryMirror::new(graph.clone());
    let compacted = record("mem_compact", MemoryStatus::Active);
    let source_a = record("mem_source_a", MemoryStatus::Archived);
    let source_b = record("mem_source_b", MemoryStatus::Archived);

    mirror
        .derived_from(&compacted, &[source_a, source_b])
        .await
        .unwrap();

    for source_id in ["mem_source_a", "mem_source_b"] {
        let edge_id = edge_id_for(
            "memory_compacts",
            &memory_node_id("mem_compact"),
            &memory_node_id(source_id),
        );
        let edge = graph
            .get_edge(edge_id)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("compacts edge for {source_id} exists"));
        assert_eq!(edge.kind, "memory_compacts");
    }
}

#[tokio::test]
async fn hide_recall_edges_marks_node_forgotten() {
    let graph = store().await;
    let mirror = GraphBackedMemoryMirror::new(graph.clone());
    let rec = record("mem_forget", MemoryStatus::Active);
    mirror.upsert_memory_node(&rec).await.unwrap();

    mirror
        .hide_recall_edges(&rec.memory_id, "user requested forget")
        .await
        .unwrap();

    let node = graph
        .get_node(memory_node_id("mem_forget"))
        .await
        .unwrap()
        .expect("node still present (history is never lost)");
    assert_eq!(node.metadata["memory_status"], "forgotten");
}

#[tokio::test]
async fn decorated_remember_mirrors_a_memory_node_into_the_graph() {
    let graph = store().await;
    let mirror = Arc::new(GraphBackedMemoryMirror::new(graph.clone()));
    let inner: Arc<dyn MemoryStore> = Arc::new(FakeMemoryStore::new());
    let store_under_test = GraphBackedMemoryStore::new(inner, mirror);

    let result = store_under_test
        .remember(remember_request("axon uses qdrant for vectors"))
        .await
        .unwrap();

    let node = graph
        .get_node(node_id_for(
            "memory",
            &format!("memory:{}", result.memory_id.0),
        ))
        .await
        .unwrap()
        .expect("memory node mirrored on remember");
    assert_eq!(node.metadata["memory_status"], "active");
}

#[tokio::test]
async fn decorated_supersede_writes_a_supersedes_edge() {
    let graph = store().await;
    let mirror = Arc::new(GraphBackedMemoryMirror::new(graph.clone()));
    let inner: Arc<dyn MemoryStore> = Arc::new(FakeMemoryStore::new());
    let store_under_test = GraphBackedMemoryStore::new(inner, mirror);

    let old = store_under_test
        .remember(remember_request("old decision"))
        .await
        .unwrap();
    let replacement = store_under_test
        .remember(remember_request("new decision"))
        .await
        .unwrap();
    store_under_test
        .supersede(MemorySupersedeRequest {
            memory_id: old.memory_id.clone(),
            replacement_id: replacement.memory_id.clone(),
            reason: Some("changed".to_string()),
            timestamp: Timestamp("2026-07-04T00:00:00Z".to_string()),
        })
        .await
        .unwrap();

    let edge_id = edge_id_for(
        "memory_supersedes",
        &node_id_for("memory", &format!("memory:{}", replacement.memory_id.0)),
        &node_id_for("memory", &format!("memory:{}", old.memory_id.0)),
    );
    let edge = graph
        .get_edge(edge_id)
        .await
        .unwrap()
        .expect("supersedes edge mirrored on supersede");
    assert_eq!(edge.kind, "memory_supersedes");
}
