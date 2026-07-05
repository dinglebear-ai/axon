use std::process::Command;
use std::sync::Arc;

use axon_api::source::*;
use axon_core::config::Config;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::payload::generation_payload_i64;
use axon_vectors::store::FakeVectorStore;
use axon_vectors::store::VectorStore;
use axon_vectors::testing::{TestPointSpec, test_clean_point};

use super::*;
use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use crate::test_support::NoopServiceRuntime;
use crate::types::CodeSearchCaller;

const BATCH_ID: &str = "00000000-0000-0000-0000-000000000001";
const JOB_ID: &str = "00000000-0000-0000-0000-000000000099";

#[tokio::test]
async fn target_code_search_keeps_unchanged_previous_generation_results_visible() {
    let repo = tempfile::tempdir().expect("repo");
    Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["init", "-q"])
        .status()
        .expect("git init");
    std::fs::write(
        repo.path().join("lib.rs"),
        "pub fn changed() -> i32 { 1 }\n",
    )
    .expect("changed file");
    std::fs::write(
        repo.path().join("stable.rs"),
        "pub fn stable_answer() -> i32 { 42 }\n",
    )
    .expect("stable file");

    let cfg = Arc::new(Config::test_default());
    let service_jobs = Arc::new(NoopServiceRuntime);
    let source_jobs = Arc::new(FakeJobWatchStore::new());
    let ledger = Arc::new(FakeLedgerStore::new());
    let embedder = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8));
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ctx = ServiceContext::from_runtime(cfg.clone(), service_jobs)
        .with_target_local_source_runtime(TargetLocalSourceRuntime::new(
            source_jobs,
            ledger,
            embedder,
            vectors.clone(),
            ProviderId::new("fake-embedding"),
            "fake-embedding",
            8,
        ));

    let first = refresh_code_search_index_with_progress(
        &ctx,
        Some(repo.path()),
        CodeSearchCaller::Cli,
        None,
    )
    .await
    .expect("first target refresh");
    std::fs::write(
        repo.path().join("lib.rs"),
        "pub fn changed() -> i32 { 2 }\n",
    )
    .expect("modified file");
    let second = refresh_code_search_index_with_progress(
        &ctx,
        Some(repo.path()),
        CodeSearchCaller::Cli,
        None,
    )
    .await
    .expect("second target refresh");
    assert_ne!(
        first.target_source_generation,
        second.target_source_generation
    );
    let first_generation = first.target_source_generation.as_ref().expect("first gen");
    let first_payload_generation =
        generation_payload_i64(first_generation, "source_generation").expect("first payload gen");
    let second_generation = second
        .target_source_generation
        .as_ref()
        .expect("second gen");
    let second_payload_generation =
        generation_payload_i64(second_generation, "source_generation").expect("second payload gen");

    let stable_points = vectors
        .points(&cfg.collection)
        .await
        .into_iter()
        .filter(|point| {
            point
                .payload
                .get("source_item_key")
                .and_then(|value| value.as_str())
                == Some("stable.rs")
        })
        .collect::<Vec<_>>();
    assert!(!stable_points.is_empty());
    assert!(stable_points.iter().any(|point| {
        point.payload["source_generation"] == serde_json::json!(first_payload_generation)
            && point.payload["committed_generation"] == serde_json::json!(first_payload_generation)
    }));
    assert!(stable_points.iter().any(|point| {
        point.payload["source_generation"] == serde_json::json!(second_payload_generation)
            && point.payload["committed_generation"] == serde_json::json!(second_payload_generation)
    }));

    let third = refresh_code_search_index_with_progress(
        &ctx,
        Some(repo.path()),
        CodeSearchCaller::Cli,
        None,
    )
    .await
    .expect("third unchanged target refresh");
    assert_eq!(
        third.target_source_generation,
        second.target_source_generation
    );

    let searched = code_search(
        &ctx,
        "stable_answer",
        CodeSearchOptions {
            limit: 10,
            offset: 0,
            cwd: Some(repo.path().to_path_buf()),
            path_prefix: None,
            ensure_fresh: false,
            caller: CodeSearchCaller::Cli,
        },
    )
    .await
    .expect("target search");

    assert_eq!(searched.freshness.status, "skipped");
    assert!(
        searched
            .results
            .iter()
            .any(|hit| hit.file_path.as_deref() == Some("stable.rs")),
        "unchanged committed result should remain searchable: {searched:#?}"
    );
}

#[tokio::test]
async fn target_code_search_excludes_uncommitted_and_redacted_vectors() {
    let repo = tempfile::tempdir().expect("repo");
    Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["init", "-q"])
        .status()
        .expect("git init");
    std::fs::write(
        repo.path().join("visible.rs"),
        "pub fn visible_answer() {}\n",
    )
    .expect("visible file");

    let cfg = Arc::new(Config::test_default());
    let service_jobs = Arc::new(NoopServiceRuntime);
    let source_jobs = Arc::new(FakeJobWatchStore::new());
    let ledger = Arc::new(FakeLedgerStore::new());
    let embedder = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8));
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ctx = ServiceContext::from_runtime(cfg.clone(), service_jobs)
        .with_target_local_source_runtime(TargetLocalSourceRuntime::new(
            source_jobs,
            ledger,
            embedder,
            vectors.clone(),
            ProviderId::new("fake-embedding"),
            "fake-embedding",
            8,
        ));

    let refreshed = refresh_code_search_index_with_progress(
        &ctx,
        Some(repo.path()),
        CodeSearchCaller::Cli,
        None,
    )
    .await
    .expect("target refresh");
    let committed = refreshed
        .target_source_generation
        .as_ref()
        .expect("committed generation");
    let committed_payload_generation =
        generation_payload_i64(committed, "committed_generation").expect("numeric generation");
    let source_id = refreshed.target_source_id.as_ref().expect("source id");
    let committed_points = vectors.points(&cfg.collection).await;
    let visible_point = committed_points
        .iter()
        .find(|point| {
            point
                .payload
                .get("source_item_key")
                .and_then(|value| value.as_str())
                == Some("visible.rs")
        })
        .expect("visible point");
    assert_eq!(
        visible_point.payload["committed_generation"],
        serde_json::json!(committed_payload_generation)
    );
    assert_eq!(
        visible_point.payload["visibility"],
        serde_json::json!("public")
    );
    assert_eq!(
        visible_point.payload["redaction_status"],
        serde_json::json!("clean")
    );
    let request = target_code_search_request(
        cfg.collection.clone(),
        "answer",
        20,
        vec![1.0; 8],
        source_id,
        committed,
        None,
    )
    .expect("target search request");
    assert_eq!(request.filters["source_id"], serde_json::json!(source_id.0));
    assert_eq!(
        request.filters["committed_generation"],
        serde_json::json!(committed_payload_generation)
    );
    assert_eq!(request.filters["visibility"], serde_json::json!("public"));
    assert_eq!(
        request.filters["redaction_status"],
        serde_json::json!("clean")
    );

    let mut staged = test_clean_point(TestPointSpec {
        collection: &cfg.collection,
        point_id: "staged-point",
        chunk_id: "staged-chunk",
        vector: &[1.0; 8],
        text: "pub fn staged_answer() {}",
        namespace: "code",
        batch_id: BATCH_ID,
        model: "fake-embedding",
        dimensions: 8,
        job_id: JOB_ID,
    });
    staged
        .payload
        .insert("source_id".to_string(), serde_json::json!(source_id.0));
    staged
        .payload
        .insert("source_generation".to_string(), serde_json::json!(999));
    staged
        .payload
        .insert("committed_generation".to_string(), serde_json::json!(999));
    staged
        .payload
        .insert("visibility".to_string(), serde_json::json!("public"));
    staged
        .payload
        .insert("redaction_status".to_string(), serde_json::json!("clean"));
    staged.payload.insert(
        "source_item_key".to_string(),
        serde_json::json!("staged.rs"),
    );
    staged.payload.insert(
        "item_canonical_uri".to_string(),
        serde_json::json!("staged.rs"),
    );

    let mut redacted = test_clean_point(TestPointSpec {
        collection: &cfg.collection,
        point_id: "redacted-point",
        chunk_id: "redacted-chunk",
        vector: &[1.0; 8],
        text: "pub fn redacted_answer() {}",
        namespace: "code",
        batch_id: BATCH_ID,
        model: "fake-embedding",
        dimensions: 8,
        job_id: JOB_ID,
    });
    redacted
        .payload
        .insert("source_id".to_string(), serde_json::json!(source_id.0));
    redacted.payload.insert(
        "source_generation".to_string(),
        serde_json::json!(committed_payload_generation),
    );
    redacted.payload.insert(
        "committed_generation".to_string(),
        serde_json::json!(committed_payload_generation),
    );
    redacted
        .payload
        .insert("visibility".to_string(), serde_json::json!("public"));
    redacted.payload.insert(
        "redaction_status".to_string(),
        serde_json::json!("redacted"),
    );
    redacted.payload.insert(
        "source_item_key".to_string(),
        serde_json::json!("redacted.rs"),
    );
    redacted.payload.insert(
        "item_canonical_uri".to_string(),
        serde_json::json!("redacted.rs"),
    );

    vectors
        .upsert(VectorPointBatch {
            batch_id: BatchId::new(uuid::Uuid::from_u128(1)),
            collection: cfg.collection.clone(),
            points: vec![staged, redacted],
            model: "fake-embedding".to_string(),
            dimensions: 8,
            sparse_vectors: None,
            payload_indexes: Vec::new(),
        })
        .await
        .expect("insert test points");

    let searched = code_search(
        &ctx,
        "answer",
        CodeSearchOptions {
            limit: 20,
            offset: 0,
            cwd: Some(repo.path().to_path_buf()),
            path_prefix: None,
            ensure_fresh: false,
            caller: CodeSearchCaller::Cli,
        },
    )
    .await
    .expect("target search");

    assert!(
        searched
            .results
            .iter()
            .any(|hit| hit.file_path.as_deref() == Some("visible.rs")),
        "committed clean result should be visible: {searched:#?}"
    );
    assert!(
        searched
            .results
            .iter()
            .all(|hit| hit.file_path.as_deref() != Some("staged.rs")),
        "staged generation leaked into results: {searched:#?}"
    );
    assert!(
        searched
            .results
            .iter()
            .all(|hit| hit.file_path.as_deref() != Some("redacted.rs")),
        "redacted result leaked into results: {searched:#?}"
    );
}

#[tokio::test]
async fn target_code_search_fails_refresh_but_can_query_last_committed_generation_when_skipped() {
    let repo = tempfile::tempdir().expect("repo");
    Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["init", "-q"])
        .status()
        .expect("git init");
    std::fs::write(
        repo.path().join("lib.rs"),
        "pub fn answer() -> i32 { 42 }\n",
    )
    .expect("source file");

    let cfg = Arc::new(Config::test_default());
    let service_jobs = Arc::new(NoopServiceRuntime);
    let source_jobs = Arc::new(FakeJobWatchStore::new());
    let ledger = Arc::new(FakeLedgerStore::new());
    let embedder = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8));
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ctx = ServiceContext::from_runtime(cfg, service_jobs).with_target_local_source_runtime(
        TargetLocalSourceRuntime::new(
            source_jobs,
            ledger,
            embedder,
            vectors,
            ProviderId::new("fake-embedding"),
            "fake-embedding",
            8,
        ),
    );

    refresh_code_search_index_with_progress(&ctx, Some(repo.path()), CodeSearchCaller::Cli, None)
        .await
        .expect("first target refresh");
    std::fs::write(repo.path().join("bad.rs"), [0xff, 0xfe, 0xfd]).expect("bad file");

    let searched = code_search(
        &ctx,
        "answer",
        CodeSearchOptions {
            limit: 10,
            offset: 0,
            cwd: Some(repo.path().to_path_buf()),
            path_prefix: None,
            ensure_fresh: true,
            caller: CodeSearchCaller::Cli,
        },
    )
    .await
    .expect("target search should fall back to last committed generation");

    assert_eq!(searched.freshness.status, "stale");
    assert!(
        searched
            .freshness
            .warning
            .as_deref()
            .is_some_and(|warning| warning.contains("valid UTF-8")),
        "refresh failure warning should mention the indexing failure: {searched:#?}"
    );
    assert!(
        searched
            .results
            .iter()
            .any(|hit| hit.file_path.as_deref() == Some("lib.rs")),
        "last committed generation should remain searchable: {searched:#?}"
    );
}
