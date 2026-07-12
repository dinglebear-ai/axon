//! `Site`/`Docs` scope discovery: enumerate a site's URLs by driving the
//! in-crate `web_engine`'s engine directly (issue #298 Wave 1b — the adapter
//! owns discovery, there is no `axon-services` crawl pre-pass anymore; the
//! engine itself relocated in-crate in Wave 2a).
//!
//! The crawl writes to a throwaway [`tempfile::TempDir`] that is never handed
//! to `acquire` or any caller — it exists only so the existing
//! `crate::web_engine::engine`/`manifest` machinery (which is disk-oriented) has
//! somewhere to write while this function reads back the resulting
//! `manifest.jsonl` to build in-memory `ManifestItem`s. `acquire` independently
//! re-fetches each changed item's content through the provider boundary, so
//! this crawl's own fetched bodies are discarded — a deliberate "correctness
//! over cache" tradeoff (see issue #298 Wave 1b task notes): reusing them
//! would require threading a content cache between `discover` and `acquire`,
//! reintroducing the very disk-handoff "second pipeline" this wave retires.
//!
//! The initial HTTP-mode crawl pass runs through `super::chrome_fallback`'s
//! `maybe_chrome_fallback`, which replicates `crawl_sync`'s multi-stage
//! Chrome-fallback machinery (WAF-blocked targeted refetch, thin-page
//! targeted refetch, HTML anchor backfill, and — if coverage is still low —
//! a full Chrome re-crawl) — see `super::chrome_fallback` module docs and
//! `crawl_sync::chrome_fallback` for the CLI-path twin this ports (issue #298
//! Wave 2b).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axon_api::source::*;

use crate::adapter::Result;

use super::chrome_fallback::maybe_chrome_fallback;
use super::manifest_items::web_manifest_item;
use super::options::build_discovery_config;
use super::url_parts::WebUrlParts;

pub(super) async fn crawl_manifest_items(plan: &SourcePlan) -> Result<Vec<ManifestItem>> {
    let start_url = plan.route.source.canonical_uri.clone();
    let temp_dir = tempfile::tempdir().map_err(|err| {
        ApiError::new(
            "adapter.web.discover.tempdir_failed",
            axon_error::ErrorStage::Discovering,
            err.to_string(),
        )
    })?;
    let mut cfg = build_discovery_config(plan, temp_dir.path().to_path_buf());

    let initial_mode = crate::web_engine::chrome_bootstrap::resolve_initial_mode(&cfg);
    let bootstrap = crate::web_engine::chrome_bootstrap::bootstrap_chrome_runtime(&cfg).await;
    if let Some(ws_url) = bootstrap.resolved_ws_url {
        cfg.chrome_remote_url = Some(ws_url);
    }

    let previous_manifest = Arc::new(HashMap::new());

    // `run_sitemap = false`: mirrors `axon-services::crawl_sync::run_crawl_phase`
    // — sitemap discovery runs as a separate, more controlled pass
    // (`backfill_sitemap_urls` below) after the main crawl rather than
    // Spider's inline `crawl_sitemap()` phase.
    let (http_summary, http_seen_urls) = crate::web_engine::engine::run_crawl_once(
        &cfg,
        &start_url,
        initial_mode,
        &cfg.output_dir,
        None,
        false,
        Arc::clone(&previous_manifest),
        None,
    )
    .await
    .map_err(|err| {
        ApiError::new(
            "adapter.web.discover.crawl_failed",
            axon_error::ErrorStage::Discovering,
            err.to_string(),
        )
    })?;

    let (mut summary, seen_urls) = maybe_chrome_fallback(
        &cfg,
        &start_url,
        http_summary,
        http_seen_urls,
        previous_manifest,
    )
    .await
    .map_err(|err| {
        ApiError::new(
            "adapter.web.discover.chrome_fallback_failed",
            axon_error::ErrorStage::Discovering,
            err.to_string(),
        )
    })?;

    if cfg.discover_sitemaps {
        backfill_sitemap_urls(&cfg, &start_url, &seen_urls, &mut summary).await;
    }

    let manifest_path = cfg.output_dir.join("manifest.jsonl");
    let entries = crate::web_engine::manifest::read_manifest_data(&manifest_path)
        .await
        .map_err(|err| {
            ApiError::new(
                "adapter.web.discover.manifest_read_failed",
                axon_error::ErrorStage::Discovering,
                err.to_string(),
            )
        })?;

    let mut items = Vec::with_capacity(entries.len());
    for entry in entries.into_values() {
        let web = WebUrlParts::parse(&entry.url)?;
        items.push(web_manifest_item(
            plan,
            &web,
            entry.content_hash,
            Some(entry.markdown_chars as u64),
            entry.structured,
        ));
    }
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));
    Ok(items)
}

/// Best-effort sitemap.xml backfill, mirroring `crawl_sync`'s
/// `run_sitemap_backfill`. Failures are swallowed (logged upstream by
/// `append_sitemap_backfill` itself) — a missing/unreachable sitemap must not
/// fail discovery of the pages the main crawl already found.
async fn backfill_sitemap_urls(
    cfg: &axon_core::config::Config,
    start_url: &str,
    seen_urls: &HashSet<String>,
    summary: &mut crate::web_engine::engine::CrawlSummary,
) {
    let manifest_path = cfg.output_dir.join("manifest.jsonl");
    let manifest_urls = crate::web_engine::manifest::read_manifest_urls(&manifest_path)
        .await
        .unwrap_or_default();
    let merged_seen: HashSet<String> = seen_urls.iter().cloned().chain(manifest_urls).collect();
    let _ = crate::web_engine::engine::append_sitemap_backfill(
        cfg,
        start_url,
        &cfg.output_dir,
        &merged_seen,
        summary,
    )
    .await;
}
