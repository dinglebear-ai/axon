use crate::crates::core::config::{Config, RenderMode};
use crate::crates::crawl::engine::CrawlSummary;
use crate::crates::services::crawl_sync::{ChromeFallbackPlan, plan_chrome_fallback};

#[test]
fn autoswitch_low_coverage_without_thin_urls_uses_html_backfill_before_chrome() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        drop_thin_markdown: true,
        ..Config::default()
    };
    let summary = CrawlSummary {
        pages_seen: 1,
        markdown_files: 1,
        ..CrawlSummary::default()
    };

    assert_eq!(
        plan_chrome_fallback(&cfg, &summary),
        ChromeFallbackPlan::HtmlBackfill
    );
}

#[test]
fn autoswitch_zero_pages_defaults_to_html_backfill() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        drop_thin_markdown: true,
        ..Config::default()
    };
    let summary = CrawlSummary::default();

    assert_eq!(
        plan_chrome_fallback(&cfg, &summary),
        ChromeFallbackPlan::HtmlBackfill
    );
}

#[test]
fn autoswitch_zero_markdown_files_defaults_to_html_backfill() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        drop_thin_markdown: true,
        ..Config::default()
    };
    let summary = CrawlSummary {
        pages_seen: 5,
        markdown_files: 0,
        ..CrawlSummary::default()
    };

    assert_eq!(
        plan_chrome_fallback(&cfg, &summary),
        ChromeFallbackPlan::HtmlBackfill
    );
}

#[test]
fn autoswitch_thin_ratio_equal_to_threshold_does_not_trigger_fallback() {
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        drop_thin_markdown: true,
        auto_switch_thin_ratio: 0.6,
        max_pages: 100,
        ..Config::default()
    };
    let summary = CrawlSummary {
        pages_seen: 10,
        thin_pages: 6,
        markdown_files: 10,
        ..CrawlSummary::default()
    };

    assert_eq!(
        plan_chrome_fallback(&cfg, &summary),
        ChromeFallbackPlan::None
    );
}
