use std::process::Command;
use std::sync::Arc;

use axon_api::source::*;
use axon_core::config::Config;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::FakeVectorStore;

use super::*;
use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use crate::test_support::NoopServiceRuntime;
use crate::types::CodeSearchCaller;

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

    let first = refresh_code_search_index_with_backend(
        &ctx,
        Some(repo.path()),
        CodeSearchCaller::Cli,
        CodeSearchRefreshBackend::TargetLocalSource,
        None,
    )
    .await
    .expect("first target refresh");
    std::fs::write(
        repo.path().join("lib.rs"),
        "pub fn changed() -> i32 { 2 }\n",
    )
    .expect("modified file");
    let second = refresh_code_search_index_with_backend(
        &ctx,
        Some(repo.path()),
        CodeSearchCaller::Cli,
        CodeSearchRefreshBackend::TargetLocalSource,
        None,
    )
    .await
    .expect("second target refresh");
    assert_ne!(
        first.target_source_generation,
        second.target_source_generation
    );
    let first_generation = first.target_source_generation.as_ref().expect("first gen");
    let second_generation = second
        .target_source_generation
        .as_ref()
        .expect("second gen");

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
        point.payload["source_generation"].as_str() == Some(first_generation.0.as_str())
            && point.payload["committed_generation"].as_str() == Some(first_generation.0.as_str())
    }));
    assert!(stable_points.iter().any(|point| {
        point.payload["source_generation"].as_str() == Some(second_generation.0.as_str())
            && point.payload["committed_generation"].as_str() == Some(second_generation.0.as_str())
    }));

    let third = refresh_code_search_index_with_backend(
        &ctx,
        Some(repo.path()),
        CodeSearchCaller::Cli,
        CodeSearchRefreshBackend::TargetLocalSource,
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

    refresh_code_search_index_with_backend(
        &ctx,
        Some(repo.path()),
        CodeSearchCaller::Cli,
        CodeSearchRefreshBackend::TargetLocalSource,
        None,
    )
    .await
    .expect("first target refresh");
    std::fs::write(repo.path().join("bad.rs"), [0xff, 0xfe, 0xfd]).expect("bad file");

    let err = code_search(
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
    .expect_err("ensure_fresh target search should fail after refresh failure");
    assert!(
        err.to_string().contains("valid UTF-8"),
        "unexpected error: {err:#}"
    );

    let searched = code_search(
        &ctx,
        "answer",
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
    .expect("target search without refresh");

    assert_eq!(searched.freshness.status, "skipped");
    assert!(searched.freshness.warning.is_none());
    assert!(
        searched
            .results
            .iter()
            .any(|hit| hit.file_path.as_deref() == Some("lib.rs")),
        "last committed generation should remain searchable: {searched:#?}"
    );
}
