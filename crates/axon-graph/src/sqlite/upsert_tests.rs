use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

#[tokio::test]
#[ignore = "manual disk-backed graph transaction benchmark"]
async fn disk_backed_candidate_transaction_benchmark() {
    for mode in ["legacy", "bounded_api", "stream"] {
        let directory =
            std::env::temp_dir().join(format!("axon-graph-bench-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let pool = axon_core::sqlite::open_pool(directory.join("graph.db").to_str().unwrap())
            .await
            .unwrap();
        crate::migration::ensure_schema(&pool).await.unwrap();
        let gate = axon_core::sqlite::SqliteWriteGate::default();
        let candidates = (0..8192)
            .map(|i| candidate(&format!("candidate-{i}"), &format!("repo:{i}")))
            .collect::<Vec<_>>();
        // Seed both databases identically; benchmark a repeated publication.
        upsert_candidates(&pool, &gate, candidates.clone())
            .await
            .unwrap();
        let start = std::time::Instant::now();
        let result = if mode == "bounded_api" {
            upsert_candidates(&pool, &gate, candidates).await
        } else if mode == "stream" {
            upsert_candidate_iter(&pool, &gate, candidates).await
        } else {
            async {
                let mut total = CandidateCounts::default();
                for value in candidates {
                    let (nodes, edges) = resolve_candidate(&value);
                    total
                        .add(write_resolved_candidate(&pool, &gate, &value, &nodes, &edges).await?);
                }
                Ok::<_, axon_api::source::ApiError>(
                    total.result(Some(SourceId::new("bounded-source"))),
                )
            }
            .await
        }
        .unwrap();
        println!(
            "graph_benchmark mode={mode} candidates={} elapsed_ms={}",
            result.candidates_seen,
            start.elapsed().as_millis()
        );
        assert_eq!(result.nodes_upserted, 8192);
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 8192);
        pool.close().await;
        std::fs::remove_dir_all(directory).unwrap();
    }
}

use axon_api::source::{GraphCandidateProducer, GraphNodeCandidate, JobId, SourceItemKey};

fn candidate(id: &str, stable_key: &str) -> GraphCandidate {
    GraphCandidate {
        candidate_id: id.to_string(),
        job_id: JobId::new(uuid::Uuid::new_v4()),
        source_id: SourceId::new("bounded-source"),
        source_item_key: SourceItemKey::new(stable_key),
        item_canonical_uri: format!("https://example.test/{stable_key}"),
        document_id: None,
        kind: "repository_snapshot".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: "git".to_string(),
            parser: None,
            version: "test".to_string(),
        },
        nodes: vec![GraphNodeCandidate {
            node_kind: "repo".to_string(),
            stable_key: stable_key.to_string(),
            label: stable_key.to_string(),
            properties: MetadataMap::new(),
        }],
        edges: Vec::new(),
        evidence: Vec::new(),
        confidence: 0.9,
        metadata: MetadataMap::new(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oversized_candidate_commits_before_the_stream_requests_another() {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    crate::migration::ensure_schema(&pool).await.unwrap();
    let gate = axon_core::sqlite::SqliteWriteGate::default();
    let small = candidate("small", "repo:small");
    let mut large = candidate("large", "repo:large");
    large.nodes = (0..512)
        .map(|index| {
            let mut node = large.nodes[0].clone();
            node.stable_key = format!("repo:large:{index}");
            node
        })
        .collect();
    let reader = pool.clone();
    let candidates = [small, large, candidate("tail", "repo:tail")]
        .into_iter().enumerate().map(move |(index, value)| {
            if index == 2 {
                let rows: i64 = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(
                        sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes").fetch_one(&reader)
                    )
                }).unwrap();
                assert_eq!(rows, 513, "large candidate must be committed and its writer released before consuming more input");
            }
            value
        });
    let result = upsert_candidate_iter(&pool, &gate, candidates)
        .await
        .unwrap();
    assert_eq!(result.nodes_upserted, 514);
}

#[tokio::test]
async fn grouped_stream_preserves_valid_prefix_on_validation_and_source_errors() {
    for mixed_source in [false, true] {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        crate::migration::ensure_schema(&pool).await.unwrap();
        let gate = axon_core::sqlite::SqliteWriteGate::default();
        let mut bad = candidate("bad", "repo:bad");
        if mixed_source {
            bad.source_id = SourceId::new("other-source");
        } else {
            bad.nodes[0].node_kind = "not-a-node-kind".into();
        }
        upsert_candidate_iter(&pool, &gate, [candidate("good", "repo:good"), bad])
            .await
            .unwrap_err();
        let keys: Vec<String> = sqlx::query_scalar("SELECT stable_key FROM graph_nodes")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(keys, ["repo:good"]);
    }
}

#[tokio::test]
async fn unchanged_aliases_avoid_updates_but_changed_targets_still_update() {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    crate::migration::ensure_schema(&pool).await.unwrap();
    let gate = axon_core::sqlite::SqliteWriteGate::default();
    let first = candidate("first", "repo:first");
    let second = candidate("second", "repo:second");
    upsert_candidates(&pool, &gate, vec![first.clone(), second.clone()])
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE alias_updates (n INTEGER); CREATE TRIGGER count_alias_updates AFTER UPDATE ON graph_aliases BEGIN INSERT INTO alias_updates VALUES (1); END;")
        .execute(&pool).await.unwrap();
    upsert_candidates(&pool, &gate, vec![first]).await.unwrap();
    let updates: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alias_updates")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(updates, 0);
    let target = resolve_node(&second.nodes[0]).node_id.0;
    let mut connection = pool.acquire().await.unwrap();
    execute_alias_batch(
        &mut connection,
        &[("stable_key".into(), "repo:first".into(), target.clone())],
    )
    .await
    .unwrap();
    let mapped: String = sqlx::query_scalar("SELECT node_id FROM graph_aliases WHERE alias_kind = 'stable_key' AND alias_value = 'repo:first'")
        .fetch_one(&mut *connection).await.unwrap();
    assert_eq!(mapped, target);
    let updates: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alias_updates")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    assert_eq!(updates, 1);
}

#[test]
fn edge_statement_batches_stay_below_sqlite_bind_limit() {
    assert_eq!(edge_read_batch_sizes(1_001), vec![900, 101]);
    assert_eq!(edge_write_batch_sizes(201), vec![100, 100, 1]);
    const {
        assert!(EDGE_WRITE_BATCH_SIZE * EDGE_WRITE_BINDS_PER_ROW <= SQLITE_SAFE_BIND_LIMIT);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn streaming_publication_releases_writer_between_bounded_groups() {
    let url = format!(
        "sqlite:file:graph-publication-{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let pool = SqlitePool::connect(&url).await.unwrap();
    let write_gate = axon_core::sqlite::SqliteWriteGate::default();
    crate::migration::ensure_schema(&pool).await.unwrap();
    sqlx::query("CREATE TABLE heartbeat_probe (value INTEGER NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();

    let writer_completed = Arc::new(AtomicBool::new(false));
    let observed_before_second = Arc::new(AtomicBool::new(false));
    let writer_flag = Arc::clone(&writer_completed);
    let writer_pool = pool.clone();
    let writer_gate = write_gate.clone();
    let writer = tokio::spawn(async move {
        for _ in 0..100 {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_nodes")
                .fetch_one(&writer_pool)
                .await
                .unwrap();
            if count > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let _permit = writer_gate.lock().await;
        sqlx::query("INSERT INTO heartbeat_probe (value) VALUES (1)")
            .execute(&writer_pool)
            .await
            .unwrap();
        writer_flag.store(true, Ordering::Release);
    });

    let observed = Arc::clone(&observed_before_second);
    let completed = Arc::clone(&writer_completed);
    let candidates = (0..17).map(move |index| {
        if index == 16 {
            std::thread::sleep(Duration::from_millis(100));
            observed.store(completed.load(Ordering::Acquire), Ordering::Release);
        }
        candidate(&format!("candidate-{index}"), &format!("repo:{index}"))
    });

    upsert_candidate_iter(&pool, &write_gate, candidates)
        .await
        .unwrap();
    writer.await.unwrap();
    assert!(
        observed_before_second.load(Ordering::Acquire),
        "the gated heartbeat writer must commit between bounded graph groups"
    );
}

#[tokio::test]
async fn injected_streaming_failure_leaves_only_the_committed_candidate_prefix_visible() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let write_gate = axon_core::sqlite::SqliteWriteGate::default();
    crate::migration::ensure_schema(&pool).await.unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_second_candidate BEFORE INSERT ON graph_nodes \
         WHEN NEW.stable_key = 'repo:second' \
         BEGIN SELECT RAISE(FAIL, 'injected candidate failure'); END",
    )
    .execute(&pool)
    .await
    .unwrap();

    let error = upsert_candidate_iter(
        &pool,
        &write_gate,
        [
            candidate("first", "repo:first"),
            candidate("second", "repo:second"),
        ],
    )
    .await
    .unwrap_err();
    assert_eq!(error.code.to_string(), "graph.storage");
    let keys: Vec<String> =
        sqlx::query_scalar("SELECT stable_key FROM graph_nodes ORDER BY stable_key")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(keys, vec!["repo:first"]);
}

#[tokio::test]
async fn batched_failure_replays_only_the_valid_prefix() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let write_gate = axon_core::sqlite::SqliteWriteGate::default();
    crate::migration::ensure_schema(&pool).await.unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_second_candidate BEFORE INSERT ON graph_nodes \
         WHEN NEW.stable_key = 'repo:second' \
         BEGIN SELECT RAISE(FAIL, 'injected candidate failure'); END",
    )
    .execute(&pool)
    .await
    .unwrap();

    let candidates = vec![
        candidate("first", "repo:first"),
        candidate("second", "repo:second"),
        candidate("third", "repo:third"),
    ];
    let error = upsert_candidates(&pool, &write_gate, candidates.clone())
        .await
        .unwrap_err();
    assert_eq!(error.code.to_string(), "graph.storage");
    let keys: Vec<String> =
        sqlx::query_scalar("SELECT stable_key FROM graph_nodes ORDER BY stable_key")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(keys, vec!["repo:first"]);

    sqlx::query("DROP TRIGGER fail_second_candidate")
        .execute(&pool)
        .await
        .unwrap();
    let result = upsert_candidates(&pool, &write_gate, candidates)
        .await
        .unwrap();
    assert_eq!(result.candidates_seen, 3);
    assert_eq!(result.nodes_upserted, 3);
}
