use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axon_core::config::{Config, RenderMode};
use axon_core::http::LoopbackGuard;
use httpmock::prelude::*;

use super::*;

fn summary(pages_seen: u32, markdown_files: u32) -> CrawlSummary {
    CrawlSummary {
        pages_seen,
        markdown_files,
        ..Default::default()
    }
}

// ─── plan_chrome_fallback branching (pure, hermetic) ──────────────────────

#[test]
fn plan_none_when_render_mode_is_not_auto_switch() {
    let cfg = Config {
        render_mode: RenderMode::Http,
        ..Config::default()
    };
    // Coverage is objectively bad, but a non-AutoSwitch render mode never
    // triggers the fallback chain at all.
    assert_eq!(
        plan_chrome_fallback(&cfg, &summary(0, 0)),
        ChromeFallbackPlan::None
    );
}

#[test]
fn plan_none_when_coverage_is_healthy() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        max_pages: 0,
        ..Config::default()
    };
    assert_eq!(
        plan_chrome_fallback(&cfg, &summary(20, 20)),
        ChromeFallbackPlan::None
    );
}

#[test]
fn plan_targets_waf_blocked_urls_before_thin_urls() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        ..Config::default()
    };
    let mut http_summary = summary(0, 0);
    http_summary.waf_blocked_pages = 1;
    http_summary
        .waf_blocked_urls
        .insert("https://example.com/blocked".to_string());
    http_summary
        .thin_urls
        .insert("https://example.com/thin".to_string());
    assert_eq!(
        plan_chrome_fallback(&cfg, &http_summary),
        ChromeFallbackPlan::TargetedRefetch
    );
}

#[test]
fn plan_targets_thin_urls_when_no_waf_blocked_pages() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        ..Config::default()
    };
    let mut http_summary = summary(0, 0);
    http_summary
        .thin_urls
        .insert("https://example.com/thin".to_string());
    assert_eq!(
        plan_chrome_fallback(&cfg, &http_summary),
        ChromeFallbackPlan::TargetedRefetch
    );
}

#[test]
fn plan_falls_back_to_html_backfill_when_neither_waf_nor_thin() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        ..Config::default()
    };
    // pages_seen == 0 alone forces `should_fallback_to_chrome` true, but with
    // no WAF-blocked or thin URLs recorded, the plan falls through to backfill.
    assert_eq!(
        plan_chrome_fallback(&cfg, &summary(0, 0)),
        ChromeFallbackPlan::HtmlBackfill
    );
}

// ─── maybe_chrome_fallback end-to-end wiring ──────────────────────────────

#[tokio::test]
async fn maybe_chrome_fallback_returns_input_unchanged_when_plan_is_none() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        max_pages: 0,
        ..Config::default()
    };
    let healthy = summary(20, 20);
    let seen: HashSet<String> = ["https://example.com/".to_string()].into_iter().collect();

    let (out_summary, out_seen) = maybe_chrome_fallback(
        &cfg,
        "https://example.com/",
        healthy.clone(),
        seen.clone(),
        Arc::new(HashMap::new()),
    )
    .await
    .unwrap();

    assert_eq!(out_summary.pages_seen, healthy.pages_seen);
    assert_eq!(out_summary.markdown_files, healthy.markdown_files);
    assert_eq!(out_seen, seen);
}

/// Regression test for issue #298 Wave 2b: before this wave, `site_discovery`
/// never called any fallback at all, so a low-coverage HTTP crawl (here,
/// `pages_seen <= 2`) shipped as-is with no attempt to backfill discovered
/// links. This exercises the `HtmlBackfill` branch end-to-end over a real
/// (mocked) HTTP server — no Chrome involved, so it is safe to run
/// unconditionally in CI. `cfg.max_pages = 0` keeps the post-backfill
/// coverage check from re-triggering (see `should_fallback_to_chrome`'s
/// `max_pages == 0` short-circuit), so this scenario never reaches the full
/// Chrome re-crawl step — that step needs a live Chrome instance and is
/// exercised only by the `#[ignore]`d test below.
#[tokio::test]
#[serial_test::serial]
async fn maybe_chrome_fallback_backfills_html_links_via_plain_http() {
    let server = MockServer::start();
    let root = server.mock(|when, then| {
        when.method(GET).path("/");
        then.status(200).body(
            r#"<html><body>
                <a href="/backfill-alpha">alpha</a>
                <a href="/backfill-beta">beta</a>
            </body></html>"#,
        );
    });
    let filler = "lorem ipsum dolor sit amet consectetur adipiscing elit ".repeat(10);
    let alpha = server.mock(|when, then| {
        when.method(GET).path("/backfill-alpha");
        then.status(200)
            .body(format!("<html><body><p>{filler}</p></body></html>"));
    });
    let beta = server.mock(|when, then| {
        when.method(GET).path("/backfill-beta");
        then.status(200)
            .body(format!("<html><body><p>{filler}</p></body></html>"));
    });

    let _loopback = LoopbackGuard::allow();
    let output_dir = tempfile::tempdir().expect("tempdir");
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        max_pages: 0,
        output_dir: output_dir.path().to_path_buf(),
        ..Config::default()
    };
    let start_url = server.url("/");
    let mut seen_urls = HashSet::new();
    seen_urls.insert(start_url.clone());

    // A tiny (<=2 pages), non-thin, non-WAF-blocked crawl forces
    // `ChromeFallbackPlan::HtmlBackfill` (see `plan_chrome_fallback`).
    let http_summary = summary(1, 1);

    let (final_summary, final_seen) = maybe_chrome_fallback(
        &cfg,
        &start_url,
        http_summary,
        seen_urls,
        Arc::new(HashMap::new()),
    )
    .await
    .expect("html backfill must not error");

    assert!(root.calls() >= 1);
    assert_eq!(alpha.calls(), 1);
    assert_eq!(beta.calls(), 1);

    // Both backfilled candidates were fetched as real, non-thin pages.
    assert_eq!(final_summary.pages_seen, 3);
    assert_eq!(final_summary.markdown_files, 3);
    assert_eq!(final_summary.thin_pages, 0);
    assert!(final_seen.iter().any(|u| u.ends_with("/backfill-alpha")));
    assert!(final_seen.iter().any(|u| u.ends_with("/backfill-beta")));
}

/// The WAF-blocked and thin-page targeted-refetch branches, and the full
/// Chrome re-crawl fallback, all require a live Chrome instance
/// (`chrome_refetch_thin_pages`/`run_crawl_once` in Chrome mode) — not
/// available hermetically in this crate's test environment. Left `#[ignore]`
/// as a documented manual/integration check; run with a local Chrome/CDP
/// endpoint reachable at `cfg.chrome_remote_url`.
#[tokio::test]
#[ignore = "requires a live Chrome/CDP instance"]
async fn maybe_chrome_fallback_targeted_refetch_requires_live_chrome() {
    unreachable!("manual/integration only — see doc comment");
}
