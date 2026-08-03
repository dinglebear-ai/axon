use super::url_utils::{
    build_exclude_blacklist_patterns, derive_auto_whitelist_pattern, extract_link_host,
    is_junk_discovered_url,
};
use axon_core::config::parse::is_docker_service_host;
use axon_core::config::{Config, RenderMode};
use axon_core::http::{axon_ua, cdp_discovery_url, ssrf_blacklist_compact_strings};
use spider::CaseInsensitiveString;
use spider::configuration::RedirectPolicy;
use spider::url::Url;
use spider::utils::hedge::HedgeConfig;
use spider::website::Website;
use std::error::Error;
use std::path::Path;
use std::time::Duration;

/// Pre-resolve the Chrome DevTools WebSocket URL from the CDP discovery endpoint.
///
/// If `remote_url` is already a `ws://` / `wss://` URL (pre-resolved by the
/// bootstrap probe), return it directly without a second fetch — eliminating
/// the redundant `/json/version` round-trip when bootstrap succeeded.
///
/// Otherwise, fetch `/json/version`, extract `webSocketDebuggerUrl`, and rewrite
/// any known Docker service hostname (from the explicit allowlist) to `127.0.0.1`
/// so the host CLI can reach the Chrome proxy.
///
/// Returns `None` inside Docker (container hostnames resolve on the bridge
/// network) or when the fetch/parse fails.
pub async fn resolve_cdp_ws_url(remote_url: &str) -> Option<String> {
    // ws:// shortcut: bootstrap already resolved the URL — use it directly.
    if remote_url.starts_with("ws://") || remote_url.starts_with("wss://") {
        return Some(remote_url.to_string());
    }

    // Inside Docker the container hostname resolves on the Docker network.
    if Path::new("/.dockerenv").exists() {
        return None;
    }

    // Build the discovery URL (appends /json/version, converts ws→http).
    let discovery_url = cdp_discovery_url(remote_url)?;

    let client = axon_core::http::http_client().ok()?;

    let body: serde_json::Value = client
        .get(&discovery_url)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;

    let ws_url = body.get("webSocketDebuggerUrl")?.as_str()?;

    // Rewrite known Docker service hostnames to 127.0.0.1, preserving the port.
    let mut parsed = Url::parse(ws_url).ok()?;
    if let Some(host) = parsed.host_str() {
        let host = host.to_string();
        if is_docker_service_host(&host) {
            let _ = parsed.set_host(Some("127.0.0.1"));
        }
    }

    Some(parsed.to_string())
}

pub(super) fn apply_limit_and_behavior_settings(
    cfg: &Config,
    website: &mut Website,
    start_url: &str,
) {
    website.with_depth(cfg.max_depth);
    website.with_subdomains(cfg.include_subdomains);
    website.with_tld(false);
    // Surface each page's discovered links so the collector can compute a real
    // QUEUED backlog and per-page link counts for the live crawl view.
    website.with_return_page_links(true);
    if cfg.max_pages > 0 {
        website.with_limit(cfg.max_pages);
    }
    if !cfg.path_budgets.is_empty() {
        // Keys borrow from `cfg.path_budgets` (owned Strings on the Config,
        // which outlives this call) so spider's `HashMap<&str, u32>` is valid
        // for the duration of `with_budget`. Bead axon_rust-37zv.
        let budget: spider::hashbrown::HashMap<&str, u32> = cfg
            .path_budgets
            .iter()
            .map(|(path, cap)| (path.as_str(), *cap))
            .collect();
        website.with_budget(Some(budget));
    }
    if cfg.respect_robots {
        website.with_respect_robots_txt(true);
    }
    if let Some(limit) = cfg.crawl_concurrency_limit {
        website.with_concurrency_limit(Some(limit.max(1)));
    }
    if cfg.delay_ms > 0 {
        website.with_delay(cfg.delay_ms);
    }
    let mut blacklist_patterns = ssrf_blacklist_compact_strings().to_vec();
    if !cfg.exclude_path_prefix.is_empty() {
        blacklist_patterns.extend(
            build_exclude_blacklist_patterns(start_url, &cfg.exclude_path_prefix)
                .into_iter()
                .map(Into::into),
        );
    }
    website.with_blacklist_url(Some(blacklist_patterns));
}

pub(super) fn apply_request_and_identity_settings(
    cfg: &Config,
    website: &mut Website,
    start_url: &str,
) {
    // Pre-compute the allowed host for cross-domain link rejection.
    // When include_subdomains is false, links to other domains are dropped here
    // before spider even attempts to fetch them — preventing scope explosions
    // where external links (e.g. GitHub repos linked from docs) pull the crawl
    // across an entirely different domain and all of its outbound links.
    let allowed_host: Option<String> = if !cfg.include_subdomains {
        Url::parse(start_url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
    } else {
        None
    };

    website.set_on_link_find(move |url, html| {
        let url_str = url.as_ref();

        if is_junk_discovered_url(url_str) {
            return (CaseInsensitiveString::default(), None);
        }

        // Drop media assets (images, fonts, audio, video, archives, PDFs, …)
        // before they are queued, fetched, or embedded. spider's compile-time
        // perfect-hash classifier keys on the URL's file extension, so
        // `.html`/`.htm`/extensionless doc routes pass through unaffected.
        // Bead axon_rust-mk95.
        if spider::utils::media_asset::is_media_asset_url(url_str) {
            return (CaseInsensitiveString::default(), None);
        }

        // Reject cross-domain absolute URLs when include_subdomains is false.
        // Relative URLs have no host component and always pass through.
        if let Some(ref host) = allowed_host
            && let Some(link_host) = extract_link_host(url_str)
            && !link_host.eq_ignore_ascii_case(host)
        {
            return (CaseInsensitiveString::default(), None);
        }

        (url, html)
    });
    if let Some(timeout_ms) = cfg.request_timeout_ms {
        website.with_request_timeout(Some(Duration::from_millis(timeout_ms)));
    }
    if cfg.fetch_retries > 0 {
        website.with_retry(cfg.fetch_retries.min(u8::MAX as usize) as u8);
    }
    website.with_normalize(cfg.normalize);
    // Hedged requests: after 3 s a duplicate HTTP request races the original.
    // Whichever returns first wins; the loser is dropped. Transparent resilience
    // for slow or flaky HTTP endpoints with no extra configuration required.
    website.with_hedge(HedgeConfig::default());
    if let Some(ref proxy) = cfg.chrome_proxy {
        website.with_proxies(Some(vec![proxy.clone()]));
    }
    website.with_user_agent(Some(
        cfg.chrome_user_agent
            .as_deref()
            .unwrap_or_else(|| axon_ua()),
    ));
}

fn apply_custom_headers(cfg: &Config, website: &mut Website) {
    if cfg.custom_headers.is_empty() {
        return;
    }
    let map = axon_core::http::parse_custom_headers(&cfg.custom_headers);
    if !map.is_empty() {
        website.with_headers(Some(map));
    }
}

/// Configure a spider `Website` with an optional `crawl_id` for the control
/// feature. When set, `spider::utils::shutdown("{crawl_id}{url}")` can signal
/// an immediate graceful stop from inside the same process.
pub(super) async fn configure_website_with_crawl_id(
    cfg: &Config,
    start_url: &str,
    mode: RenderMode,
    crawl_id: Option<&str>,
) -> Result<Website, Box<dyn Error>> {
    let mut website = Website::new(start_url);
    if let Some(id) = crawl_id {
        website.with_crawl_id(id.to_string());
    }
    apply_limit_and_behavior_settings(cfg, &mut website, start_url);
    apply_request_and_identity_settings(cfg, &mut website, start_url);
    apply_custom_headers(cfg, &mut website);

    // Enable the spider control thread so in-process shutdown() can signal an
    // immediate stop. The crawl worker calls spider::utils::shutdown() when a
    // Redis cancel key is detected — this drains in-flight requests gracefully
    // instead of abruptly dropping the crawl future.
    website.with_no_control_thread(false);

    if cfg.cache {
        website.with_caching(true);
        if cfg.cache_http_only {
            website.with_cache_skip_browser(true);
        }
    }

    // Conditional re-crawl: enable spider's ETag cache so seeded validators drive
    // If-None-Match / If-Modified-Since requests. On a 304 spider drops the page
    // silently; the engine reconciles those drops back into the manifest after the
    // crawl (see src/crawl/engine/etag.rs). Crawl path only — single-page scrape
    // would lose content on a 304 and has no reconciliation seam. Bead
    // axon_rust-hiyf.
    if cfg.etag_conditional {
        website.configuration.with_etag_cache(true);
    }

    // WARC archive output: when --warc <path> is set, spider writes every
    // fetched page to a WARC 1.1 archive via the broadcast channel. HTTP and
    // Chrome render paths both archive identically. Crawl path only.
    if let Some(ref warc_path) = cfg.warc_output {
        website
            .configuration
            .with_warc(spider::utils::warc::WarcConfig {
                path: warc_path.to_string_lossy().to_string(),
                write_warcinfo: true,
                software: format!("axon/{}", env!("CARGO_PKG_VERSION")),
            });
    }

    // Chrome web-automation: run declarative per-path-prefix steps
    // (click/scroll/wait/fill/evaluate/…) against each page before capture.
    // Requires a Chrome render path; ignored (with a warning) on HTTP-only.
    if let Some(ref script_path) = cfg.automation_script {
        match mode {
            RenderMode::Chrome | RenderMode::AutoSwitch => {
                let scripts =
                    crate::web_engine::automation::load_automation_scripts(script_path).await?;
                axon_core::logging::log_info(&format!(
                    "loaded {} automation-script prefix(es) from {}",
                    scripts.len(),
                    script_path.display()
                ));
                // auto-switch is HTTP-first: the Chrome pass (and therefore the
                // automation steps) only runs on thin-content fallback, so a
                // content-rich site may never execute the scripts. Surface this
                // so it isn't a silent no-op — pass --render-mode chrome to force.
                if matches!(mode, RenderMode::AutoSwitch) {
                    axon_core::logging::log_warn(
                        "--automation-script is set with --render-mode auto-switch; \
                         automation only runs on the Chrome fallback pass, which fires \
                         only when the HTTP pass returns thin content. Pass \
                         --render-mode chrome to guarantee the steps run.",
                    );
                }
                website.with_automation_scripts(Some(scripts));
            }
            RenderMode::Http => {
                axon_core::logging::log_warn(
                    "--automation-script is set but --render-mode is http; \
                     web automation requires Chrome and will be skipped",
                );
            }
        }
    }

    website = super::super::browser::configure_spider_browser(
        cfg,
        website,
        mode,
        super::super::browser::BrowserTimeoutPolicy::FloorForBrowserWork,
    )
    .await?;

    // P3 — spider builder fields previously parsed but never applied.
    if !cfg.url_whitelist.is_empty() {
        website.with_whitelist_url(Some(
            cfg.url_whitelist
                .iter()
                .map(|s| spider::compact_str::CompactString::from(s.as_str()))
                .collect::<Vec<_>>(),
        ));
    } else if let Some(pattern) = derive_auto_whitelist_pattern(start_url) {
        // When no explicit whitelist is provided and the start URL has a deep
        // path (≥2 segments), auto-scope the crawl to that directory subtree.
        // This prevents inadvertently crawling the entire domain when the user's
        // intent is clearly a specific subsection (e.g. /api/python/google/generativeai/).
        axon_core::logging::log_info(&format!("auto-scoped crawl to path prefix: {pattern}"));
        website.with_whitelist_url(Some(vec![pattern]));
    }
    if cfg.block_assets {
        website.with_block_assets(true);
    }
    if let Some(max_bytes) = cfg.max_page_bytes {
        website.with_max_page_bytes(Some(max_bytes as f64));
    }
    if cfg.redirect_policy_strict {
        website.with_redirect_policy(RedirectPolicy::Strict);
    }

    // We always control the sitemap phase explicitly via run_crawl_once(run_sitemap: bool).
    // Prevent spider from auto-running sitemap during crawl()/crawl_raw().
    website.with_ignore_sitemap(true);

    Ok(website)
}
