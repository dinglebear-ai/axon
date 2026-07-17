use super::*;

use axon_api::source::{
    AdapterRef, AuthorityLevel, GraphCandidateProducer, GraphEdgeCandidate, GraphEvidence,
    GraphNodeCandidate, JobId, LifecycleStatus, SourceCounts, SourceGenerationId, SourceKind,
    SourceScope, SourceSummary, Timestamp,
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
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        artifacts: Vec::new(),
        inline: None,
    }
}

/// A minimal but registry-valid parser-produced candidate: a repo -> package
/// dependency edge with evidence, mirroring what a real manifest parser
/// (`axon-parse`'s `manifest` parser) emits.
fn dependency_candidate(source_id: &str, candidate_id: &str, parser_id: &str) -> GraphCandidate {
    GraphCandidate {
        candidate_id: candidate_id.to_string(),
        job_id: JobId::new(Uuid::from_u128(7)),
        source_id: SourceId::new(source_id),
        source_item_key: SourceItemKey::new("Cargo.toml"),
        item_canonical_uri: "file:///repo/Cargo.toml".to_string(),
        document_id: None,
        kind: "repo_package".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: "test".to_string(),
            parser: Some(parser_id.to_string()),
            version: "1".to_string(),
        },
        nodes: vec![
            GraphNodeCandidate {
                node_kind: "repo".to_string(),
                stable_key: format!("repo:{source_id}"),
                label: source_id.to_string(),
                properties: MetadataMap::new(),
            },
            GraphNodeCandidate {
                node_kind: "package".to_string(),
                stable_key: "pkg:tokio".to_string(),
                label: "tokio".to_string(),
                properties: MetadataMap::new(),
            },
        ],
        edges: vec![GraphEdgeCandidate {
            edge_kind: "repo_declares_dependency".to_string(),
            from_stable_key: format!("repo:{source_id}"),
            to_stable_key: "pkg:tokio".to_string(),
            evidence_ids: vec![format!("ev:{candidate_id}")],
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id: format!("ev:{candidate_id}"),
            evidence_kind: "dependency_manifest".to_string(),
            source_id: SourceId::new(source_id),
            source_item_key: SourceItemKey::new("Cargo.toml"),
            document_id: None,
            chunk_id: None,
            range: None,
            quote: None,
            confidence: 0.9,
            metadata: MetadataMap::new(),
        }],
        confidence: 0.9,
        metadata: MetadataMap::new(),
    }
}

/// An invalid candidate: `node_kind` is not in `axon-graph`'s closed
/// registry, so `validate_candidate` rejects it.
fn invalid_candidate(source_id: &str) -> GraphCandidate {
    let mut candidate = dependency_candidate(source_id, "gc-invalid", "manifest");
    candidate.nodes[1].node_kind = "not_a_real_kind".to_string();
    candidate
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
        graph_node_ids: Vec::new(),
        last_refreshed_at: None,
        user_label: None,
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
        Vec::new(),
    )
    .await;

    assert!(!summary.degraded);
    // 1 container + 2 documents.
    assert_eq!(summary.nodes_upserted, 3);
    assert_eq!(summary.edges_upserted, 2);
    assert_eq!(summary.evidence_records, 2);

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
        Vec::new(),
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
        Vec::new(),
    )
    .await;
    assert!(summary.degraded);
    assert_eq!(summary.nodes_upserted, 0);
}

async fn seed_web_source(ledger: &FakeLedgerStore, source_id: &str, uri: &str) {
    ledger
        .upsert_source(source_summary(source_id, uri))
        .await
        .expect("seed source");
    ledger
        .put_manifest(manifest(
            source_id,
            "gen_1",
            vec![manifest_item(source_id, "item-1", uri, ItemKind::WebPage)],
        ))
        .await
        .expect("seed manifest");
}

#[tokio::test]
async fn parser_produced_candidates_reach_the_graph_store_for_web_and_git() {
    // Web family: baseline skeleton + one real parser-produced dependency
    // candidate both land in the durable graph.
    let uri = "https://example.com/docs";
    let ledger = FakeLedgerStore::new();
    seed_web_source(&ledger, "src_web", uri).await;
    let pool = Arc::new(graph_pool().await);

    let web_candidate = dependency_candidate("src_web", "gc-web-1", "manifest");
    let summary = write_baseline_graph(
        SourceInputKind::Web,
        Some(pool.clone()),
        &ledger,
        &counts("src_web", "gen_1"),
        uri,
        vec![web_candidate.clone()],
    )
    .await;
    assert!(!summary.degraded);
    // 1 baseline container + 1 baseline document + 2 parser-produced nodes.
    assert_eq!(summary.nodes_upserted, 4);
    // 1 baseline containment edge + 1 parser-produced dependency edge.
    assert_eq!(summary.edges_upserted, 2);

    let node_kinds: Vec<String> = sqlx::query_scalar("SELECT kind FROM graph_nodes ORDER BY kind")
        .fetch_all(&*pool)
        .await
        .expect("query node kinds");
    assert!(node_kinds.iter().any(|k| k == "package"));
    assert!(node_kinds.iter().any(|k| k == "repo"));

    let edge_kinds: Vec<String> = sqlx::query_scalar("SELECT kind FROM graph_edges ORDER BY kind")
        .fetch_all(&*pool)
        .await
        .expect("query edge kinds");
    assert!(edge_kinds.iter().any(|k| k == "repo_declares_dependency"));

    // Git family: same graphing stage, a fresh source, its own dependency
    // candidate — proves the write path is family-agnostic, not web-only.
    let git_uri = "https://github.com/example/repo";
    seed_web_source(&ledger, "src_git", git_uri).await;
    let git_candidate = dependency_candidate("src_git", "gc-git-1", "manifest");
    let git_summary = write_baseline_graph(
        SourceInputKind::Git,
        Some(pool.clone()),
        &ledger,
        &counts("src_git", "gen_1"),
        git_uri,
        vec![git_candidate],
    )
    .await;
    assert!(!git_summary.degraded);
    assert_eq!(git_summary.nodes_upserted, 4);
    assert_eq!(git_summary.edges_upserted, 2);
}

#[tokio::test]
async fn invalid_graph_candidates_are_dropped_not_written() {
    let uri = "https://example.com/docs";
    let ledger = FakeLedgerStore::new();
    seed_web_source(&ledger, "src_web", uri).await;
    let pool = Arc::new(graph_pool().await);

    let summary = write_baseline_graph(
        SourceInputKind::Web,
        Some(pool.clone()),
        &ledger,
        &counts("src_web", "gen_1"),
        uri,
        vec![invalid_candidate("src_web")],
    )
    .await;

    // The invalid candidate is filtered out before the write, so only the
    // baseline skeleton (1 container + 1 document, 1 containment edge) lands
    // — the write is not degraded, and the bad candidate is not published.
    assert!(!summary.degraded);
    assert_eq!(summary.nodes_upserted, 2);
    assert_eq!(summary.edges_upserted, 1);

    let node_kinds: Vec<String> = sqlx::query_scalar("SELECT kind FROM graph_nodes")
        .fetch_all(&*pool)
        .await
        .expect("query node kinds");
    assert!(!node_kinds.iter().any(|k| k == "not_a_real_kind"));
    assert!(!node_kinds.iter().any(|k| k == "package"));
}

#[tokio::test]
async fn unchanged_item_reuse_does_not_double_write() {
    // Simulates the unchanged-refresh path: the same parser-produced
    // candidate is submitted twice for the same generation (e.g. a re-run
    // that reuses previously prepared documents). The store's upsert-by-
    // stable-key semantics must not double the node/edge counts.
    let uri = "https://example.com/docs";
    let ledger = FakeLedgerStore::new();
    seed_web_source(&ledger, "src_web", uri).await;
    let pool = Arc::new(graph_pool().await);
    let candidate = dependency_candidate("src_web", "gc-web-1", "manifest");

    let first = write_baseline_graph(
        SourceInputKind::Web,
        Some(pool.clone()),
        &ledger,
        &counts("src_web", "gen_1"),
        uri,
        vec![candidate.clone()],
    )
    .await;
    assert!(!first.degraded);

    let second = write_baseline_graph(
        SourceInputKind::Web,
        Some(pool.clone()),
        &ledger,
        &counts("src_web", "gen_1"),
        uri,
        vec![candidate],
    )
    .await;
    assert!(!second.degraded);

    let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes")
        .fetch_one(&*pool)
        .await
        .expect("count nodes");
    let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(&*pool)
        .await
        .expect("count edges");
    // Still 4 nodes / 2 edges after the second write — reuse merged into the
    // existing rows by stable key instead of duplicating them.
    assert_eq!(node_count, 4);
    assert_eq!(edge_count, 2);
}
