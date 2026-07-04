use super::*;

use axon_api::source::{
    AdapterRef, AuthorityLevel, JobId, LifecycleStatus, SourceCounts, SourceGenerationId,
    SourceKind, SourceScope, SourceSummary, Timestamp,
};
use axon_graph::migration::ensure_schema;
use axon_ledger::store::FakeLedgerStore;
use sqlx::SqlitePool;
use uuid::Uuid;

fn counts(source_id: &str, generation: &str) -> IndexCounts {
    IndexCounts {
        job_id: JobId::new(Uuid::from_u128(7)),
        source_id: SourceId::new(source_id),
        generation: SourceGenerationId::new(generation),
        documents_prepared: 0,
        chunks_prepared: 0,
        vector_points_written: 0,
        removed: 0,
    }
}

fn manifest_item(source_id: &str, key: &str, uri: &str, item_kind: ItemKind) -> ManifestItem {
    ManifestItem {
        source_id: SourceId::new(source_id),
        source_item_key: SourceItemKey::new(key),
        canonical_uri: uri.to_string(),
        item_kind,
        content_kind: None,
        display_path: None,
        parent_key: None,
        size_bytes: None,
        content_hash: None,
        mtime: None,
        version: None,
        fetch_plan: None,
        metadata: MetadataMap::new(),
        graph_hints: Vec::new(),
    }
}

fn manifest(source_id: &str, generation: &str, items: Vec<ManifestItem>) -> SourceManifest {
    SourceManifest {
        source_id: SourceId::new(source_id),
        generation: SourceGenerationId::new(generation),
        adapter: AdapterRef {
            name: "web".to_string(),
            version: "test".to_string(),
        },
        scope: SourceScope::Site,
        items,
        created_at: Timestamp("2026-07-03T00:00:00Z".to_string()),
        metadata: MetadataMap::new(),
    }
}

fn source_summary(source_id: &str, uri: &str) -> SourceSummary {
    SourceSummary {
        source_id: SourceId::new(source_id),
        canonical_uri: uri.to_string(),
        display_name: uri.to_string(),
        source_kind: SourceKind::Web,
        adapter: AdapterRef {
            name: "web".to_string(),
            version: "test".to_string(),
        },
        authority: AuthorityLevel::Inferred,
        status: LifecycleStatus::Completed,
        counts: SourceCounts {
            items_total: 0,
            items_changed: 0,
            documents_total: 0,
            chunks_total: 0,
            vector_points_total: 0,
            bytes_total: 0,
        },
        created_at: Timestamp("2026-07-03T00:00:00Z".to_string()),
        updated_at: Timestamp("2026-07-03T00:00:00Z".to_string()),
        tags: Vec::new(),
        watch_id: None,
        last_job_id: None,
    }
}

async fn graph_pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("open graph pool");
    ensure_schema(&pool).await.expect("graph schema");
    pool
}

#[test]
fn build_candidate_emits_container_and_document_skeleton() {
    let uri = "https://en.wikipedia.org/wiki/Vector_database";
    let m = manifest(
        "src_web",
        "gen_1",
        vec![
            manifest_item("src_web", "item-1", uri, ItemKind::WebPage),
            manifest_item(
                "src_web",
                "item-2",
                "https://en.wikipedia.org/wiki/Embedding",
                ItemKind::WebPage,
            ),
        ],
    );

    let candidate = build_candidate(SourceInputKind::Web, &counts("src_web", "gen_1"), uri, &m);

    // One container node + one node per document.
    assert_eq!(candidate.nodes.len(), 3);
    assert_eq!(candidate.nodes[0].node_kind, "web_origin");
    assert!(
        candidate.nodes[1..]
            .iter()
            .all(|n| n.node_kind == "web_page")
    );
    // One containment edge per document, each with the web family edge kind.
    assert_eq!(candidate.edges.len(), 2);
    assert!(
        candidate
            .edges
            .iter()
            .all(|e| e.edge_kind == "docs_site_contains_page")
    );
    // Every edge references the single container as `from`.
    assert!(
        candidate
            .edges
            .iter()
            .all(|e| e.from_stable_key == candidate.nodes[0].stable_key)
    );
    // Evidence present so the candidate validates.
    assert_eq!(candidate.evidence.len(), 2);
}

#[test]
fn document_and_edge_kinds_are_family_specific() {
    assert_eq!(container_node_kind(SourceInputKind::Feed), "feed");
    assert_eq!(
        containment_edge_kind(SourceInputKind::Feed),
        "feed_contains_entry"
    );
    assert_eq!(
        container_node_kind(SourceInputKind::Reddit),
        "reddit_subreddit"
    );
    assert_eq!(
        containment_edge_kind(SourceInputKind::Reddit),
        "subreddit_has_thread"
    );
    let repo_item = manifest_item("s", "k", "u", ItemKind::RepoFile);
    assert_eq!(document_node_kind(&repo_item), "repo_file");
}

#[tokio::test]
async fn write_baseline_graph_persists_nonempty_graph() {
    let uri = "https://en.wikipedia.org/wiki/Vector_database";
    let ledger = FakeLedgerStore::new();
    ledger
        .upsert_source(source_summary("src_web", uri))
        .await
        .expect("seed source");
    ledger
        .put_manifest(manifest(
            "src_web",
            "gen_1",
            vec![
                manifest_item("src_web", "item-1", uri, ItemKind::WebPage),
                manifest_item(
                    "src_web",
                    "item-2",
                    "https://en.wikipedia.org/wiki/Embedding",
                    ItemKind::WebPage,
                ),
            ],
        ))
        .await
        .expect("seed manifest");

    let pool = Arc::new(graph_pool().await);
    let summary = write_baseline_graph(
        SourceInputKind::Web,
        Some(pool.clone()),
        &ledger,
        &counts("src_web", "gen_1"),
        uri,
    )
    .await;

    assert!(!summary.degraded);
    // 1 container + 2 documents.
    assert_eq!(summary.nodes_upserted, 3);
    assert_eq!(summary.edges_upserted, 2);
    assert!(summary.evidence_records >= 2);

    // The rows are genuinely in the durable graph tables.
    let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes")
        .fetch_one(&*pool)
        .await
        .expect("count nodes");
    let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(&*pool)
        .await
        .expect("count edges");
    assert_eq!(node_count, 3);
    assert_eq!(edge_count, 2);
}

#[tokio::test]
async fn write_baseline_graph_without_pool_is_degraded() {
    let ledger = FakeLedgerStore::new();
    let summary = write_baseline_graph(
        SourceInputKind::Web,
        None,
        &ledger,
        &counts("src_web", "gen_1"),
        "https://example.com",
    )
    .await;
    assert!(summary.degraded);
    assert_eq!(summary.nodes_upserted, 0);
    assert_eq!(summary.edges_upserted, 0);
}

#[tokio::test]
async fn write_baseline_graph_missing_manifest_is_degraded() {
    let ledger = FakeLedgerStore::new();
    let pool = Arc::new(graph_pool().await);
    let summary = write_baseline_graph(
        SourceInputKind::Web,
        Some(pool),
        &ledger,
        &counts("src_missing", "gen_1"),
        "https://example.com",
    )
    .await;
    assert!(summary.degraded);
    assert_eq!(summary.nodes_upserted, 0);
}
