use super::*;
use crate::context::ServiceContext;
use crate::jobs as job_service;
use axon_jobs::backend::JobKind as LegacyJobKind;
use std::sync::Arc;
use std::time::Duration;

async fn test_ctx_with_workers() -> ServiceContext {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = axon_core::config::Config {
        sqlite_path: dir.path().join("jobs.db"),
        ..axon_core::config::Config::test_default()
    };
    std::mem::forget(dir);
    ServiceContext::new_with_workers(Arc::new(cfg))
        .await
        .expect("service context")
}

/// Extract now enqueues onto the unified `JobStore` and runs on the real
/// unified worker (not the legacy per-family `axon_extract_jobs` backend),
/// while `job_service::job_status`/`list_jobs`/`cancel_job`/etc. for
/// `JobKind::Extract` bridge onto the same store (see
/// `runtime/sqlite/extract_bridge.rs`) so existing CLI/MCP/REST callers keep
/// working unchanged. Empty URLs keep this deterministic and network-free.
#[tokio::test]
async fn extract_job_runs_end_to_end_on_the_unified_store() {
    let ctx = test_ctx_with_workers().await;

    // `extract_start_with_context` only rejects an empty *slice* -- a single
    // deliberately-invalid URL keeps this test network-free (it fails fast
    // in URL parsing/fetch) while still proving the unified worker really
    // dispatches to `axon_extract::sync::extract_sync`, not the
    // `job_runner.unsupported_stage` catch-all.
    let outcome = extract_start_with_context(
        ctx.cfg(),
        &["not-a-real-url".to_string()],
        None,
        &ctx,
        None,
        None,
    )
    .await
    .expect("enqueue");
    let job_id = uuid::Uuid::parse_str(&outcome.result.job_id).expect("job id");

    let mut status = None;
    for _ in 0..100 {
        let job = job_service::job_status(&ctx, LegacyJobKind::Extract, job_id)
            .await
            .expect("job_status")
            .expect("job exists");
        if job.status != "pending" && job.status != "running" {
            status = Some(job);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let job = status.expect("extract job reached a terminal status within timeout");
    let unsupported_stage = job
        .error_text
        .as_deref()
        .is_some_and(|text| text.contains("not wired yet"));
    assert!(
        !unsupported_stage,
        "extract must dispatch to the real runner, not the catch-all: {:?}",
        job.error_text
    );

    let jobs = job_service::list_jobs(&ctx, LegacyJobKind::Extract, 10, 0)
        .await
        .expect("list_jobs");
    assert!(jobs.iter().any(|j| j.id == job_id));
}

/// `extract_bridge::job_summary_to_service_job` reads back the original
/// `{"urls": [...], "config_json": "..."}` request payload (via
/// `JobStore::request_json`) and populates `ServiceJob.urls_json`/`target`,
/// so `axon extract list`/`axon extract status <id>` show the real target
/// URLs instead of falling back to the job id. Regression test for the
/// cosmetic gap documented in commit 33aea9fa8.
#[tokio::test]
async fn extract_job_status_and_list_expose_the_original_urls() {
    let ctx = test_ctx_with_workers().await;
    let urls = vec!["not-a-real-url".to_string()];

    let outcome = extract_start_with_context(ctx.cfg(), &urls, None, &ctx, None, None)
        .await
        .expect("enqueue");
    let job_id = uuid::Uuid::parse_str(&outcome.result.job_id).expect("job id");

    // Wait for the job to reach a terminal status so urls_json is stable to
    // assert against (the bridge populates it from the stored request on
    // every call, so it is actually already present right after enqueue --
    // draining to terminal just keeps this test consistent with the
    // end-to-end test above).
    let mut terminal = None;
    for _ in 0..100 {
        let job = job_service::job_status(&ctx, LegacyJobKind::Extract, job_id)
            .await
            .expect("job_status")
            .expect("job exists");
        if job.status != "pending" && job.status != "running" {
            terminal = Some(job);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let job = terminal.expect("extract job reached a terminal status within timeout");

    let urls_json = job
        .urls_json
        .as_ref()
        .expect("urls_json must be populated from the stored request payload");
    let listed_urls: Vec<String> = serde_json::from_value(urls_json.clone())
        .expect("urls_json must deserialize back to the original URL list");
    assert_eq!(listed_urls, urls);
    assert_eq!(
        job.target.as_deref(),
        Some(urls[0].as_str()),
        "single-URL extract jobs should surface the URL as target too"
    );

    // `axon extract list`'s equivalent rendering path must see the same data.
    let jobs = job_service::list_jobs(&ctx, LegacyJobKind::Extract, 10, 0)
        .await
        .expect("list_jobs");
    let listed = jobs
        .iter()
        .find(|j| j.id == job_id)
        .expect("job present in list");
    let listed_urls_json = listed
        .urls_json
        .as_ref()
        .expect("list_jobs row must also carry urls_json");
    let listed_urls: Vec<String> = serde_json::from_value(listed_urls_json.clone())
        .expect("list_jobs urls_json must deserialize back to the original URL list");
    assert_eq!(listed_urls, urls);
}
