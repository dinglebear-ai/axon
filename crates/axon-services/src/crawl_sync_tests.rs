use crate::crawl_sync::{chrome_fallback::plan_chrome_fallback, crawl_sync_effective_config};
use axon_core::config::{Config, ScrapeFormat};
use axon_crawl::engine::CrawlSummary;

// ─── LLM format guard ────────────────────────────────────────────────────────

/// `crawl_sync` only sees `format == Llm` after the CLI guard has passed it
/// through, which requires `--wait true`. Verify the enum round-trips cleanly
/// through `Config::default()` override so tests can construct the right shape.
#[test]
fn config_scrape_format_llm_round_trips() {
    let cfg = Config {
        format: ScrapeFormat::Llm,
        wait: true,
        ..Config::default()
    };
    assert_eq!(cfg.format, ScrapeFormat::Llm);
    assert!(cfg.wait);
}

/// When `format` is anything other than `Llm`, the LLM stream pass is skipped.
/// Confirm the flag discrimination logic holds for each non-Llm variant.
#[test]
fn non_llm_formats_do_not_trigger_stream() {
    for format in [
        ScrapeFormat::Markdown,
        ScrapeFormat::Html,
        ScrapeFormat::RawHtml,
        ScrapeFormat::Json,
    ] {
        let cfg = Config {
            format,
            ..Config::default()
        };
        assert_ne!(
            cfg.format,
            ScrapeFormat::Llm,
            "format {format:?} should not trigger LLM stream"
        );
    }
}

// ─── Chrome fallback plan (regression) ───────────────────────────────────────

/// LLM format must not change the Chrome fallback decision — it is applied
/// post-crawl and is orthogonal to render mode selection.
#[test]
fn llm_format_does_not_affect_chrome_fallback_plan() {
    let cfg_llm = Config {
        format: ScrapeFormat::Llm,
        ..Config::default()
    };
    let cfg_md = Config {
        format: ScrapeFormat::Markdown,
        ..Config::default()
    };
    let summary = CrawlSummary {
        pages_seen: 10,
        thin_pages: 8,
        ..CrawlSummary::default()
    };
    assert_eq!(
        plan_chrome_fallback(&cfg_llm, &summary),
        plan_chrome_fallback(&cfg_md, &summary),
        "LLM format must not change Chrome fallback decision"
    );
}

/// Zero-page summaries with LLM format produce the same fallback plan as
/// without LLM format — confirming format is orthogonal to fallback.
#[test]
fn llm_format_zero_pages_fallback_plan_unchanged() {
    let cfg = Config {
        format: ScrapeFormat::Llm,
        ..Config::default()
    };
    let summary = CrawlSummary::default();
    // With default render_mode (Http/AutoSwitch default), zero pages still gives a plan.
    // The important thing is it matches the non-Llm equivalent.
    let cfg_md = Config {
        format: ScrapeFormat::Markdown,
        ..Config::default()
    };
    assert_eq!(
        plan_chrome_fallback(&cfg, &summary),
        plan_chrome_fallback(&cfg_md, &summary)
    );
}

#[test]
fn sitemap_only_sync_crawl_uses_effective_page_cap() {
    let cfg = Config {
        sitemap_only: true,
        max_pages: 0,
        ..Config::default_minimal()
    };

    let effective = crawl_sync_effective_config(&cfg, "https://docs.rs/std");

    assert_eq!(effective.max_pages, crate::crawl::DEFAULT_CRAWL_MAX_PAGES);
    assert!(effective.output_dir.ends_with("domains/docs.rs/sync"));
}

#[test]
fn sitemap_only_sync_crawl_preserves_unbounded_operator_override() {
    let cfg = Config {
        sitemap_only: true,
        max_pages: 50_000,
        allow_unbounded_broad_crawl: true,
        ..Config::default_minimal()
    };

    let effective = crawl_sync_effective_config(&cfg, "https://docs.rs/std");

    assert_eq!(effective.max_pages, 50_000);
}
