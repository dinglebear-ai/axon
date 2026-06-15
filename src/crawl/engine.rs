mod adaptive;
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
use crate::core::logging::{log_done, log_info, log_warn};
use crate::crawl::manifest::ManifestEntry;
pub use adaptive::AdaptiveCrawlSnapshot;
use collector::{CollectorConfig, collect_crawl_pages};
use dir_ops::prepare_crawl_output_dir;
pub use dir_ops::update_latest_reflink;
use runtime::configure_website;
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
#[cfg(test)]
pub(crate) use url_utils::regex_escape;
pub(crate) use url_utils::{
    MapScope, canonicalize_url_for_dedupe, is_excluded_url_path, is_junk_discovered_url,
    normalize_map_candidate_url,
};
pub use waf::{WafDiagnostics, build_waf_diagnostics};

pub const MAX_CRAWL_DIAGNOSTICS: usize = 100;
const LEGACY_CRAWL_BROADCAST_BUFFER_MAX: usize = 16_384;

pub(crate) fn crawl_subscribe_buffer_size(cfg: &Config) -> usize {
    let min = cfg.crawl_broadcast_buffer_min.max(1);
    let max = cfg
        .crawl_broadcast_buffer_max
        .max(min)
        .max(LEGACY_CRAWL_BROADCAST_BUFFER_MAX);
    let desired = if cfg.max_pages == 0 {
        max
    } else {
        cfg.max_pages as usize
    };

    desired.clamp(min, max)
}

fn start_host(start_url: &str) -> Option<String> {
    url::Url::parse(start_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
}

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

/// Upper bound on the live per-page event ring carried in `CrawlSummary` and
/// persisted into the crawl job's `result_json` for the palette's log tail.
pub const MAX_CRAWL_EVENTS: usize = 60;

/// A single per-page fetch event surfaced to the live crawl view. Serialized into
/// `result_json.events` by the progress persister. `t` is milliseconds since the
/// collector started; the frontend renders `<t>ms fetch <url> → <status> · <n> links`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PageEvent {
    pub t: u64,
    pub url: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<u32>,
}

/// A host that returned 429 during the crawl, with the configured retry backoff.
/// Drives the "N hosts rate-limited · backing off Ns" banner.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RateLimitHost {
    pub host: String,
    pub backoff_ms: u64,
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
    /// Bounded ring of recent per-page fetch events for the live log tail.
    pub recent_events: Vec<PageEvent>,
    /// Hosts seen returning 429, with the configured backoff (for the banner).
    pub rate_limited: Vec<RateLimitHost>,
    /// Max crawl depth from config — the denominator of the DEPTH stat.
    pub depth_max: u32,
    pub adaptive: Option<AdaptiveCrawlSnapshot>,
}

impl CrawlSummary {
    pub fn push_diagnostic(&mut self, diagnostic: CrawlDiagnostic) {
        if self.diagnostics.len() < MAX_CRAWL_DIAGNOSTICS {
            self.diagnostics.push(diagnostic);
        }
    }

    /// Append a per-page event, evicting the oldest beyond `MAX_CRAWL_EVENTS`.
    pub fn push_event(&mut self, event: PageEvent) {
        if self.recent_events.len() >= MAX_CRAWL_EVENTS {
            self.recent_events.remove(0);
        }
        self.recent_events.push(event);
    }

    /// Record (or refresh the backoff of) a rate-limited host.
    pub fn note_rate_limited(&mut self, host: &str, backoff_ms: u64) {
        if host.is_empty() {
            return;
        }
        if let Some(existing) = self.rate_limited.iter_mut().find(|h| h.host == host) {
            existing.backoff_ms = backoff_ms;
        } else {
            self.rate_limited.push(RateLimitHost {
                host: host.to_string(),
                backoff_ms,
            });
        }
    }

    /// Pages discovered but not yet fetched (the live QUEUED backlog).
    pub fn queued(&self) -> u32 {
        self.pages_discovered.saturating_sub(self.pages_seen)
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
    log_info(&format!(
        "crawl start url={start_url} render_mode={mode:?} max_pages={} max_depth={}",
        cfg.max_pages, cfg.max_depth
    ));
    let total_start = Instant::now();

    let (_, recycling_bin) = prepare_crawl_dirs(cfg, start_url, output_dir).await?;

    let mut website = runtime::configure_website_with_crawl_id(cfg, start_url, mode, crawl_id)
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
