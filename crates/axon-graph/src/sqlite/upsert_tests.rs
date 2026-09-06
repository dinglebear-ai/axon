use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

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

#[test]
fn edge_statement_batches_stay_below_sqlite_bind_limit() {
    assert_eq!(edge_read_batch_sizes(1_001), vec![900, 101]);
    assert_eq!(edge_write_batch_sizes(201), vec![100, 100, 1]);
    const {
        assert!(EDGE_WRITE_BATCH_SIZE * EDGE_WRITE_BINDS_PER_ROW <= SQLITE_SAFE_BIND_LIMIT);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn streaming_publication_releases_writer_between_candidates() {
    let url = format!(
        "sqlite:file:graph-publication-{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let pool = SqlitePool::connect(&url).await.unwrap();
    crate::migration::ensure_schema(&pool).await.unwrap();
    sqlx::query("CREATE TABLE heartbeat_probe (value INTEGER NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();

    let writer_completed = Arc::new(AtomicBool::new(false));
    let observed_before_second = Arc::new(AtomicBool::new(false));
    let writer_flag = Arc::clone(&writer_completed);
    let writer_pool = pool.clone();
    let writer = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        sqlx::query("INSERT INTO heartbeat_probe (value) VALUES (1)")
            .execute(&writer_pool)
            .await
            .unwrap();
        writer_flag.store(true, Ordering::Release);
    });

    let observed = Arc::clone(&observed_before_second);
    let completed = Arc::clone(&writer_completed);
    let candidates = [
        candidate("first", "repo:first"),
        candidate("second", "repo:second"),
    ]
    .into_iter()
    .enumerate()
    .map(move |(index, candidate)| {
        if index == 1 {
            std::thread::sleep(Duration::from_millis(100));
            observed.store(completed.load(Ordering::Acquire), Ordering::Release);
        }
        candidate
    });

    upsert_candidate_iter(&pool, candidates).await.unwrap();
    writer.await.unwrap();
    assert!(
        observed_before_second.load(Ordering::Acquire),
        "the competing heartbeat writer must commit before the second candidate transaction"
    );
}

#[tokio::test]
async fn injected_streaming_failure_leaves_only_the_committed_candidate_prefix_visible() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
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
