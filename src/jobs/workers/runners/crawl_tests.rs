use super::{build_crawl_result_json, run_crawl_job};
use crate::core::config::Config;
use crate::crawl::engine::{CrawlDiagnostic, CrawlSummary};
use crate::jobs::backend::JobPayload;
use crate::jobs::ops::enqueue_job;
use crate::jobs::store::open_sqlite_pool;
use std::path::Path;

fn make_summary() -> CrawlSummary {
    let mut summary = CrawlSummary {
        pages_seen: 7,
        markdown_files: 5,
        pages_discovered: 9,
        thin_pages: 2,
        error_pages: 1,
        waf_blocked_pages: 0,
        elapsed_ms: 1234,
        ..CrawlSummary::default()
    };
    summary.push_diagnostic(
        CrawlDiagnostic::new("http_fetch", "http_status", "skipped page with HTTP 500")
            .with_url("https://example.com/broken")
            .with_http_status(500),
    );
    summary
}

#[tokio::test]
async fn run_crawl_job_rejects_blocked_url_before_crawl() {
    let pool = open_sqlite_pool(":memory:").await.expect("pool");
    let cfg = Config::default_minimal();
    let id = enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: "http://127.0.0.1/".to_string(),
            config_json: "{}".to_string(),
        },
        &cfg,
    )
    .await
    .expect("enqueue");

    let err = run_crawl_job(&pool, &cfg, id, None, None)
        .await
        .expect_err("blocked URL should fail before crawl");

    assert!(
        err.to_string().contains("private/reserved range"),
        "expected SSRF validation error, got: {err}"
    );
}

#[test]
fn crawl_result_json_uses_canonical_keys() {
    let json = build_crawl_result_json(
        "https://example.com",
        Path::new("/tmp/axon-crawl"),
        Path::new("/tmp/axon-crawl"),
        &make_summary(),
        Some("embed-job-id"),
        None,
        None,
    );
    let obj = json.as_object().expect("json is an object");

    assert_eq!(
        obj.get("url").and_then(|v| v.as_str()),
        Some("https://example.com")
    );
    assert_eq!(obj.get("pages_crawled").and_then(|v| v.as_u64()), Some(7));
    assert_eq!(obj.get("md_created").and_then(|v| v.as_u64()), Some(5));
    assert_eq!(
        obj.get("pages_discovered").and_then(|v| v.as_u64()),
        Some(9)
    );
    assert_eq!(obj.get("thin_md").and_then(|v| v.as_u64()), Some(2));
    assert_eq!(obj.get("error_pages").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(
        obj.get("waf_blocked_pages").and_then(|v| v.as_u64()),
        Some(0)
    );
    assert_eq!(
        obj.get("diagnostic_count").and_then(|v| v.as_u64()),
        Some(1)
    );
    assert_eq!(
        obj.get("diagnostic_counts")
            .and_then(|v| v.get("http_fetch:http_status"))
            .and_then(|v| v.as_u64()),
        Some(1)
    );
    assert_eq!(
        obj.get("diagnostics")
            .and_then(|v| v.as_array())
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(obj.get("elapsed_ms").and_then(|v| v.as_u64()), Some(1234));
    assert_eq!(
        obj.get("embed_job_id").and_then(|v| v.as_str()),
        Some("embed-job-id")
    );
    assert_eq!(
        obj.get("output_dir").and_then(|v| v.as_str()),
        Some("/tmp/axon-crawl")
    );
    assert_eq!(
        obj.get("output_path").and_then(|v| v.as_str()),
        Some("/tmp/axon-crawl/markdown")
    );
}

#[test]
fn crawl_result_json_omits_legacy_aliases() {
    let json = build_crawl_result_json(
        "https://example.com",
        Path::new("/tmp/axon-crawl"),
        Path::new("/tmp/axon-crawl"),
        &make_summary(),
        None,
        None,
        None,
    );
    let obj = json.as_object().expect("json is an object");

    assert!(
        !obj.contains_key("pages_seen"),
        "legacy alias pages_seen must not appear in crawl result JSON"
    );
    assert!(
        !obj.contains_key("markdown_files"),
        "legacy alias markdown_files must not appear in crawl result JSON"
    );
}

#[test]
fn crawl_result_json_required_keys() {
    let json = build_crawl_result_json(
        "https://example.com",
        Path::new("/tmp/axon-crawl"),
        Path::new("/tmp/axon-crawl"),
        &make_summary(),
        None,
        None,
        None,
    );
    let obj = json.as_object().expect("json is an object");
    for key in [
        "url",
        "output_dir",
        "output_path",
        "pages_crawled",
        "md_created",
        "pages_discovered",
        "thin_md",
        "error_pages",
        "waf_blocked_pages",
        "diagnostic_count",
        "diagnostic_counts",
        "diagnostics",
        "elapsed_ms",
        "embed_job_id",
    ] {
        assert!(obj.contains_key(key), "required key missing: {key}");
    }
    assert!(
        !obj.contains_key("embed_deferred"),
        "embed_deferred must be absent when embed was not deferred"
    );
}

#[test]
fn crawl_result_json_includes_embed_deferred_when_capacity_exceeded() {
    let json = build_crawl_result_json(
        "https://example.com",
        Path::new("/tmp/axon-crawl"),
        Path::new("/tmp/axon-crawl"),
        &make_summary(),
        None,
        Some("embed queue at capacity: 50/50 pending embed jobs"),
        None,
    );
    let obj = json.as_object().expect("json is an object");
    assert_eq!(obj.get("embed_job_id").and_then(|v| v.as_str()), None);
    assert_eq!(
        obj.get("embed_deferred").and_then(|v| v.as_str()),
        Some("embed queue at capacity: 50/50 pending embed jobs"),
        "capacity-deferred embed must surface a reason in result_json"
    );
}

#[test]
fn crawl_result_json_preserves_caller_path_and_worker_path() {
    let json = build_crawl_result_json(
        "https://example.com",
        Path::new("/home/axon/.axon/output/domains/example.com/job"),
        Path::new("/home/jmagar/.axon/output/domains/example.com/job"),
        &make_summary(),
        None,
        None,
        Some("sitemap fetch failed"),
    );
    let obj = json.as_object().expect("json is an object");
    assert_eq!(
        obj.get("output_dir").and_then(|v| v.as_str()),
        Some("/home/jmagar/.axon/output/domains/example.com/job")
    );
    assert_eq!(
        obj.get("worker_output_dir").and_then(|v| v.as_str()),
        Some("/home/axon/.axon/output/domains/example.com/job")
    );
    assert_eq!(
        obj.get("sitemap_backfill_error").and_then(|v| v.as_str()),
        Some("sitemap fetch failed")
    );
}
