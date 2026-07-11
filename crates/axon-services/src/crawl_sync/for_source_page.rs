//! Single-page acquisition for `axon source <web-url>` when `scope = Page`.
//!
//! `crawl_for_source` (in `for_source.rs`) always runs a full site crawl —
//! that is correct for `Site`/`Docs` scope but wrong for `Page`: a `scope =
//! Page` request must acquire exactly the one URL, with no link following and
//! no sitemap/llms.txt backfill. This module fetches + transforms a single
//! page (the same `scrape` path `axon scrape` uses) and writes it as a
//! single-entry crawl-output manifest/markdown pair, so the rest of the web
//! source bridge (`crates/axon-services/src/web_source.rs`, which reads
//! `manifest_path`/`markdown_root`) is unchanged and scope-agnostic.

use std::error::Error;

use axon_core::config::Config;
use axon_core::content::url_to_stable_filename;
use axon_crawl::manifest::ManifestEntry;
use sha2::{Digest, Sha256};

use super::for_source::{
    CrawlForSourceResult, crawl_output_manifest_and_markdown, crawl_sync_output_dir,
    effective_crawl_config_for_source,
};

/// Fetch exactly one page (`start_url`) — no crawling, no link following — and
/// write it as a single-entry crawl-output manifest/markdown pair at the same
/// paths a full-site crawl would use, so `WebSourceAdapter::discover` reads it
/// identically.
///
/// Thin-page filtering still applies: when `cfg.drop_thin_markdown` (default
/// `true`) and the transformed markdown is under `cfg.min_markdown_chars`, no
/// manifest entry or markdown file is written and the result reports zero
/// pages/markdown files — mirroring the crawl engine's own thin-page skip
/// (`crates/axon-crawl/src/engine/collector/page.rs::PageOutcome::Thin`).
pub async fn crawl_for_source_page(
    cfg: &Config,
    start_url: &str,
) -> Result<CrawlForSourceResult, Box<dyn Error>> {
    // `max_pages` is meaningless for a single-page fetch; reuse the shared
    // embed-disable rule from `effective_crawl_config_for_source` so the web
    // bridge (not this acquisition step) owns vectorization.
    let page_cfg = effective_crawl_config_for_source(cfg, None);

    let scraped = axon_crawl::scrape::scrape_to_result(&page_cfg, start_url).await?;
    let markdown = scraped.markdown;
    let chars = markdown.len();

    let output_dir = crawl_sync_output_dir(&page_cfg.output_dir, start_url);
    let (manifest_path, markdown_root) = crawl_output_manifest_and_markdown(&output_dir);
    tokio::fs::create_dir_all(markdown_root.join("markdown")).await?;

    if page_cfg.drop_thin_markdown && chars < page_cfg.min_markdown_chars {
        // Write an empty manifest so downstream manifest reads see a present,
        // zero-entry file rather than a missing-file error.
        tokio::fs::write(&manifest_path, b"").await?;
        return Ok(CrawlForSourceResult {
            output_dir,
            manifest_path,
            markdown_root,
            pages_seen: 1,
            markdown_files: 0,
        });
    }

    let filename = url_to_stable_filename(&scraped.url);
    let relative_path = format!("markdown/{filename}");
    let content_hash = Sha256::digest(markdown.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    let entry = ManifestEntry {
        url: scraped.url.clone(),
        relative_path: relative_path.clone(),
        markdown_chars: chars,
        content_hash: Some(content_hash),
        changed: true,
        structured: None,
    };

    tokio::fs::write(markdown_root.join(&relative_path), markdown.as_bytes()).await?;
    let mut manifest_line = serde_json::to_string(&entry)?;
    manifest_line.push('\n');
    tokio::fs::write(&manifest_path, manifest_line.as_bytes()).await?;

    Ok(CrawlForSourceResult {
        output_dir,
        manifest_path,
        markdown_root,
        pages_seen: 1,
        markdown_files: 1,
    })
}

#[cfg(test)]
#[path = "for_source_page_tests.rs"]
mod tests;
