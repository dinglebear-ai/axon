use crate::crates::core::config::Config;
use crate::crates::core::http::cdp_discovery_url;
use crate::crates::crawl::engine::resolve_cdp_ws_url;
use spider::configuration::Viewport;
use spider::features::chrome_common::{ScreenShotConfig, ScreenshotParams};
use spider::website::Website;
use std::error::Error;

/// Capture a screenshot using Spider's Chrome screenshot support with explicit
/// viewport and full_page parameters.
///
/// Called by both the CLI handler and the services layer so capture logic stays
/// in one place.
pub(crate) async fn spider_screenshot_with_options(
    cfg: &Config,
    url: &str,
    width: u32,
    height: u32,
    full_page: bool,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let remote_url = cfg
        .chrome_remote_url
        .as_deref()
        .ok_or("screenshot requires Chrome — set AXON_CHROME_REMOTE_URL")?;

    // Resolve the Chrome connection URL using the same logic as the crawl
    // engine: try the CDP WS discovery first, fall back to the discovery
    // URL or the raw remote URL.
    let chrome_url = match resolve_cdp_ws_url(remote_url).await {
        Some(ws_url) => ws_url,
        None => cdp_discovery_url(remote_url).unwrap_or_else(|| remote_url.to_string()),
    };

    let params = ScreenshotParams {
        full_page: Some(full_page),
        ..Default::default()
    };

    let screenshot_config = ScreenShotConfig::new(
        params, true,  // bytes — return PNG bytes on page.screenshot_bytes
        false, // save — we handle file writing ourselves
        None,  // output_dir — not needed since save=false
    );

    let mut website = Website::new(url);
    website.with_chrome_connection(Some(chrome_url));
    website.with_screenshot(Some(screenshot_config));
    website.with_viewport(Some(Viewport::new(width, height)));

    // Single page only — no crawling beyond the target URL.
    website.with_limit(1);
    website.with_depth(0);
    website.with_subdomains(false);

    // Wait for network idle so JS-rendered pages finish loading before capture.
    website.with_wait_for_idle_network0(Some(spider::configuration::WaitForIdleNetwork::new(
        Some(std::time::Duration::from_secs(
            cfg.chrome_network_idle_timeout_secs,
        )),
    )));

    // Dismiss browser dialogs that would otherwise block capture indefinitely.
    website.with_dismiss_dialogs(true);

    // Build the website config (required after Chrome settings).
    let mut website = website
        .build()
        .map_err(|_| format!("failed to build Spider website config for screenshot of {url}"))?;

    website.crawl().await;

    let pages = website
        .get_pages()
        .ok_or_else(|| format!("no pages returned from screenshot crawl of {url}"))?;

    let page = pages.first().ok_or_else(|| {
        format!("screenshot crawl of {url} returned zero pages — Chrome may not be reachable")
    })?;

    page.screenshot_bytes.clone().ok_or_else(|| {
        format!("screenshot bytes not captured for {url} — Chrome may not be reachable").into()
    })
}

/// Sanitize a URL into a safe screenshot filename.
///
/// Strips the scheme, replaces non-alphanumeric chars with hyphens,
/// collapses runs of hyphens, trims edges, and truncates to 120 chars.
pub(crate) fn url_to_screenshot_filename(url: &str, idx: usize) -> String {
    let stripped = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    let sanitized: String = stripped
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();

    // Collapse consecutive hyphens and trim leading/trailing hyphens.
    let mut collapsed = String::with_capacity(sanitized.len());
    let mut prev_hyphen = true; // Start true to trim leading hyphens.
    for c in sanitized.chars() {
        if c == '-' {
            if !prev_hyphen {
                collapsed.push('-');
            }
            prev_hyphen = true;
        } else {
            collapsed.push(c);
            prev_hyphen = false;
        }
    }
    let collapsed = collapsed.trim_end_matches('-');

    // Truncate to a reasonable filename length.
    let max_name = 120;
    let name = if collapsed.len() > max_name {
        &collapsed[..max_name]
    } else {
        collapsed
    };

    format!("{idx:04}-{name}.png")
}
