use std::sync::Arc;

use axon_api::source::*;
use axon_graph::SqliteGraphStore;
use axon_graph::merge::{edge_id_for, node_id_for};
use axon_graph::store::GraphStore;

use super::{GraphBackedMemoryMirror, GraphBackedMemoryStore, MemoryGraphMirror};
use crate::record::SystemClock;
use crate::sqlite::SqliteMemoryStore;
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
        visibility: Visibility::Internal,
        title: Some(format!("title-{id}")),
        links: Vec::new(),
        decay: None,
        embedding_refs: Vec::new(),
        superseded_by: None,
        contradicts: None,
    }
}

struct BatchRecordingMirror {
    batch_sizes: tokio::sync::Mutex<Vec<usize>>,
    fail_batch: Option<usize>,
}

impl BatchRecordingMirror {
    fn new(fail_batch: Option<usize>) -> Self {
        Self {
            batch_sizes: tokio::sync::Mutex::new(Vec::new()),
            fail_batch,
        }
    }
}

#[async_trait::async_trait]
impl MemoryGraphMirror for BatchRecordingMirror {
    async fn upsert_memory_node(&self, _record: &MemoryRecord) -> crate::store::Result<()> {
        Ok(())
    }

    async fn upsert_memory_nodes(&self, records: &[MemoryRecord]) -> crate::store::Result<()> {
        let mut sizes = self.batch_sizes.lock().await;
        sizes.push(records.len());
        if self.fail_batch == Some(sizes.len()) {
            return Err(ApiError::new(
                "graph.fake_failure",
                axon_error::ErrorStage::Graphing,
                "forced graph transaction failure",
            ));
        }
        Ok(())
    }

    async fn supersedes(
        &self,
        _replacement: &MemoryRecord,
        _old: &MemoryRecord,
        _reason: Option<&str>,
    ) -> crate::store::Result<()> {
        Ok(())
    }

    async fn contradicts(
        &self,
        _left: &MemoryRecord,
        _right: &MemoryRecord,
        _reason: Option<&str>,
    ) -> crate::store::Result<()> {
        Ok(())
    }

    async fn derived_from(
        &self,
        _compacted: &MemoryRecord,
        _sources: &[MemoryRecord],
    ) -> crate::store::Result<()> {
        Ok(())
    }

    async fn link(
        &self,
        _record: &MemoryRecord,
        _target: &MemoryRecord,
        _link: &MemoryLink,
    ) -> crate::store::Result<()> {
        Ok(())
    }

    async fn hide_recall_edges(
        &self,
        _memory_id: &MemoryId,
        _reason: &str,
    ) -> crate::store::Result<()> {
        Ok(())
    }
}

fn import_record(id: &str) -> MemoryRecord {
    record(id, MemoryStatus::Active)
}

fn sqlite_memory_store() -> Arc<dyn MemoryStore> {
    Arc::new(SqliteMemoryStore::in_memory(Arc::new(SystemClock)).expect("memory store"))
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
async fn link_writes_edge_with_both_endpoint_nodes() {
    // Regression: link() used to emit only the source node while pointing the
    // edge's to_stable_key at the raw target id, so graph candidate validation
    // rejected it ("edge memory_relates_to references unknown to stable_key").
    let graph = store().await;
    let mirror = GraphBackedMemoryMirror::new(graph.clone());
    let source = record("mem_src", MemoryStatus::Active);
    let target = record("mem_tgt", MemoryStatus::Active);
    let link = MemoryLink {
        link_type: "relates_to".to_string(),
        target: "mem_tgt".to_string(),
        confidence: 1.0,
        evidence: Vec::new(),
    };

    mirror.link(&source, &target, &link).await.unwrap();

    graph
        .get_node(memory_node_id("mem_src"))
        .await
        .unwrap()
        .expect("source node exists");
    graph
        .get_node(memory_node_id("mem_tgt"))
        .await
        .unwrap()
        .expect("target node exists");

    let edge_id = edge_id_for(
        "memory_relates_to",
        &memory_node_id("mem_src"),
        &memory_node_id("mem_tgt"),
    );
    let edge = graph
        .get_edge(edge_id)
        .await
        .unwrap()
        .expect("relates_to edge exists");
    assert_eq!(edge.kind, "memory_relates_to");
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
async fn decorated_keyword_search_includes_graph_refs_when_requested() {
    let graph = store().await;
    let graph_store: Arc<dyn GraphStore> = graph.clone();
    let mirror = Arc::new(GraphBackedMemoryMirror::new(Arc::clone(&graph_store)));
    let inner: Arc<dyn MemoryStore> = Arc::new(FakeMemoryStore::new());
    let store_under_test = GraphBackedMemoryStore::new(inner, mirror).with_graph_store(graph_store);

    let result = store_under_test
        .remember(remember_request("keyword recall should include graph refs"))
        .await
        .unwrap();

    let hits = store_under_test
        .search(MemorySearchRequest {
            query: "keyword graph".to_string(),
            limit: 10,
            filters: MetadataMap::new(),
            include_graph: true,
            include_archived: false,
            reinforce: false,
            include_statuses: Vec::new(),
        })
        .await
        .unwrap();

    assert_eq!(hits.results.len(), 1);
    let graph = hits.graph.expect("graph refs");
    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(
        graph.nodes[0].node_id,
        node_id_for("memory", &format!("memory:{}", result.memory_id.0))
    );
    assert!(graph.warnings.is_empty());
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

#[tokio::test]
async fn import_uses_configured_graph_transaction_batch_size() {
    let mirror = Arc::new(BatchRecordingMirror::new(None));
    let store = GraphBackedMemoryStore::new(sqlite_memory_store(), mirror.clone())
        .with_graph_tx_batch_size(2);

    let result = store
        .import(MemoryImportRequest {
            records: (0..5)
                .map(|index| import_record(&format!("incoming-{index}")))
                .collect(),
            mode: MemoryImportMode::Merge,
            dry_run: false,
        })
        .await
        .unwrap();

    assert_eq!(result.created, 5);
    assert_eq!(*mirror.batch_sizes.lock().await, vec![2, 2, 1]);
    assert!(result.warnings.is_empty());
}

#[tokio::test]
async fn failed_graph_transaction_marks_only_its_chunk_for_recovery() {
    let mirror = Arc::new(BatchRecordingMirror::new(Some(2)));
    let store = GraphBackedMemoryStore::new(sqlite_memory_store(), mirror.clone())
        .with_graph_tx_batch_size(2);

    let result = store
        .import(MemoryImportRequest {
            records: (0..5)
                .map(|index| import_record(&format!("incoming-{index}")))
                .collect(),
            mode: MemoryImportMode::Merge,
            dry_run: false,
        })
        .await
        .unwrap();

    assert_eq!(*mirror.batch_sizes.lock().await, vec![2, 2, 1]);
    let recovery_warnings = result
        .warnings
        .iter()
        .filter(|warning| warning.code == "memory.graph_failed")
        .collect::<Vec<_>>();
    assert_eq!(recovery_warnings.len(), 2);
    assert!(recovery_warnings.iter().all(|warning| warning.retryable));

    for (index, memory_id) in result.created_ids.iter().enumerate() {
        let stored = store
            .get(memory_id.clone())
            .await
            .unwrap()
            .expect("imported record");
        let has_recovery_marker = stored
            .history
            .iter()
            .any(|event| event.message.contains("memory.graph_failed"));
        assert_eq!(has_recovery_marker, matches!(index, 2 | 3));
        assert_eq!(stored.status, MemoryStatus::Review);
    }
}
