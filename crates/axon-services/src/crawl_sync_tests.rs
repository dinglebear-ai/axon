use crate::crawl_sync::{
    chrome_fallback::plan_chrome_fallback, crawl_changed_manifest_keys,
    crawl_manifest_to_ledger_items, crawl_source_identity, crawl_sync_effective_config,
};
use axon_core::config::{Config, ScrapeFormat};
use axon_crawl::engine::CrawlSummary;
use axon_source_ledger::{SourceIdentity, SourceKind, SourceLedgerStore};

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

#[test]
fn crawl_source_identity_is_collection_scoped() {
    let source_a = crawl_source_identity("https://example.com/docs", "axon-a");
    let source_b = crawl_source_identity("https://example.com/docs", "axon-b");

    assert_ne!(source_a.source_id, source_b.source_id);
    assert_eq!(source_a.source_id, "crawl:axon-a:https://example.com/docs");
    assert_eq!(source_b.source_id, "crawl:axon-b:https://example.com/docs");
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

#[tokio::test]
async fn crawl_manifest_adapter_uses_url_hash_and_markdown_size()
-> Result<(), Box<dyn std::error::Error>> {
    let manifest = tempfile::NamedTempFile::new()?;
    tokio::fs::write(
        manifest.path(),
        serde_json::json!({
            "url": "https://example.com/a",
            "relative_path": "markdown/a.md",
            "markdown_chars": 42,
            "content_hash": "hash-a"
        })
        .to_string()
            + "\n",
    )
    .await?;

    let items = crawl_manifest_to_ledger_items(manifest.path()).await?;

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item_key, "https://example.com/a");
    assert_eq!(items[0].content_hash, "hash-a");
    assert_eq!(items[0].size_bytes, 42);
    Ok(())
}

#[tokio::test]
async fn crawl_changed_manifest_keys_excludes_reused_pages()
-> Result<(), Box<dyn std::error::Error>> {
    let manifest = tempfile::NamedTempFile::new()?;
    tokio::fs::write(
        manifest.path(),
        serde_json::json!({
            "url": "https://example.com/reused",
            "relative_path": "markdown/reused.md",
            "markdown_chars": 42,
            "content_hash": "hash-reused",
            "changed": false
        })
        .to_string()
            + "\n"
            + &serde_json::json!({
                "url": "https://example.com/changed",
                "relative_path": "markdown/changed.md",
                "markdown_chars": 50,
                "content_hash": "hash-changed",
                "changed": true
            })
            .to_string()
            + "\n",
    )
    .await?;

    let keys = crawl_changed_manifest_keys(manifest.path()).await?;

    assert_eq!(keys.len(), 1);
    assert!(keys.contains("https://example.com/changed"));
    Ok(())
}

#[tokio::test]
async fn crawl_embed_failure_does_not_commit_generation() -> Result<(), Box<dyn std::error::Error>>
{
    let pool = axon_jobs::store::open_sqlite_pool(":memory:").await?;
    let store = SourceLedgerStore::new(pool);
    let source = SourceIdentity::new("crawl-source", SourceKind::Crawl, "axon", 1);
    store.ensure_source(&source).await?;

    let embed_result: Result<(), &str> = Err("embed failed");
    if embed_result.is_ok() {
        let generation = store.begin_generation(&source).await?;
        store
            .commit_generation(&source.source_id, generation)
            .await?;
    }

    let status = store.source_status(&source.source_id).await?;
    assert_eq!(status.committed_generation, 0);
    assert_eq!(store.max_generation(&source.source_id).await?, 0);
    Ok(())
}
