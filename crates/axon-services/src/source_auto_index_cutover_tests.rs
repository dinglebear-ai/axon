use std::sync::Arc;

use axon_api::source::{JobKind, JobListRequest, JobSummary, SourceScope};
use axon_core::config::Config;

use crate::context::ServiceContext;
use crate::search_source_index::enqueue_web_source_auto_index;

async fn context_with_store(cfg: Config) -> ServiceContext {
    ServiceContext::new(Arc::new(cfg))
        .await
        .expect("service context")
}

fn cfg_with_temp_store() -> Config {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config::test_default();
    cfg.sqlite_path = dir.path().join("jobs.db");
    std::mem::forget(dir);
    cfg
}

async fn jobs_by_kind(ctx: &ServiceContext, kind: JobKind) -> Vec<JobSummary> {
    ctx.job_store()
        .expect("unified job store")
        .list(JobListRequest {
            status: None,
            kind: Some(kind),
            source_id: None,
            watch_id: None,
            limit: Some(100),
            cursor: None,
        })
        .await
        .expect("list jobs")
        .items
}

#[tokio::test]
async fn search_auto_index_enqueues_page_scoped_source_job_not_crawl() {
    let mut cfg = cfg_with_temp_store();
    cfg.custom_headers = vec!["Authorization: Bearer secret".to_string()];
    cfg.url_whitelist = vec![".*".to_string()];
    let ctx = context_with_store(cfg.clone()).await;

    let job = enqueue_web_source_auto_index(
        &cfg,
        &ctx,
        "http://93.184.216.34/",
        SourceScope::Page,
        1,
        0,
        true,
        "search",
    )
    .await
    .expect("auto-index enqueue");

    assert_eq!(jobs_by_kind(&ctx, JobKind::Source).await.len(), 1);
    assert!(jobs_by_kind(&ctx, JobKind::Crawl).await.is_empty());

    let request_json = ctx
        .job_store()
        .expect("unified job store")
        .request_json(job.id)
        .await
        .expect("request_json")
        .expect("request json stored");
    let source_request = request_json
        .get("source_request")
        .expect("source_request payload");
    assert_eq!(
        source_request.get("scope").and_then(|v| v.as_str()),
        Some("page")
    );
    assert_eq!(
        source_request
            .pointer("/limits/max_pages")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        source_request
            .pointer("/limits/max_depth")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        source_request.pointer("/metadata/headers_policy"),
        Some(&serde_json::json!("stripped"))
    );
    assert!(
        source_request
            .pointer("/options/values/custom_headers")
            .is_none(),
        "source auto-index must not carry caller headers"
    );
}

#[tokio::test]
async fn source_auto_index_rejects_tailscale_target_before_enqueue() {
    let cfg = cfg_with_temp_store();
    let ctx = context_with_store(cfg.clone()).await;

    let err = enqueue_web_source_auto_index(
        &cfg,
        &ctx,
        "http://100.120.242.29/internal",
        SourceScope::Page,
        1,
        0,
        true,
        "search",
    )
    .await
    .expect_err("private address should be rejected");

    let rendered = err.to_string();
    assert!(
        rendered.contains("blocked") || rendered.contains("not global"),
        "unexpected ssrf rejection: {rendered}"
    );
    assert!(jobs_by_kind(&ctx, JobKind::Source).await.is_empty());
    assert!(jobs_by_kind(&ctx, JobKind::Crawl).await.is_empty());
}
