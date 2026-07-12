//! `Site`/`Docs` scope discovery: enumerate a site's URLs by driving
//! `axon-crawl`'s engine directly (issue #298 Wave 1b — the adapter owns
//! discovery, there is no `axon-services` crawl pre-pass anymore).
//!
//! The crawl writes to a throwaway [`tempfile::TempDir`] that is never handed
//! to `acquire` or any caller — it exists only so the existing
//! `axon_crawl::engine`/`manifest` machinery (which is disk-oriented) has
//! somewhere to write while this function reads back the resulting
//! `manifest.jsonl` to build in-memory `ManifestItem`s. `acquire` independently
//! re-fetches each changed item's content through the provider boundary, so
//! this crawl's own fetched bodies are discarded — a deliberate "correctness
//! over cache" tradeoff (see issue #298 Wave 1b task notes): reusing them
//! would require threading a content cache between `discover` and `acquire`,
//! reintroducing the very disk-handoff "second pipeline" this wave retires.
//!
//! This single crawl pass does not replicate `crawl_sync`'s full multi-stage
//! Chrome-fallback machinery (WAF-blocked targeted refetch, HTML anchor
//! backfill, thin-page targeted refetch) — see `crawl_sync::chrome_fallback`.
//! It runs one pass at the resolved initial render mode. A follow-up (#298
//! Wave 2) could port that fallback chain in if link-discovery fidelity on
//! JS-heavy sites under `auto_switch` becomes a problem in practice.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axon_api::source::*;

use crate::adapter::Result;

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

    let initial_mode = axon_crawl::chrome_bootstrap::resolve_initial_mode(&cfg);
    let bootstrap = axon_crawl::chrome_bootstrap::bootstrap_chrome_runtime(&cfg).await;
    if let Some(ws_url) = bootstrap.resolved_ws_url {
        cfg.chrome_remote_url = Some(ws_url);
    }

    // `run_sitemap = false`: mirrors `axon-services::crawl_sync::run_crawl_phase`
    // — sitemap discovery runs as a separate, more controlled pass
    // (`backfill_sitemap_urls` below) after the main crawl rather than
    // Spider's inline `crawl_sitemap()` phase.
    let (mut summary, seen_urls) = axon_crawl::engine::run_crawl_once(
        &cfg,
        &start_url,
        initial_mode,
        &cfg.output_dir,
        None,
        false,
        Arc::new(HashMap::new()),
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

    if cfg.discover_sitemaps {
        backfill_sitemap_urls(&cfg, &start_url, &seen_urls, &mut summary).await;
    }

    let manifest_path = cfg.output_dir.join("manifest.jsonl");
    let entries = axon_crawl::manifest::read_manifest_data(&manifest_path)
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
    summary: &mut axon_crawl::engine::CrawlSummary,
) {
    let manifest_path = cfg.output_dir.join("manifest.jsonl");
    let manifest_urls = axon_crawl::manifest::read_manifest_urls(&manifest_path)
        .await
        .unwrap_or_default();
    let merged_seen: HashSet<String> = seen_urls.iter().cloned().chain(manifest_urls).collect();
    let _ = axon_crawl::engine::append_sitemap_backfill(
        cfg,
        start_url,
        &cfg.output_dir,
        &merged_seen,
        summary,
    )
    .await;
}
