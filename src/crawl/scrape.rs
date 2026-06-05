use crate::core::config::{Config, RenderMode};
use crate::core::content::{
    build_selector_config, extract_meta_description, find_between, to_markdown,
};
use crate::core::http::{
    axon_ua, build_ssrf_guarded_client_builder, normalize_url, ssrf_blacklist_compact_strings,
    validate_url,
};
use crate::core::logging::log_warn;
use spider::website::Website;
use spider_transformations::transformation::content::SelectorConfiguration;
use std::error::Error;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::time::sleep;

/// Emit the `accept_invalid_certs` security warning exactly once per process
/// regardless of how many URLs are scraped in a session.
fn warn_invalid_certs_once() {
    static WARNED: OnceLock<()> = OnceLock::new();
    WARNED.get_or_init(|| {
        log_warn(
            "accept_invalid_certs is enabled — TLS certificate validation is disabled. \
             This exposes all connections to MITM attacks.",
        );
    });
}

// ── Spider Website configuration ─────────────────────────────────────────────

/// Build a Spider Website configured for a single-page scrape.
///
/// Applies SSRF blacklist, timeout, retry, user-agent, and limit=1 so Spider
/// never follows links beyond the target page.
pub(crate) fn build_scrape_website(cfg: &Config, url: &str) -> Result<Website, Box<dyn Error>> {
    let ssrf_patterns = ssrf_blacklist_compact_strings().to_vec();

    let mut website = Website::new(url);
    // Single page only — do not follow any discovered links.
    website.with_limit(1);
    // Block image/CSS/JS assets; we only want the HTML document.
    website.with_block_assets(true);
    // Wire SSRF blacklist patterns so Spider's internal redirect-following
    // cannot reach private ranges even if the seed URL resolves to one.
    website.with_blacklist_url(Some(ssrf_patterns));

    if let Some(timeout_ms) = cfg.request_timeout_ms {
        website.with_request_timeout(Some(Duration::from_millis(timeout_ms)));
    }
    // with_retry takes u8; cfg.fetch_retries is usize — clamp to u8::MAX (255).
    let retries = cfg.fetch_retries.min(u8::MAX as usize) as u8;
    website.with_retry(retries);

    website.with_user_agent(Some(
        cfg.chrome_user_agent
            .as_deref()
            .unwrap_or_else(|| axon_ua()),
    ));
    if let Some(proxy) = cfg.chrome_proxy.as_deref() {
        website.with_proxies(Some(vec![proxy.to_string()]));
    }
    // Wire custom headers so `--header` works for single-page scrapes too.
    if !cfg.custom_headers.is_empty() {
        let map = crate::core::http::parse_custom_headers(&cfg.custom_headers);
        if !map.is_empty() {
            website.with_headers(Some(map));
        }
    }
    // Apply the same safe defaults as configure_website().
    website.with_no_control_thread(true);
    if cfg.accept_invalid_certs {
        warn_invalid_certs_once();
        website.with_danger_accept_invalid_certs(true);
    }
    if matches!(cfg.render_mode, RenderMode::Chrome) {
        website.with_dismiss_dialogs(true);
        website.configuration.disable_log = true;
        if cfg.bypass_csp {
            website.with_csp_bypass(true);
        }
    }

    Ok(website)
}

// ── Page fetching and matching ───────────────────────────────────────────────

#[derive(Debug)]
pub(crate) struct ScrapedPage {
    pub url: String,
    pub html: String,
    pub status_code: u16,
}

fn canonical_url_for_match(input: &str) -> String {
    input
        .split('#')
        .next()
        .unwrap_or(input)
        .split('?')
        .next()
        .unwrap_or(input)
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn host_from_url(input: &str) -> Option<&str> {
    let (_, rest) = input.split_once("://")?;
    Some(rest.split('/').next().unwrap_or(rest))
}

fn last_path_segment(input: &str) -> Option<&str> {
    let without_fragment = input.split('#').next().unwrap_or(input);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    without_query.split('/').rfind(|s| !s.is_empty())
}

pub(crate) fn page_matches_requested_url(requested_url: &str, page_url: &str) -> bool {
    let requested_canon = canonical_url_for_match(requested_url);
    let page_canon = canonical_url_for_match(page_url);
    if requested_canon == page_canon {
        return true;
    }

    // docs.rs and similar doc hosts often redirect `/latest/.../foo.html` to
    // a concrete version path while preserving the terminal file name.
    if let (Some(req_host), Some(page_host), Some(req_last), Some(page_last)) = (
        host_from_url(&requested_canon),
        host_from_url(&page_canon),
        last_path_segment(&requested_canon),
        last_path_segment(&page_canon),
    ) {
        return req_host.eq_ignore_ascii_case(page_host)
            && req_last.eq_ignore_ascii_case(page_last)
            && req_last.contains(".html");
    }

    false
}

pub(crate) fn pick_best_page_for_url(
    requested_url: &str,
    mut candidates: Vec<ScrapedPage>,
) -> Option<ScrapedPage> {
    if let Some(index) = candidates
        .iter()
        .position(|p| page_matches_requested_url(requested_url, &p.url))
    {
        return Some(candidates.swap_remove(index));
    }
    candidates.into_iter().next()
}

/// Build a `reqwest::Client` with SSRF-safe redirect policy and optional
/// config-driven settings (timeout, TLS bypass, proxy, UA, headers).
///
/// When no config-specific overrides are active, callers should prefer the
/// shared `http_client()` singleton. This constructor exists for the cases
/// where `accept_invalid_certs`, proxy, or custom headers require a
/// purpose-built client.
fn build_scrape_fallback_client(cfg: &Config) -> Result<reqwest::Client, Box<dyn Error>> {
    let mut builder =
        build_ssrf_guarded_client_builder(cfg.request_timeout_ms.map(Duration::from_millis));
    if cfg.accept_invalid_certs {
        warn_invalid_certs_once();
        builder = builder.danger_accept_invalid_certs(true);
    }
    builder = builder.user_agent(
        cfg.chrome_user_agent
            .as_deref()
            .unwrap_or_else(|| axon_ua()),
    );
    if let Some(proxy) = cfg.chrome_proxy.as_deref().filter(|p| !p.trim().is_empty()) {
        builder = builder.proxy(reqwest::Proxy::all(proxy)?);
    }
    if !cfg.custom_headers.is_empty() {
        let map = crate::core::http::parse_custom_headers(&cfg.custom_headers);
        if !map.is_empty() {
            builder = builder.default_headers(map);
        }
    }
    builder = builder.redirect(reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 {
            return attempt.error("too many redirects");
        }
        let url = attempt.url().as_str().to_string();
        if validate_url(&url).is_err() {
            attempt.error(format!("SSRF: redirect to blocked URL {url}"))
        } else {
            attempt.follow()
        }
    }));
    Ok(builder.build()?)
}

pub(crate) async fn direct_fetch_requested_page(
    cfg: &Config,
    requested_url: &str,
) -> Result<ScrapedPage, Box<dyn Error>> {
    let client = build_scrape_fallback_client(cfg)?;
    let attempts = cfg.fetch_retries.saturating_add(1).max(1);
    let mut last_err: Option<String> = None;
    for attempt in 1..=attempts {
        match client.get(requested_url).send().await {
            Ok(resp) => {
                let status_code = resp.status().as_u16();
                let html = resp.text().await?;
                return Ok(ScrapedPage {
                    url: requested_url.to_string(),
                    html,
                    status_code,
                });
            }
            Err(err) => {
                last_err = Some(err.to_string());
                if attempt < attempts {
                    sleep(Duration::from_millis(cfg.retry_backoff_ms)).await;
                }
            }
        }
    }
    Err(format!(
        "direct fetch fallback failed for {requested_url}: {}",
        last_err.unwrap_or_else(|| "unknown error".to_string())
    )
    .into())
}

/// Fetch a single page from a configured Spider `Website`.
///
/// Uses explicit `subscribe()` + `crawl_raw()`/`crawl()` instead of Spider's
/// `scrape_raw()`. This is the correct approach — not a workaround. Spider's
/// `scrape_raw()` uses a biased-select internally: for fast single-page fetches
/// the done channel fires before the page receiver gets a turn, so `get_pages()`
/// comes back empty. Owning the subscription ourselves avoids this race entirely.
pub(crate) async fn fetch_single_page(
    cfg: &Config,
    website: &mut Website,
    requested_url: &str,
) -> Result<ScrapedPage, Box<dyn Error>> {
    let mut rx = website.subscribe(16);
    // Spawn the collector BEFORE the crawl so it is ready to receive the broadcast.
    let collect: tokio::task::JoinHandle<Vec<ScrapedPage>> = tokio::spawn(async move {
        let mut pages = Vec::new();
        while let Ok(page) = rx.recv().await {
            pages.push(ScrapedPage {
                url: page.get_url().to_string(),
                html: page.get_html(),
                status_code: page.status_code.as_u16(),
            });
        }
        pages
    });
    match cfg.render_mode {
        RenderMode::Http | RenderMode::AutoSwitch => website.crawl_raw().await,
        RenderMode::Chrome => website.crawl().await,
    }
    website.unsubscribe();
    let mut candidates = collect
        .await
        .map_err(|e| format!("page collector panicked for scrape of {requested_url}: {e}"))?;

    // Include any pages retained by Spider internals and prefer a URL that
    // matches the requested target over whichever page arrived first.
    if let Some(pages) = website.get_pages() {
        candidates.extend(pages.iter().map(|page| ScrapedPage {
            url: page.get_url().to_string(),
            html: page.get_html(),
            status_code: page.status_code.as_u16(),
        }));
    }
    let Some(selected) = pick_best_page_for_url(requested_url, candidates) else {
        return direct_fetch_requested_page(cfg, requested_url).await;
    };

    if page_matches_requested_url(requested_url, &selected.url) {
        Ok(selected)
    } else {
        direct_fetch_requested_page(cfg, requested_url).await
    }
}

// ── Output formatting (pure functions) ───────────────────────────────────────

/// Maximum anchor hrefs captured into the `links` field of a scrape payload.
/// Large enough to capture a documentation page's full link set so the watch
/// change-detector sees a stable run-to-run snapshot, bounded so an
/// adversarially link-dense page can't bloat the payload.
const SCRAPE_LINKS_LIMIT: usize = 512;

/// Build the canonical 6-field JSON response for a scraped page.
///
/// Performs markdown conversion, title extraction, description extraction, and
/// anchor-link extraction in one place. All JSON-producing paths delegate here.
///
/// The `links` field is an array of `{href, text}` objects (text is currently
/// empty — diffing compares by href). It is the input the watch change-detector
/// reuses via `services::diff::extract_links_from_payload` to detect link
/// additions/removals; other payload consumers ignore it.
pub fn build_scrape_json(
    url: &str,
    html: &str,
    status_code: u16,
    selector_config: Option<&SelectorConfiguration>,
) -> serde_json::Value {
    let links: Vec<serde_json::Value> =
        crate::core::content::extract_anchor_hrefs(url, html, SCRAPE_LINKS_LIMIT)
            .into_iter()
            .map(|href| serde_json::json!({ "href": href, "text": "" }))
            .collect();
    serde_json::json!({
        "url": url,
        "status_code": status_code,
        "markdown": to_markdown(html, selector_config),
        "title": find_between(html, "<title>", "</title>").unwrap_or(""),
        "description": extract_meta_description(html).unwrap_or_default(),
        "links": links,
    })
}

/// Select the output text from the page HTML based on the requested format.
///
/// - `Markdown` / `Json`: convert HTML → markdown via our transform pipeline.
/// - `Html` / `RawHtml`: return raw HTML string.
///
/// This is a pure function, extractable and testable without Spider running.
pub fn select_output(
    format: crate::core::config::ScrapeFormat,
    url: &str,
    html: &str,
    status_code: u16,
    selector_config: Option<&SelectorConfiguration>,
) -> Result<String, Box<dyn Error>> {
    use crate::core::config::ScrapeFormat;
    match format {
        ScrapeFormat::Markdown => Ok(to_markdown(html, selector_config)),
        ScrapeFormat::Html | ScrapeFormat::RawHtml => Ok(html.to_string()),
        ScrapeFormat::Json => Ok(serde_json::to_string_pretty(&build_scrape_json(
            url,
            html,
            status_code,
            selector_config,
        ))?),
        ScrapeFormat::Llm => {
            let md = to_markdown(html, selector_config);
            Ok(crate::core::content::to_llm_text(&md, url))
        }
    }
}

// ── Service-facing entry point ───────────────────────────────────────────────

/// Scrape a single URL and return the canonical 5-field JSON payload.
///
/// This is the business logic entry point used by the services layer.
/// No CLI output or formatting — just fetches, validates, and returns data.
pub async fn scrape_payload(cfg: &Config, url: &str) -> Result<serde_json::Value, Box<dyn Error>> {
    let normalized = normalize_url(url);
    validate_url(&normalized).map_err(|e| format!("invalid scrape URL {normalized}: {e}"))?;

    let mut website = build_scrape_website(cfg, &normalized)
        .map_err(|e| format!("failed to build scrape config for {normalized}: {e}"))?;
    let page = fetch_single_page(cfg, &mut website, &normalized)
        .await
        .map_err(|e| format!("fetch failed for scrape of {normalized}: {e}"))?;
    let html = page.html;
    let status_code = page.status_code;
    if !(200..300).contains(&status_code) {
        return Err(format!("scrape failed: HTTP {} for {}", status_code, normalized).into());
    }

    let sel_cfg = build_selector_config(cfg);
    Ok(build_scrape_json(
        &normalized,
        &html,
        status_code,
        sel_cfg.as_ref(),
    ))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "scrape_tests.rs"]
mod tests;
