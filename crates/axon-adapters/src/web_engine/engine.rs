mod adaptive;
mod cdp_render;
mod collector;
mod dir_ops;
pub mod etag;
pub mod llms_txt;
pub mod map;
pub mod memory_guard;
mod runtime;
pub mod sitemap;
mod summary;
#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
mod thin_refetch;
mod url_utils;
mod waf;

use crate::web_engine::manifest::ManifestEntry;
pub use adaptive::AdaptiveCrawlSnapshot;
use axon_core::config::{Config, RenderMode};
use axon_core::content::{LadderThresholds, build_selector_config};
use axon_core::logging::{log_done, log_info, log_warn};
use collector::{CollectorConfig, collect_crawl_pages};
use dir_ops::prepare_crawl_output_dir;
pub use dir_ops::update_latest_reflink;
use spider::website::Website;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::Sender;

pub use llms_txt::{discover_llms_txt_urls, discover_llms_txt_urls_with_metadata};
pub use map::MapResult;
#[cfg(test)]
pub use map::{derive_map_scope, merge_map_candidate_urls};
pub use map::{discover_site_urls, discover_site_urls_with_metadata};
pub use runtime::resolve_cdp_ws_url;
pub use sitemap::append_candidate_backfill;
pub use sitemap::{BackfillStats, append_sitemap_backfill};
pub use sitemap::{SitemapDiscovery, discover_sitemap_urls};
pub use summary::{
    CrawlDiagnostic, CrawlSummary, MAX_CRAWL_DIAGNOSTICS, MAX_CRAWL_EVENTS, PageEvent,
    RateLimitHost,
};
pub use thin_refetch::chrome_refetch_thin_pages;
#[cfg(test)]
pub use url_utils::regex_escape;
pub use url_utils::{
    MapScope, canonicalize_url_for_dedupe, is_excluded_url_path, is_junk_discovered_url,
    normalize_map_candidate_url,
};
// Host-equality helpers stay crate-internal: they encode map/sitemap scoping
// policy, not a transport-facing contract.
pub(crate) use url_utils::{apex_host, hosts_match};
pub use waf::{WafDiagnostics, build_waf_diagnostics};

pub(crate) const MAX_TRACKED_DISCOVERED_URLS: usize = 50_000;

pub(crate) fn crawl_subscribe_buffer_size(cfg: &Config) -> usize {
    let min = cfg.crawl_broadcast_buffer_min.max(1);
    let max = cfg.crawl_broadcast_buffer_max.max(min);
    let desired = if cfg.max_pages == 0 {
        max
    } else {
        cfg.max_pages as usize
    };

    desired.clamp(min, max)
}

pub(crate) fn validate_crawl_memory_safety(cfg: &Config, start_url: &str) -> Result<(), String> {
    if cfg.max_pages > 0 || has_effective_scope(cfg, start_url) || cfg.allow_unbounded_broad_crawl {
        return Ok(());
    }

    Err(format!(
        "uncapped unscoped crawl rejected for {start_url}; set --max-pages, --budget, or --url-whitelist, or set AXON_ALLOW_UNBOUNDED_BROAD_CRAWL=true to override"
    ))
}

/// Whether the crawl is bounded by something other than a page cap: an explicit
/// path budget or URL whitelist, or the auto path-prefix scoping a deep start
/// URL receives at crawl time. Auto-scoping confines the crawl to the start
/// URL's path subtree (`derive_auto_whitelist_pattern`), so a deep-path
/// `--max-pages 0` crawl is bounded just like an explicit whitelist — and the
/// crawl memory guard backstops OOM within that subtree. Root (`/`) and
/// single-segment paths are not auto-scoped and remain rejected when uncapped.
fn has_effective_scope(cfg: &Config, start_url: &str) -> bool {
    !cfg.path_budgets.is_empty()
        || !cfg.url_whitelist.is_empty()
        || url_utils::derive_auto_whitelist_pattern(start_url).is_some()
}

fn crawl_control_id(crawl_id: Option<&str>) -> String {
    crawl_id
        .map(str::to_owned)
        .unwrap_or_else(|| format!("sync-{}", uuid::Uuid::new_v4()))
}

fn start_host(start_url: &str) -> Option<String> {
    url::Url::parse(start_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
}

pub fn should_fallback_to_chrome(summary: &CrawlSummary, max_pages: u32, cfg: &Config) -> bool {
    if summary.markdown_files == 0 {
        return true;
    }
    // A very-low-page crawl does not provide enough HTTP-only signal to judge
    // whether the captured content is complete, so give AutoSwitch one Chrome
    // retry even if the page is not technically "thin".
    if summary.pages_seen <= 2 {
        return true;
    }
    let thin_ratio = if summary.pages_seen == 0 {
        1.0
    } else {
        summary.thin_pages as f64 / summary.pages_seen as f64
    };
    if thin_ratio > cfg.auto_switch_thin_ratio {
        return true;
    }
    // When max_pages == 0 (uncapped), there's no expected page count to compare
    // against, so "low coverage" is meaningless — skip that check entirely.
    if max_pages == 0 {
        return false;
    }
    summary.markdown_files < (max_pages / 10).max(cfg.auto_switch_min_pages as u32)
}

fn configure_adaptive_crawl(
    cfg: &Config,
    website: &mut Website,
) -> Option<adaptive::AdaptiveCrawlControl> {
    let adaptive = adaptive::AdaptiveCrawlControl::from_config(cfg);
    if let Some(control) = adaptive.as_ref() {
        for warning in adaptive::warnings_for_config(cfg) {
            log_warn(&format!("[adaptive-concurrency] {warning}"));
        }
        control.attach_to(website);
    }
    adaptive
}

fn record_adaptive_summary(
    adaptive: &Option<adaptive::AdaptiveCrawlControl>,
    summary: &mut CrawlSummary,
) {
    if let Some(control) = adaptive.as_ref() {
        let snapshot = control.snapshot();
        log_info(&format!(
            "[adaptive-concurrency] crawl stats {}",
            snapshot.log_summary()
        ));
        summary.adaptive = Some(snapshot);
    }
}

fn inline_chrome_ws_url(cfg: &Config) -> Option<String> {
    // AutoSwitch starts with HTTP, but thin pages can still use inline Chrome
    // re-rendering when the original config requested AutoSwitch.
    if cfg.chrome_remote_local_policy {
        log_warn(
            "[Chrome] inline thin refetch disabled because remote-local-policy requires Spider interception",
        );
        return None;
    }
    matches!(cfg.render_mode, RenderMode::AutoSwitch).then(|| cfg.chrome_remote_url.clone())?
}

#[expect(
    clippy::too_many_arguments,
    reason = "crawl orchestration requires many config/state params"
)]
pub async fn run_crawl_once(
    cfg: &Config,
    start_url: &str,
    mode: RenderMode,
    output_dir: &Path,
    progress_tx: Option<Sender<CrawlSummary>>,
    run_sitemap: bool,
    previous_manifest: Arc<HashMap<String, ManifestEntry>>,
    crawl_id: Option<&str>,
) -> Result<(CrawlSummary, HashSet<String>), Box<dyn Error>> {
    validate_crawl_memory_safety(cfg, start_url).map_err(|e| -> Box<dyn Error> { e.into() })?;

    log_info(&format!(
        "crawl start url={start_url} render_mode={mode:?} max_pages={} max_depth={}",
        cfg.max_pages, cfg.max_depth
    ));
    let total_start = Instant::now();
    let control_id = crawl_control_id(crawl_id);
    let memory_guard = memory_guard::CrawlMemoryGuard::spawn(
        &control_id,
        start_url,
        cfg.crawl_memory_abort_percent,
    );

    let (_, recycling_bin) = prepare_crawl_dirs(cfg, start_url, output_dir).await?;

    let mut website =
        runtime::configure_website_with_crawl_id(cfg, start_url, mode, Some(&control_id))
            .await
            .map_err(|e| format!("failed to configure crawl website for {start_url}: {e}"))?;
    let adaptive = configure_adaptive_crawl(cfg, &mut website);

    // Conditional re-crawl seeding (bead axon_rust-hiyf): load persisted ETag
    // validators and seed spider's per-Website cache before the crawl so unchanged
    // pages 304 and are skipped. The seeded set drives post-crawl reconciliation of
    // those silent skips. Empty/absent sidecar → empty seed → no reconciliation.
    let (etag_previous_sidecar, etag_seeded_urls) =
        etag::load_and_seed(cfg, &mut website, output_dir).await;

    // Buffer at least max_pages worth of messages to prevent silent page drops
    // under high-throughput crawls. Profile-derived bounds keep the broadcast
    // ring large enough for fast profiles without making huge max_pages unbounded.
    let subscribe_buf = crawl_subscribe_buffer_size(cfg);
    let rx = website.subscribe(subscribe_buf);
    let markdown_dir = output_dir.join("markdown");
    let manifest_path = output_dir.join("manifest.jsonl");

    let min_chars = cfg.min_markdown_chars;
    let drop_thin = cfg.drop_thin_markdown;
    let exclude_path_prefix = cfg.exclude_path_prefix.clone();
    let start_host = start_host(start_url);
    let crawl_start = Instant::now();

    let inline_chrome_ws_url = inline_chrome_ws_url(cfg);

    let join = tokio::spawn(collect_crawl_pages(
        rx,
        CollectorConfig {
            markdown_dir,
            manifest_path,
            min_chars,
            drop_thin,
            exclude_path_prefix,
            include_subdomains: cfg.include_subdomains,
            start_host,
            scope: None,
            progress_tx,
            previous_manifest: Arc::clone(&previous_manifest),
            selector_config: build_selector_config(cfg),
            chrome_ws_url: inline_chrome_ws_url,
            chrome_timeout_secs: cfg.chrome_network_idle_timeout_secs,
            output_dir: output_dir.to_path_buf(),
            ladder_thresholds: LadderThresholds::from_config(cfg),
            antibot_max_scan_bytes: cfg.antibot_max_body_scan_bytes,
            structured_max_bytes: cfg.structured_data_max_bytes,
            max_depth: cfg.max_depth as u32,
            retry_backoff_ms: cfg.retry_backoff_ms,
            adaptive: adaptive.clone(),
            max_tracked_discovered_urls: MAX_TRACKED_DISCOVERED_URLS,
        },
    ));

    // Spider-native sitemap phase: pages flow through the live subscription above.
    // persist_links() carries accumulated sitemap links into the subsequent main crawl.
    // Both phases poll on their own task stack (see `fresh_stack`); the website
    // moves in and back out so ETag reconciliation below still sees the crawl.
    let sitemap_phase = run_sitemap && cfg.discover_sitemaps;
    let mut crawl_site = std::mem::take(&mut website);
    website = super::fresh_stack::crawl_on_fresh_stack(async move {
        if sitemap_phase {
            crawl_site.crawl_sitemap().await;
            crawl_site.persist_links();
        }
        match mode {
            RenderMode::Http => crawl_site.crawl_raw().await,
            RenderMode::Chrome => crawl_site.crawl().await,
            RenderMode::AutoSwitch => crawl_site.crawl_raw().await,
        }
        crawl_site
    })
    .await;
    website.unsubscribe();
    memory_guard.stop();

    let joined = join
        .await
        .map_err(|e| format!("collector join failure for {start_url}: {e}"));
    if let Some(reason) = memory_guard.abort_reason() {
        return Err(reason.into());
    }
    let (mut summary, urls) =
        joined?.map_err(|e| format!("collector failure for {start_url}: {e}"))?;
    summary.elapsed_ms = crawl_start.elapsed().as_millis();
    record_adaptive_summary(&adaptive, &mut summary);

    reconcile_etag_and_cleanup(
        cfg,
        output_dir,
        &recycling_bin,
        &previous_manifest,
        &etag_seeded_urls,
        &etag_previous_sidecar,
        &urls,
        &website,
        &mut summary,
    )
    .await?;

    log_done(&format!(
        "crawl done url={} pages_fetched={} duration_ms={}",
        start_url,
        summary.pages_seen,
        total_start.elapsed().as_millis()
    ));
    Ok((summary, urls))
}

async fn prepare_crawl_dirs(
    cfg: &Config,
    start_url: &str,
    output_dir: &Path,
) -> Result<(std::path::PathBuf, std::path::PathBuf), Box<dyn Error>> {
    let markdown_dir = output_dir.join("markdown");
    let recycling_bin = output_dir.join("markdown.old");
    prepare_crawl_output_dir(output_dir, &markdown_dir, &recycling_bin, cfg)
        .await
        .map_err(|e| {
            format!(
                "failed to prepare output dir {} for crawl of {start_url}: {e}",
                output_dir.display()
            )
        })?;
    Ok((markdown_dir, recycling_bin))
}

#[expect(
    clippy::too_many_arguments,
    reason = "post-crawl ETag reconciliation needs crawl state from the completed Website"
)]
async fn reconcile_etag_and_cleanup(
    cfg: &Config,
    output_dir: &Path,
    recycling_bin: &Path,
    previous_manifest: &Arc<HashMap<String, ManifestEntry>>,
    etag_seeded_urls: &HashSet<String>,
    etag_previous_sidecar: &HashMap<String, etag::EtagEntry>,
    urls: &HashSet<String>,
    website: &Website,
    summary: &mut CrawlSummary,
) -> Result<(), Box<dyn Error>> {
    // MUST run before the recycling bin is purged — reconciliation relinks reused
    // markdown out of markdown.old for genuine Spider 304 skips.
    if cfg.etag_conditional {
        let etag_visited: HashSet<String> = website
            .get_links()
            .iter()
            .filter_map(|u| canonicalize_url_for_dedupe(u.as_ref()))
            .collect();
        let reused = etag::reconcile_unmodified(
            output_dir,
            previous_manifest,
            etag_seeded_urls,
            urls,
            &etag_visited,
            etag_previous_sidecar,
        )
        .await;
        summary.reused_pages += reused as u32;
        etag::persist_next_sidecar(output_dir, website, etag_previous_sidecar, urls).await;
    }

    if dir_ops::path_exists(recycling_bin).await {
        tokio::fs::remove_dir_all(recycling_bin)
            .await
            .map_err(|e| {
                format!(
                    "failed to remove recycling bin {}: {e}",
                    recycling_bin.display()
                )
            })?;
        log_info("Purged recycling bin — armory is now synchronized with battlefield.");
    }
    Ok(())
}

/// Crawl only the sitemap — no follow-on main crawl.
/// Pages flow through the same subscription pipeline as `run_crawl_once`.
pub async fn run_sitemap_only(
    cfg: &Config,
    start_url: &str,
    output_dir: &Path,
    previous_manifest: Arc<HashMap<String, ManifestEntry>>,
) -> Result<(CrawlSummary, HashSet<String>), Box<dyn Error>> {
    validate_crawl_memory_safety(cfg, start_url).map_err(|e| -> Box<dyn Error> { e.into() })?;

    tokio::fs::create_dir_all(output_dir.join("markdown"))
        .await
        .map_err(|e| {
            format!("failed to create markdown dir for sitemap crawl of {start_url}: {e}")
        })?;

    let control_id = crawl_control_id(None);
    let memory_guard = memory_guard::CrawlMemoryGuard::spawn(
        &control_id,
        start_url,
        cfg.crawl_memory_abort_percent,
    );
    let mut website = runtime::configure_website_with_crawl_id(
        cfg,
        start_url,
        cfg.render_mode,
        Some(&control_id),
    )
    .await
    .map_err(|e| format!("failed to configure website for sitemap crawl of {start_url}: {e}"))?;
    // Override the default set by configure_website: sitemap IS the crawl here.
    website.with_ignore_sitemap(false);

    let subscribe_buf = crawl_subscribe_buffer_size(cfg);
    let rx = website.subscribe(subscribe_buf);
    let manifest_path = output_dir.join("manifest.jsonl");
    let markdown_dir = output_dir.join("markdown");
    let start_host = start_host(start_url);
    let crawl_start = Instant::now();

    let join = tokio::spawn(collect_crawl_pages(
        rx,
        CollectorConfig {
            markdown_dir,
            manifest_path,
            min_chars: cfg.min_markdown_chars,
            drop_thin: cfg.drop_thin_markdown,
            exclude_path_prefix: cfg.exclude_path_prefix.clone(),
            include_subdomains: cfg.include_subdomains,
            start_host,
            scope: None,
            progress_tx: None,
            previous_manifest: Arc::clone(&previous_manifest),
            selector_config: build_selector_config(cfg),
            // Sitemap-only crawl: no inline Chrome rendering (HTTP-only path).
            chrome_ws_url: None,
            chrome_timeout_secs: cfg.chrome_network_idle_timeout_secs,
            output_dir: output_dir.to_path_buf(),
            ladder_thresholds: LadderThresholds::from_config(cfg),
            antibot_max_scan_bytes: cfg.antibot_max_body_scan_bytes,
            structured_max_bytes: cfg.structured_data_max_bytes,
            max_depth: cfg.max_depth as u32,
            retry_backoff_ms: cfg.retry_backoff_ms,
            adaptive: None,
            max_tracked_discovered_urls: MAX_TRACKED_DISCOVERED_URLS,
        },
    ));

    website.crawl_sitemap().await;
    website.unsubscribe();
    memory_guard.stop();

    let joined = join
        .await
        .map_err(|e| format!("sitemap collector join failure for {start_url}: {e}"));
    if let Some(reason) = memory_guard.abort_reason() {
        return Err(reason.into());
    }
    let (mut summary, urls) =
        joined?.map_err(|e| format!("sitemap collector failure for {start_url}: {e}"))?;
    summary.elapsed_ms = crawl_start.elapsed().as_millis();

    Ok((summary, urls))
}
