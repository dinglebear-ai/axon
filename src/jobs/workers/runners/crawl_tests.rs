use super::{
    CrawlBudget, backfill_enabled, build_crawl_result_json, crawl_timeout_duration,
    merge_candidates, run_crawl_job, validate_within_budget,
};
use crate::core::config::Config;
use crate::crawl::engine::{AdaptiveCrawlSnapshot, CrawlDiagnostic, CrawlSummary};
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
    summary.push_event(crate::crawl::engine::PageEvent {
        t: 42,
        url: "https://example.com/docs".to_string(),
        status: 200,
        links: Some(3),
    });
    summary.note_rate_limited("example.com", 250);
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
        0,
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
    assert_eq!(obj.get("queued").and_then(|v| v.as_u64()), Some(2));
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
    assert_eq!(
        obj.get("events").and_then(|v| v.as_array()).map(Vec::len),
        Some(1)
    );
    assert_eq!(
        obj.get("rate_limited")
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
        0,
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
        0,
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
        "queued",
        "thin_md",
        "error_pages",
        "waf_blocked_pages",
        "diagnostic_count",
        "diagnostic_counts",
        "diagnostics",
        "events",
        "rate_limited",
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
        0,
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
fn crawl_result_json_includes_adaptive_concurrency_snapshot() {
    let mut summary = make_summary();
    summary.adaptive = Some(AdaptiveCrawlSnapshot {
        successes: 11,
        failures: 3,
        lag_events: 1,
        syncs: 4,
        current_target: 2,
        available_permits: 1,
    });

    let json = build_crawl_result_json(
        "https://example.com",
        Path::new("/tmp/axon-crawl"),
        Path::new("/tmp/axon-crawl"),
        0,
        &summary,
        None,
        None,
        None,
    );
    let adaptive = json
        .get("adaptive_concurrency")
        .expect("adaptive telemetry");

    assert_eq!(
        adaptive.get("current_target").and_then(|v| v.as_u64()),
        Some(2)
    );
    assert_eq!(
        adaptive.get("available_permits").and_then(|v| v.as_u64()),
        Some(1)
    );
    assert_eq!(adaptive.get("successes").and_then(|v| v.as_u64()), Some(11));
    assert_eq!(adaptive.get("failures").and_then(|v| v.as_u64()), Some(3));
    assert_eq!(adaptive.get("lag_events").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(adaptive.get("syncs").and_then(|v| v.as_u64()), Some(4));
}

#[test]
fn crawl_result_json_preserves_caller_path_and_worker_path() {
    let json = build_crawl_result_json(
        "https://example.com",
        Path::new("/home/axon/.axon/output/domains/example.com/job"),
        Path::new("/home/jmagar/.axon/output/domains/example.com/job"),
        0,
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

#[test]
fn merge_candidates_unions_and_dedupes() {
    let s = vec!["https://x.com/a".to_string(), "https://x.com/b".to_string()];
    let l = vec!["https://x.com/b".to_string(), "https://x.com/c".to_string()];
    let out = merge_candidates(s, l);
    assert_eq!(out.len(), 3, "b deduped across sitemap + llms");
}

/// Regression guard: the sitemap contribution must NEVER be truncated by an llms-derived
/// cap. On `main`, `append_sitemap_backfill` passed every discovered sitemap URL to backfill
/// with no url-count cap; `merge_candidates` must preserve that. Here the sitemap set is far
/// larger than `max_llms_txt_urls` would be — all sitemap URLs survive the merge.
#[test]
fn merge_candidates_never_drops_sitemap_urls() {
    let sitemap: Vec<String> = (0..1000).map(|i| format!("https://x.com/s/{i}")).collect();
    let llms = vec!["https://x.com/llms-only".to_string()];
    let out = merge_candidates(sitemap.clone(), llms);
    assert_eq!(
        out.len(),
        1001,
        "all 1000 sitemap URLs must survive plus the 1 llms URL — no blanket cap"
    );
    for s in &sitemap {
        assert!(out.contains(s), "sitemap URL {s} must not be dropped");
    }
}

/// The `i64` seconds knob maps to an `Option<Duration>` the helper consumes:
/// `0` disables (None), positive values enable, negatives are treated as
/// disabled (defensive — a negative timeout is meaningless).
#[test]
fn crawl_timeout_duration_maps_seconds_to_option() {
    assert_eq!(crawl_timeout_duration(0), None);
    assert_eq!(
        crawl_timeout_duration(7200),
        Some(std::time::Duration::from_secs(7200))
    );
    assert_eq!(crawl_timeout_duration(-5), None);
}

/// The backfill gate is an OR, not an AND: with sitemaps disabled but llms.txt enabled,
/// backfill must still fire so the `/llms.txt` candidates are fetched.
#[test]
fn backfill_gate_fires_for_llms_only() {
    let both_off = Config {
        discover_sitemaps: false,
        discover_llms_txt: false,
        ..Config::default()
    };
    assert!(!backfill_enabled(&both_off), "both off → no backfill");

    let llms_only = Config {
        discover_sitemaps: false,
        discover_llms_txt: true,
        ..Config::default()
    };
    assert!(
        backfill_enabled(&llms_only),
        "sitemaps off + llms on → backfill must still fire"
    );

    let sitemap_only = Config {
        discover_sitemaps: true,
        discover_llms_txt: false,
        ..Config::default()
    };
    assert!(
        backfill_enabled(&sitemap_only),
        "sitemaps on + llms off → backfill fires"
    );
}

/// The wall-clock budget bounds URL validation: with an already-elapsed deadline,
/// `validate_within_budget` aborts with the crawl-timeout error instead of running
/// DNS to completion. This pins the ordering fix — `run_crawl_job` anchors the
/// budget *before* validation, so a slow/hung lookup counts against
/// `crawl_job_timeout_secs`. A regression that moved the budget back below
/// validation (handing the engine a fresh full budget) would fail here.
#[tokio::test]
async fn validate_within_budget_aborts_on_already_elapsed_deadline() {
    let budget = CrawlBudget {
        deadline: Some(tokio::time::Instant::now() - std::time::Duration::from_secs(1)),
        secs: 7200,
    };
    // `.invalid` is reserved (RFC 6761) and never resolves, so validation's DNS
    // lookup stays pending and the already-passed deadline wins deterministically.
    let err = validate_within_budget("https://does-not-exist.invalid/x", None, budget)
        .await
        .expect_err("an already-elapsed deadline must abort validation");
    assert!(
        err.to_string().contains("7200"),
        "expected the crawl-timeout error naming the limit, got: {err}"
    );
}
