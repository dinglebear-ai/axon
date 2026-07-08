use super::*;
use crate::context::ServiceContext;
use axon_api::source::{JobKind, JobListRequest};
use axon_core::config::Config;
use std::sync::Arc;

async fn test_ctx() -> ServiceContext {
    // `Config::test_default()` sets `tavily_api_key = "test-key"`, which would
    // make `research_with_context` actually reach out to Tavily over the
    // network. Clearing both `tavily_api_key` and `searxng_url` makes
    // `ensure_tavily_configured` (called first thing inside
    // `research_payload`) fail fast and deterministically, before any
    // network call, while still exercising the real job-tracking wrapper
    // end to end.
    //
    // `Config::default()`'s `sqlite_path` points at the real
    // `~/.axon/jobs.db` — every test in this process would otherwise share
    // one on-disk job store and pollute each other's `JobKind::Research`
    // listings. Point each test at its own tempdir DB instead (leaked via
    // `mem::forget` so the dir outlives the SQLite connection pool, mirroring
    // the pattern in `crate::extract_tests`).
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = Config {
        tavily_api_key: String::new(),
        searxng_url: String::new(),
        sqlite_path: dir.path().join("jobs.db"),
        ..Config::test_default()
    };
    std::mem::forget(dir);
    // `job_store()` is populated by the SQLite runtime regardless of
    // `spawn_workers`, so `ServiceContext::new` (no in-process workers) is
    // sufficient — `track_research_job` only needs `ctx.job_store()`, not a
    // running worker loop, since research executes inline in the caller.
    ServiceContext::new(Arc::new(cfg))
        .await
        .expect("service context")
}

/// `track_research_job` must create a real unified job record for a
/// `research` call (Research is unconditionally `JobPolicy::JobBacked`), and
/// must leave that job `Failed` (not stuck `Queued`/`Running`) when the
/// wrapped operation errors out. The missing Tavily/SearXNG config makes the
/// operation fail immediately in `ensure_tavily_configured`, before any
/// network call, so this stays deterministic and network-free.
#[tokio::test]
async fn track_research_job_records_failed_status_on_error() {
    let ctx = test_ctx().await;
    let store = ctx.job_store().expect("unified job store must be present");

    let request_json = serde_json::json!({"operation": "research", "query": "test query"});
    let result: Result<(), Box<dyn std::error::Error>> =
        track_research_job(&ctx, request_json, || async {
            crate::search::ensure_tavily_configured(ctx.cfg(), "research")
        })
        .await;

    assert!(
        result.is_err(),
        "expected the wrapped operation to fail fast without TAVILY_API_KEY/AXON_SEARXNG_URL"
    );

    let page = store
        .list(JobListRequest {
            status: None,
            kind: Some(JobKind::Research),
            source_id: None,
            watch_id: None,
            limit: Some(10),
            cursor: None,
        })
        .await
        .expect("list jobs");

    assert_eq!(
        page.items.len(),
        1,
        "expected exactly one Research job to have been created"
    );
    let job = &page.items[0];
    assert_eq!(
        job.status,
        LifecycleStatus::Failed,
        "job must be marked Failed, not left Queued/Running: {job:?}"
    );
}

/// A successful operation must leave the job `Completed`.
#[tokio::test]
async fn track_research_job_records_completed_status_on_success() {
    let ctx = test_ctx().await;
    let store = ctx.job_store().expect("unified job store must be present");

    let request_json = serde_json::json!({"operation": "research", "query": "test query"});
    let result: Result<u32, Box<dyn std::error::Error>> =
        track_research_job(&ctx, request_json, || async { Ok(42u32) }).await;

    assert_eq!(result.expect("op should succeed"), 42);

    let page = store
        .list(JobListRequest {
            status: None,
            kind: Some(JobKind::Research),
            source_id: None,
            watch_id: None,
            limit: Some(10),
            cursor: None,
        })
        .await
        .expect("list jobs");

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].status, LifecycleStatus::Completed);
}
