use super::{crawl_start_with_context, map_crawl_start_result, predict_crawl_output_dir};
use crate::context::ServiceContext;
use crate::types::{ExecutionMode, StartDisposition};
use axon_api::source::{AuthSnapshot, CallerContext, TransportKind, Visibility};
use axon_core::config::Config;
use axon_jobs::backend::JobKind as LegacyJobKind;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

fn test_config(start_url: &str) -> Config {
    let mut cfg = Config::default_minimal();
    cfg.start_url = start_url.to_string();
    cfg
}

async fn test_ctx_with_workers(start_url: &str) -> (Config, ServiceContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cfg = test_config(start_url);
    cfg.sqlite_path = dir.path().join("jobs.db");
    std::mem::forget(dir);
    let ctx = ServiceContext::new_with_workers(Arc::new(cfg.clone()))
        .await
        .expect("service context");
    (cfg, ctx)
}

#[test]
fn map_crawl_start_result_includes_predicted_output_paths() {
    let result = map_crawl_start_result(
        Path::new("/tmp/axon-output"),
        &[("https://docs.rs".to_string(), "job-123".to_string())],
    );

    assert_eq!(result.job_ids, vec!["job-123".to_string()]);
    assert_eq!(
        result.output_dir,
        Some("/tmp/axon-output/domains/docs.rs/job-123".to_string())
    );
    assert_eq!(
        result.predicted_paths,
        vec![
            "/tmp/axon-output/domains/docs.rs/job-123/manifest.jsonl".to_string(),
            "/tmp/axon-output/domains/docs.rs/job-123/markdown".to_string(),
            "/tmp/axon-output/domains/docs.rs/job-123/audit/docs-rs-diff-report.json".to_string(),
        ]
    );
    assert_eq!(result.predicted_artifact_handles.len(), 3);
    assert_eq!(
        result.predicted_artifact_handles[0].relative_path(),
        "domains/docs.rs/job-123/manifest.jsonl"
    );
    assert_eq!(
        result.predicted_artifact_handles[0].job_id(),
        Some("job-123")
    );
    assert_eq!(
        result.predicted_artifact_handles[0].url(),
        Some("https://docs.rs")
    );
    assert_eq!(result.jobs.len(), 1);
    let job = &result.jobs[0];
    assert_eq!(job.url, "https://docs.rs");
    assert_eq!(
        job.output_dir,
        "/tmp/axon-output/domains/docs.rs/job-123".to_string()
    );
    assert_eq!(
        job.predicted_paths,
        vec![
            "/tmp/axon-output/domains/docs.rs/job-123/manifest.jsonl".to_string(),
            "/tmp/axon-output/domains/docs.rs/job-123/markdown".to_string(),
            "/tmp/axon-output/domains/docs.rs/job-123/audit/docs-rs-diff-report.json".to_string(),
        ]
    );
    assert_eq!(job.predicted_artifact_handles.len(), 3);
}

#[test]
fn predict_crawl_output_dir_uses_runtime_job_layout() {
    let output_dir = predict_crawl_output_dir(
        Path::new(".cache/axon-rust/output"),
        "https://[::1]:8080/docs",
        "job-456",
    );

    assert_eq!(
        output_dir,
        Path::new(".cache/axon-rust/output")
            .join("domains")
            .join("___1_")
            .join("job-456")
    );
}

#[test]
fn resolve_crawl_max_pages_default_and_cap() {
    use super::{DEFAULT_CRAWL_MAX_PAGES, resolve_crawl_max_pages};
    // unspecified (0) → default
    assert_eq!(resolve_crawl_max_pages(0, false), DEFAULT_CRAWL_MAX_PAGES);
    // within bounds → unchanged
    assert_eq!(resolve_crawl_max_pages(120, false), 120);
    assert_eq!(
        resolve_crawl_max_pages(DEFAULT_CRAWL_MAX_PAGES, false),
        DEFAULT_CRAWL_MAX_PAGES
    );
    // over the cap → clamped
    assert_eq!(
        resolve_crawl_max_pages(50_000, false),
        DEFAULT_CRAWL_MAX_PAGES
    );
    // operator override passes through untouched, including 0 (uncapped)
    assert_eq!(resolve_crawl_max_pages(0, true), 0);
    assert_eq!(resolve_crawl_max_pages(50_000, true), 50_000);
}

#[test]
fn map_crawl_job_result_preserves_output_files() {
    let result = super::map_crawl_job_result_with_root(
        serde_json::json!({
            "id": "job-123",
            "url": "https://docs.rs",
            "phase": "completed",
            "output_files": [
                "/tmp/axon-output/manifest.jsonl",
                "/tmp/axon-output/markdown/index.md"
            ]
        }),
        Some(Path::new("/tmp/axon-output")),
    );

    assert_eq!(
        result.output_files.as_ref(),
        Some(&vec![
            "/tmp/axon-output/manifest.jsonl".to_string(),
            "/tmp/axon-output/markdown/index.md".to_string(),
        ])
    );
    assert_eq!(result.output_file_handles.len(), 2);
    assert_eq!(
        result.output_file_handles[0].relative_path(),
        "manifest.jsonl"
    );
    assert_eq!(result.output_file_handles[0].job_id(), Some("job-123"));
    assert_eq!(result.output_file_handles[0].url(), Some("https://docs.rs"));
}

#[test]
fn map_crawl_job_result_derives_handles_from_result_json_paths() {
    let result = super::map_crawl_job_result_with_root(
        serde_json::json!({
            "id": "job-456",
            "url": "https://docs.rs",
            "result": {
                "output_dir": "/tmp/axon-output/domains/docs.rs/job-456",
                "output_path": "/tmp/axon-output/domains/docs.rs/job-456/markdown"
            }
        }),
        Some(Path::new("/tmp/axon-output")),
    );

    assert_eq!(result.output_file_handles.len(), 2);
    assert_eq!(
        result.output_file_handles[0].relative_path(),
        "domains/docs.rs/job-456/manifest.jsonl"
    );
    assert_eq!(
        result.output_file_handles[1].relative_path(),
        "domains/docs.rs/job-456/markdown"
    );
}

#[tokio::test]
async fn crawl_start_with_context_rejects_empty_urls() {
    let (cfg, ctx) = test_ctx_with_workers("https://docs.rs").await;

    let err = crawl_start_with_context(&cfg, &[], &ctx, None, None)
        .await
        .expect_err("empty urls must fail");
    assert!(err.to_string().contains("No URLs provided"));
}

/// Crawl now enqueues onto the unified `JobStore` with real caller auth
/// (mirroring Task 1's embed cutover), one unified job per URL.
#[tokio::test]
async fn crawl_start_with_context_enqueues_on_unified_job_store_with_caller_auth() {
    let (cfg, ctx) = test_ctx_with_workers("https://docs.rs").await;
    let caller = AuthSnapshot::from_caller(
        &CallerContext {
            actor: Some("user_1".to_string()),
            transport: TransportKind::Cli,
            scopes: vec!["axon:read".to_string(), "axon:write".to_string()],
            visibility_ceiling: Visibility::Internal,
        },
        Visibility::Internal,
        "test",
    );

    let outcome = crawl_start_with_context(
        &cfg,
        std::slice::from_ref(&cfg.start_url),
        &ctx,
        None,
        Some(&caller),
    )
    .await
    .expect("crawl_start_with_context should enqueue");

    assert_eq!(outcome.disposition, StartDisposition::Enqueued);
    assert_eq!(outcome.execution_mode, ExecutionMode::InProcess);
    assert_eq!(outcome.result.jobs.len(), 1);
    assert_eq!(outcome.result.jobs[0].url, cfg.start_url);

    let store = ctx.job_store().expect("unified job store must be attached");
    let job_id = uuid::Uuid::parse_str(&outcome.result.jobs[0].job_id).unwrap();
    let job = store
        .get(axon_api::source::JobId(job_id))
        .await
        .unwrap()
        .expect("job row must exist");
    assert_eq!(job.kind, axon_api::source::JobKind::Crawl);
}

/// Multiple URLs enqueue one unified job each.
#[tokio::test]
async fn crawl_start_with_context_enqueues_one_job_per_url() {
    let (cfg, ctx) = test_ctx_with_workers("https://docs.rs").await;
    let urls = vec![
        "https://docs.rs".to_string(),
        "https://docs.rs/other".to_string(),
    ];

    let outcome = crawl_start_with_context(&cfg, &urls, &ctx, None, None)
        .await
        .expect("crawl_start_with_context should enqueue both urls");

    assert_eq!(outcome.result.jobs.len(), 2);
    assert_eq!(outcome.result.job_ids.len(), 2);
    assert_ne!(outcome.result.job_ids[0], outcome.result.job_ids[1]);
}

/// Crawl now runs on the real unified worker (see `CrawlRunner` in
/// `runtime/job_runners/crawl_runner.rs`) via `job_service::job_status`/
/// `list_jobs`/etc. bridging onto the same store (see
/// `runtime/sqlite/crawl_bridge.rs`). A `.invalid` TLD (RFC 2606 — guaranteed
/// never to resolve) keeps this test network-free and fast: DNS resolution
/// fails with NXDOMAIN immediately rather than the connection-level timeout
/// an unroutable IP would incur, while still proving the unified worker
/// dispatches to the real `CrawlRunner`, not the `job_runner.unsupported_stage`
/// catch-all.
#[tokio::test]
async fn crawl_job_runs_end_to_end_and_is_claimed_promptly() {
    let (cfg, ctx) =
        test_ctx_with_workers("https://nonexistent-domain-test-axon.invalid/page").await;
    let started = std::time::Instant::now();
    let outcome =
        crawl_start_with_context(&cfg, std::slice::from_ref(&cfg.start_url), &ctx, None, None)
            .await
            .expect("enqueue");
    let job_id = uuid::Uuid::parse_str(&outcome.result.jobs[0].job_id).expect("job id");

    let mut status = None;
    for _ in 0..200 {
        let job = crate::jobs::job_status(&ctx, LegacyJobKind::Crawl, job_id)
            .await
            .expect("job_status")
            .expect("job exists");
        if job.status != "pending" && job.status != "running" {
            status = Some(job);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let job = status.expect("crawl job should reach a terminal status within timeout");
    let unsupported_stage = job
        .error_text
        .as_deref()
        .is_some_and(|text| text.contains("not wired yet"));
    assert!(
        !unsupported_stage,
        "crawl must dispatch to the real runner, not the catch-all: {:?}",
        job.error_text
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "crawl job took longer than a poll-interval-free path should — notify_unified() regression?"
    );

    let jobs = crate::jobs::list_jobs(&ctx, LegacyJobKind::Crawl, 10, 0)
        .await
        .expect("list_jobs");
    assert!(jobs.iter().any(|j| j.id == job_id));
}

#[tokio::test]
async fn crawl_start_snapshots_effective_max_pages_at_enqueue_boundary()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    async fn captured_max_pages(
        requested: u32,
        allow_unbounded: bool,
    ) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
        let (mut cfg, ctx) = test_ctx_with_workers("https://docs.rs").await;
        cfg.max_pages = requested;
        cfg.allow_unbounded_broad_crawl = allow_unbounded;

        let outcome = crawl_start_with_context(&cfg, &[cfg.start_url.clone()], &ctx, None, None)
            .await
            .map_err(|e| e.to_string())?;

        let store = ctx.job_store().expect("unified job store must be attached");
        let job_id = uuid::Uuid::parse_str(&outcome.result.jobs[0].job_id).unwrap();
        let request_json = store
            .request_json(axon_api::source::JobId(job_id))
            .await
            .map_err(|e| e.message)?
            .expect("request_json must be stored");
        let config_json = request_json
            .get("config_json")
            .and_then(|v| v.as_str())
            .expect("config_json field present");
        let snapshot: serde_json::Value =
            serde_json::from_str(config_json).expect("config_json is valid JSON");
        Ok(snapshot
            .pointer("/config/max_pages")
            .and_then(serde_json::Value::as_u64)
            .expect("crawl snapshot has max_pages") as u32)
    }

    assert_eq!(captured_max_pages(0, false).await?, 5_000);
    assert_eq!(captured_max_pages(50_000, false).await?, 5_000);
    assert_eq!(captured_max_pages(0, true).await?, 0);
    assert_eq!(captured_max_pages(50_000, true).await?, 50_000);
    Ok(())
}
