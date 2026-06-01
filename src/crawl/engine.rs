mod cdp_render;
mod collector;
mod dir_ops;
pub(crate) mod etag;
pub(crate) mod llms_txt;
pub(crate) mod map;
mod runtime;
pub(crate) mod sitemap;
#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
mod thin_refetch;
mod url_utils;
mod waf;

use crate::core::config::{Config, RenderMode};
use crate::core::content::{LadderThresholds, build_selector_config};
use crate::core::logging::{log_done, log_info};
use crate::crawl::manifest::ManifestEntry;
use collector::{CollectorConfig, collect_crawl_pages};
use dir_ops::prepare_crawl_output_dir;
pub use dir_ops::update_latest_reflink;
use runtime::configure_website;
#[cfg(test)]
use spider::website::Website;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::Sender;

pub(crate) use llms_txt::discover_llms_txt_urls;
pub use map::MapResult;
pub(crate) use map::{append_html_anchor_backfill, map_with_sitemap};
#[cfg(test)]
pub(crate) use map::{derive_map_scope, merge_map_candidate_urls};
pub(crate) use runtime::resolve_cdp_ws_url;
pub(crate) use sitemap::append_candidate_backfill;
pub use sitemap::{BackfillStats, append_sitemap_backfill};
pub(crate) use sitemap::{SitemapDiscovery, discover_sitemap_urls};
pub(crate) use thin_refetch::chrome_refetch_thin_pages;
pub(crate) use url_utils::{
    MapScope, canonicalize_url_for_dedupe, is_excluded_url_path, normalize_map_candidate_url,
};
#[cfg(test)]
pub(crate) use url_utils::{is_junk_discovered_url, regex_escape};
pub use waf::{WafDiagnostics, build_waf_diagnostics};

pub const MAX_CRAWL_DIAGNOSTICS: usize = 100;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CrawlDiagnostic {
    pub phase: String,
    pub class: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dropped: Option<u64>,
}

impl CrawlDiagnostic {
    pub fn new(
        phase: impl Into<String>,
        class: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            phase: phase.into(),
            class: class.into(),
            message: message.into(),
            url: None,
            http_status: None,
            dropped: None,
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn with_http_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    pub fn with_dropped(mut self, dropped: u64) -> Self {
        self.dropped = Some(dropped);
        self
    }
}

#[derive(Debug, Default, Clone)]
pub struct CrawlSummary {
    pub pages_seen: u32,
    pub markdown_files: u32,
    pub thin_pages: u32,
    pub reused_pages: u32,
    pub pages_discovered: u32,
    pub elapsed_ms: u128,
    /// Canonical URLs of pages that were below `min_markdown_chars`.
    /// Populated by the collector and used by the auto-switch path to
    /// perform targeted per-URL Chrome re-fetches instead of a full re-crawl.
    pub thin_urls: HashSet<String>,
    /// Pages skipped due to non-2xx HTTP status codes.
    pub error_pages: u32,
    /// Pages blocked by a WAF or anti-bot system (`waf_check || blocked_crawl`).
    pub waf_blocked_pages: u32,
    /// Canonical URLs of WAF-blocked pages; used for targeted stealth Chrome retry.
    pub waf_blocked_urls: HashSet<String>,
    /// Bounded diagnostic samples for operator-facing `axon crawl errors`.
    pub diagnostics: Vec<CrawlDiagnostic>,
}

impl CrawlSummary {
    pub fn push_diagnostic(&mut self, diagnostic: CrawlDiagnostic) {
        if self.diagnostics.len() < MAX_CRAWL_DIAGNOSTICS {
            self.diagnostics.push(diagnostic);
        }
    }
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
    log_info(&format!(
        "crawl start url={} render_mode={:?} max_pages={} max_depth={}",
        start_url, mode, cfg.max_pages, cfg.max_depth
    ));
    let total_start = Instant::now();

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

    let mut website = runtime::configure_website_with_crawl_id(cfg, start_url, mode, crawl_id)
        .await
        .map_err(|e| format!("failed to configure crawl website for {start_url}: {e}"))?;

    // Conditional re-crawl seeding (bead axon_rust-hiyf): load persisted ETag
    // validators and seed spider's per-Website cache before the crawl so unchanged
    // pages 304 and are skipped. The seeded set drives post-crawl reconciliation of
    // those silent skips. Empty/absent sidecar → empty seed → no reconciliation.
    let (etag_previous_sidecar, etag_seeded_urls) =
        etag::load_and_seed(cfg, &mut website, output_dir).await;

    // Buffer at least max_pages worth of messages to prevent silent page drops
    // under high-throughput crawls (extreme/max profiles). Clamp to 16 384 so
    // a large --max-pages value can't allocate an unbounded broadcast ring buffer.
    let subscribe_buf = (cfg.max_pages as usize).clamp(4096, 16_384);
    let rx = website.subscribe(subscribe_buf);
    let markdown_dir = output_dir.join("markdown");
    let manifest_path = output_dir.join("manifest.jsonl");

    let min_chars = cfg.min_markdown_chars;
    let drop_thin = cfg.drop_thin_markdown;
    let exclude_path_prefix = cfg.exclude_path_prefix.clone();
    let crawl_start = Instant::now();

    // Enable inline Chrome re-rendering when the *config* requests AutoSwitch,
    // even though `mode` is `Http` for the initial crawl phase (AutoSwitch
    // always starts with HTTP — `resolve_initial_mode` converts AutoSwitch→Http).
    // Chrome mode does its own rendering; Http mode with no AutoSwitch intent
    // has no Chrome target. When cfg.render_mode is AutoSwitch and Chrome is
    // configured, thin pages are re-rendered immediately while the HTTP crawl
    // continues — no second pass needed.
    let inline_chrome_ws_url = if matches!(cfg.render_mode, RenderMode::AutoSwitch) {
        cfg.chrome_remote_url.clone()
    } else {
        None
    };

    let join = tokio::spawn(collect_crawl_pages(
        rx,
        CollectorConfig {
            markdown_dir,
            manifest_path,
            min_chars,
            drop_thin,
            exclude_path_prefix,
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
        },
    ));

    // Spider-native sitemap phase: pages flow through the live subscription above.
    // persist_links() carries accumulated sitemap links into the subsequent main crawl.
    if run_sitemap && cfg.discover_sitemaps {
        website.crawl_sitemap().await;
        website.persist_links();
    }

    match mode {
        RenderMode::Http => website.crawl_raw().await,
        RenderMode::Chrome | RenderMode::AutoSwitch => website.crawl().await,
    }
    website.unsubscribe();

    let (mut summary, urls) = join
        .await
        .map_err(|e| format!("collector join failure for {start_url}: {e}"))?
        .map_err(|e| format!("collector failure for {start_url}: {e}"))?;
    summary.elapsed_ms = crawl_start.elapsed().as_millis();

    // Conditional re-crawl reconciliation + persistence (bead axon_rust-hiyf).
    // MUST run before the recycling bin is purged below — reconciliation relinks
    // reused markdown out of markdown.old. `urls` already contains every URL that
    // arrived in the broadcast (inserted before the status check), so seeded URLs
    // absent from it are exactly spider's silent 304 skips.
    if cfg.etag_conditional {
        // Gate reconciliation on spider's visited set so only genuine 304 skips
        // (URLs spider actually scheduled + fetched this run) are reused — never
        // pages that are no longer discovered (PR #153 review; bead axon_rust-hiyf).
        // Canonicalize into the same key space as `urls`/`previous_manifest`.
        let etag_visited: HashSet<String> = website
            .get_links()
            .iter()
            .filter_map(|u| canonicalize_url_for_dedupe(u.as_ref()))
            .collect();
        let reused = etag::reconcile_unmodified(
            output_dir,
            &previous_manifest,
            &etag_seeded_urls,
            &urls,
            &etag_visited,
        )
        .await;
        summary.reused_pages += reused as u32;
        etag::persist_next_sidecar(output_dir, &website, &etag_previous_sidecar, &urls).await;
    }

    if dir_ops::path_exists(&recycling_bin).await {
        tokio::fs::remove_dir_all(&recycling_bin)
            .await
            .map_err(|e| {
                format!(
                    "failed to remove recycling bin {}: {e}",
                    recycling_bin.display()
                )
            })?;
        log_info("Purged recycling bin — armory is now synchronized with battlefield.");
    }

    log_done(&format!(
        "crawl done url={} pages_fetched={} duration_ms={}",
        start_url,
        summary.pages_seen,
        total_start.elapsed().as_millis()
    ));
    Ok((summary, urls))
}

/// Crawl only the sitemap — no follow-on main crawl.
/// Pages flow through the same subscription pipeline as `run_crawl_once`.
pub async fn run_sitemap_only(
    cfg: &Config,
    start_url: &str,
    output_dir: &Path,
    previous_manifest: Arc<HashMap<String, ManifestEntry>>,
) -> Result<(CrawlSummary, HashSet<String>), Box<dyn Error>> {
    tokio::fs::create_dir_all(output_dir.join("markdown"))
        .await
        .map_err(|e| {
            format!("failed to create markdown dir for sitemap crawl of {start_url}: {e}")
        })?;

    let mut website = configure_website(cfg, start_url, cfg.render_mode)
        .await
        .map_err(|e| {
            format!("failed to configure website for sitemap crawl of {start_url}: {e}")
        })?;
    // Override the default set by configure_website: sitemap IS the crawl here.
    website.with_ignore_sitemap(false);

    let subscribe_buf = (cfg.max_pages as usize).clamp(4096, 16_384);
    let rx = website.subscribe(subscribe_buf);
    let manifest_path = output_dir.join("manifest.jsonl");
    let markdown_dir = output_dir.join("markdown");
    let crawl_start = Instant::now();

    let join = tokio::spawn(collect_crawl_pages(
        rx,
        CollectorConfig {
            markdown_dir,
            manifest_path,
            min_chars: cfg.min_markdown_chars,
            drop_thin: cfg.drop_thin_markdown,
            exclude_path_prefix: cfg.exclude_path_prefix.clone(),
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
        },
    ));

    website.crawl_sitemap().await;
    website.unsubscribe();

    let (mut summary, urls) = join
        .await
        .map_err(|e| format!("sitemap collector join failure for {start_url}: {e}"))?
        .map_err(|e| format!("sitemap collector failure for {start_url}: {e}"))?;
    summary.elapsed_ms = crawl_start.elapsed().as_millis();

    Ok((summary, urls))
}
