use crate::crates::core::config::{Config, RenderMode};
use crate::crates::core::content::{
    build_selector_config, extract_meta_description, find_between, to_markdown,
};
use crate::crates::core::http::{normalize_url, ssrf_blacklist_compact_strings, validate_url};
use crate::crates::core::logging::log_warn;
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

    if let Some(ua) = cfg.chrome_user_agent.as_deref() {
        website.with_user_agent(Some(ua));
    }
    if let Some(proxy) = cfg.chrome_proxy.as_deref() {
        website.with_proxies(Some(vec![proxy.to_string()]));
    }
    // Wire custom headers so `--header` works for single-page scrapes too.
    if !cfg.custom_headers.is_empty() {
        let map = crate::crates::core::http::parse_custom_headers(&cfg.custom_headers);
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

pub(crate) async fn direct_fetch_requested_page(
    cfg: &Config,
    requested_url: &str,
) -> Result<ScrapedPage, Box<dyn Error>> {
    let mut builder = reqwest::Client::builder();
    if let Some(timeout_ms) = cfg.request_timeout_ms {
        builder = builder.timeout(Duration::from_millis(timeout_ms));
    }
    if cfg.accept_invalid_certs {
        warn_invalid_certs_once();
        builder = builder.danger_accept_invalid_certs(true);
    }
    if let Some(ua) = cfg.chrome_user_agent.as_deref() {
        builder = builder.user_agent(ua);
    }
    if let Some(proxy) = cfg.chrome_proxy.as_deref().filter(|p| !p.trim().is_empty()) {
        builder = builder.proxy(reqwest::Proxy::all(proxy)?);
    }
    if !cfg.custom_headers.is_empty() {
        let map = crate::crates::core::http::parse_custom_headers(&cfg.custom_headers);
        if !map.is_empty() {
            builder = builder.default_headers(map);
        }
    }
    // Validate each redirect target through the SSRF blacklist so a public URL
    // cannot redirect to a private/internal address and bypass the guard.
    // Preserve reqwest's default redirect cap (10) to prevent infinite loops.
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
    let client = builder.build()?;
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
    let mut rx = website.subscribe(16).ok_or_else(|| {
        format!("failed to subscribe to spider broadcast for scrape of {requested_url}")
    })?;
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

/// Build the canonical 5-field JSON response for a scraped page.
///
/// Performs markdown conversion, title extraction, and description extraction
/// in one place. All JSON-producing paths delegate here.
pub fn build_scrape_json(
    url: &str,
    html: &str,
    status_code: u16,
    selector_config: Option<&SelectorConfiguration>,
) -> serde_json::Value {
    serde_json::json!({
        "url": url,
        "status_code": status_code,
        "markdown": to_markdown(html, selector_config),
        "title": find_between(html, "<title>", "</title>").unwrap_or(""),
        "description": extract_meta_description(html).unwrap_or_default(),
    })
}

/// Select the output text from the page HTML based on the requested format.
///
/// - `Markdown` / `Json`: convert HTML → markdown via our transform pipeline.
/// - `Html` / `RawHtml`: return raw HTML string.
///
/// This is a pure function, extractable and testable without Spider running.
pub fn select_output(
    format: crate::crates::core::config::ScrapeFormat,
    url: &str,
    html: &str,
    status_code: u16,
    selector_config: Option<&SelectorConfiguration>,
) -> Result<String, Box<dyn Error>> {
    use crate::crates::core::config::ScrapeFormat;
    match format {
        ScrapeFormat::Markdown => Ok(to_markdown(html, selector_config)),
        ScrapeFormat::Html | ScrapeFormat::RawHtml => Ok(html.to_string()),
        ScrapeFormat::Json => Ok(serde_json::to_string_pretty(&build_scrape_json(
            url,
            html,
            status_code,
            selector_config,
        ))?),
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
mod tests {
    use super::*;
    use crate::crates::core::config::ScrapeFormat;

    // select_output — pure function, no network required

    #[test]
    fn test_select_output_markdown_returns_markdown() {
        let html = "<html><body><p>Hello world</p></body></html>";
        let result = select_output(
            ScrapeFormat::Markdown,
            "https://example.com",
            html,
            200,
            None,
        )
        .expect("select_output should succeed");
        assert!(
            !result.contains("<html>"),
            "should not contain raw HTML tags"
        );
        assert!(result.contains("Hello world"), "should contain page text");
    }

    #[test]
    fn test_select_output_html_returns_raw_html() {
        let html = "<html><body><p>Hello world</p></body></html>";
        let result = select_output(ScrapeFormat::Html, "https://example.com", html, 200, None)
            .expect("select_output should succeed");
        assert_eq!(result, html, "Html format should return raw HTML unchanged");
    }

    #[test]
    fn test_select_output_rawhtml_returns_raw_html() {
        let html = "<html><body><p>Test content</p></body></html>";
        let result = select_output(
            ScrapeFormat::RawHtml,
            "https://example.com",
            html,
            200,
            None,
        )
        .expect("select_output should succeed");
        assert_eq!(
            result, html,
            "RawHtml format should return raw HTML unchanged"
        );
    }

    #[test]
    fn test_select_output_json_includes_status_code() {
        let html = "<html><head><title>My Page</title></head><body><p>Content</p></body></html>";
        let result = select_output(
            ScrapeFormat::Json,
            "https://example.com/page",
            html,
            200,
            None,
        )
        .expect("select_output should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("output should be valid JSON");
        assert_eq!(parsed["status_code"], 200);
    }

    #[test]
    fn test_select_output_json_includes_url() {
        let html = "<html><body><p>Test</p></body></html>";
        let url = "https://example.com/docs";
        let result = select_output(ScrapeFormat::Json, url, html, 200, None)
            .expect("select_output should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("output should be valid JSON");
        assert_eq!(parsed["url"], url);
    }

    #[test]
    fn test_select_output_json_includes_title() {
        let html =
            "<html><head><title>Spider Docs</title></head><body><p>Content</p></body></html>";
        let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
            .expect("select_output should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("output should be valid JSON");
        assert_eq!(parsed["title"], "Spider Docs");
    }

    #[test]
    fn test_select_output_json_includes_markdown() {
        let html = "<html><body><p>Hello world</p></body></html>";
        let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
            .expect("select_output should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("output should be valid JSON");
        let md = parsed["markdown"].as_str().expect("markdown field missing");
        assert!(md.contains("Hello world"));
        assert!(!md.contains("<html>"));
    }

    #[test]
    fn test_select_output_json_status_code_non_200() {
        let html = "<html><body>Not Found</body></html>";
        let result = select_output(ScrapeFormat::Json, "https://example.com", html, 404, None)
            .expect("select_output should succeed even for non-200");
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("output should be valid JSON");
        assert_eq!(parsed["status_code"], 404);
    }

    #[test]
    fn test_select_output_empty_html_body() {
        let html = "";
        let result = select_output(
            ScrapeFormat::Markdown,
            "https://example.com",
            html,
            200,
            None,
        )
        .expect("select_output must handle empty HTML without error");
        assert!(result.trim().is_empty());
    }

    #[test]
    fn test_select_output_json_missing_title() {
        let html = "<html><body><p>No title here</p></body></html>";
        let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
            .expect("select_output should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("output should be valid JSON");
        assert_eq!(parsed["title"], "");
    }

    #[test]
    fn test_select_output_json_missing_description() {
        let html = "<html><head><title>T</title></head><body><p>Content</p></body></html>";
        let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
            .expect("select_output should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("output should be valid JSON");
        assert_eq!(parsed["description"], "");
    }

    #[test]
    fn test_select_output_json_includes_description() {
        let html = r#"<html><head><title>Page</title><meta name="description" content="A fine page"></head><body><p>Body</p></body></html>"#;
        let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
            .expect("select_output should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("output should be valid JSON");
        assert_eq!(parsed["description"], "A fine page");
    }

    #[test]
    fn test_select_output_json_has_all_five_fields() {
        let html = r#"<html><head><title>T</title><meta name="description" content="D"></head><body><p>B</p></body></html>"#;
        let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
            .expect("select_output should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("output should be valid JSON");
        let obj = parsed.as_object().expect("JSON output must be an object");
        for field in &["url", "status_code", "markdown", "title", "description"] {
            assert!(obj.contains_key(*field), "missing required field: {field}");
        }
        assert_eq!(obj.len(), 5);
    }

    #[test]
    fn test_select_output_html_preserves_entities() {
        let html = "<html><body><p>A &amp; B &lt; C</p></body></html>";
        let result = select_output(ScrapeFormat::Html, "https://example.com", html, 200, None)
            .expect("select_output should succeed");
        assert_eq!(result, html);
        assert!(result.contains("&amp;"));
        assert!(result.contains("&lt;"));
    }

    #[test]
    fn test_select_output_markdown_empty_body() {
        let html = "<html><head></head><body></body></html>";
        let result = select_output(
            ScrapeFormat::Markdown,
            "https://example.com",
            html,
            200,
            None,
        )
        .expect("select_output must not panic on empty body");
        assert!(result.trim().is_empty());
    }

    #[test]
    fn test_select_output_json_has_all_required_fields() {
        let html = r#"<html><head><title>T</title><meta name="description" content="D"></head><body><p>B</p></body></html>"#;
        let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
            .expect("select_output should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("output should be valid JSON");
        let obj = parsed.as_object().expect("JSON output must be an object");
        let required = ["url", "status_code", "title", "description", "markdown"];
        for field in &required {
            assert!(obj.contains_key(*field), "missing required field: {field}");
        }
        assert_eq!(obj.len(), required.len());
    }

    // page_matches_requested_url — URL matching logic

    #[test]
    fn test_page_matches_requested_url_exact_match() {
        assert!(page_matches_requested_url(
            "https://example.com/docs/trait.Client.html",
            "https://example.com/docs/trait.Client.html"
        ));
    }

    #[test]
    fn test_page_matches_requested_url_ignores_query_and_fragment() {
        assert!(page_matches_requested_url(
            "https://example.com/docs/trait.Client.html",
            "https://example.com/docs/trait.Client.html?x=1#section"
        ));
    }

    #[test]
    fn test_page_matches_requested_url_accepts_docs_redirect_filename_match() {
        assert!(page_matches_requested_url(
            "https://docs.rs/agent-client-protocol/latest/agent_client_protocol/trait.Client.html",
            "https://docs.rs/agent-client-protocol/0.9.5/agent_client_protocol/trait.Client.html"
        ));
    }

    #[test]
    fn test_page_matches_requested_url_rejects_different_terminal_page() {
        assert!(!page_matches_requested_url(
            "https://docs.rs/agent-client-protocol/latest/agent_client_protocol/trait.Client.html",
            "https://docs.rs/releases"
        ));
    }

    // SSRF guard — validate_url must reject private IPs

    #[test]
    fn test_ssrf_guard_rejects_loopback() {
        assert!(validate_url("http://127.0.0.1/admin").is_err());
    }

    #[test]
    fn test_ssrf_guard_rejects_private_rfc1918() {
        assert!(validate_url("http://192.168.1.1/secret").is_err());
    }

    #[test]
    fn test_ssrf_guard_rejects_localhost_hostname() {
        assert!(validate_url("http://localhost/").is_err());
    }

    #[test]
    fn test_ssrf_guard_allows_public_url() {
        assert!(validate_url("https://example.com/docs").is_ok());
    }

    // Config-to-Spider mapping helpers

    #[test]
    fn test_fetch_retries_casts_to_u8_without_overflow() {
        let large: usize = 300;
        let clamped = large.min(u8::MAX as usize) as u8;
        assert_eq!(clamped, 255u8);
    }

    #[test]
    fn test_fetch_retries_small_value_preserved() {
        let small: usize = 3;
        let cast = small.min(u8::MAX as usize) as u8;
        assert_eq!(cast, 3u8);
    }

    #[test]
    fn test_timeout_ms_converts_to_duration() {
        let timeout_ms: u64 = 15_000;
        let dur = Duration::from_millis(timeout_ms);
        assert_eq!(dur.as_secs(), 15);
    }

    #[test]
    fn test_timeout_none_uses_spider_default() {
        let timeout_ms: Option<u64> = None;
        let passed_to_spider = timeout_ms.map(Duration::from_millis);
        assert!(passed_to_spider.is_none());
    }

    // build_scrape_website — config-to-Spider mapping

    #[test]
    fn test_build_scrape_website_sets_limit_one() {
        let cfg = Config::default();
        let website = build_scrape_website(&cfg, "https://example.com")
            .expect("build_scrape_website should succeed");
        let budget = website
            .configuration
            .budget
            .as_ref()
            .expect("budget must be set");
        assert_eq!(budget.len(), 1);
        let val = budget.values().next().expect("budget should have a value");
        assert_eq!(*val, 1);
    }

    #[test]
    fn test_build_scrape_website_blocks_assets() {
        let cfg = Config::default();
        let website = build_scrape_website(&cfg, "https://example.com")
            .expect("build_scrape_website should succeed");
        assert!(website.configuration.only_html);
    }

    #[test]
    fn test_build_scrape_website_wires_ssrf_blacklist() {
        let cfg = Config::default();
        let website = build_scrape_website(&cfg, "https://example.com")
            .expect("build_scrape_website should succeed");
        let blacklist = website
            .configuration
            .blacklist_url
            .as_ref()
            .expect("blacklist_url must be set");
        assert!(!blacklist.is_empty());
    }

    #[test]
    fn test_build_scrape_website_sets_retry_from_config() {
        let cfg = Config {
            fetch_retries: 5,
            ..Config::default()
        };
        let website = build_scrape_website(&cfg, "https://example.com")
            .expect("build_scrape_website should succeed");
        assert_eq!(website.configuration.retry, 5);
    }

    #[test]
    fn test_build_scrape_website_sets_timeout_from_config() {
        let cfg = Config {
            request_timeout_ms: Some(10_000),
            ..Config::default()
        };
        let website = build_scrape_website(&cfg, "https://example.com")
            .expect("build_scrape_website should succeed");
        let timeout = website
            .configuration
            .request_timeout
            .as_ref()
            .expect("timeout must be set");
        assert_eq!(timeout.as_millis(), 10_000);
    }

    #[test]
    fn test_build_scrape_website_explicit_timeout_overrides_spider_default() {
        let cfg_with = Config {
            request_timeout_ms: Some(7_500),
            ..Config::default()
        };
        let cfg_without = Config {
            request_timeout_ms: None,
            ..Config::default()
        };
        let ws_with = build_scrape_website(&cfg_with, "https://example.com")
            .expect("build_scrape_website should succeed");
        let ws_without = build_scrape_website(&cfg_without, "https://example.com")
            .expect("build_scrape_website should succeed");
        let timeout = ws_with
            .configuration
            .request_timeout
            .as_ref()
            .expect("timeout must be set");
        assert_eq!(timeout.as_millis(), 7_500);
        let default_timeout = ws_without
            .configuration
            .request_timeout
            .as_ref()
            .map(|d| d.as_millis());
        assert_ne!(default_timeout, Some(7_500));
    }

    #[test]
    fn test_build_scrape_website_wires_custom_headers() {
        let cfg = Config {
            custom_headers: vec![
                "Authorization: Bearer test-token".to_string(),
                "X-Custom: value".to_string(),
            ],
            ..Config::default()
        };
        let website = build_scrape_website(&cfg, "https://example.com")
            .expect("build_scrape_website should succeed");
        let headers = website
            .configuration
            .headers
            .as_ref()
            .expect("headers must be set");
        assert!(headers.contains_key("authorization"));
        assert!(headers.contains_key("x-custom"));
    }

    #[test]
    fn test_build_scrape_website_no_headers_when_empty() {
        let cfg = Config {
            custom_headers: vec![],
            ..Config::default()
        };
        let website = build_scrape_website(&cfg, "https://example.com")
            .expect("build_scrape_website should succeed");
        assert!(website.configuration.headers.is_none());
    }

    #[test]
    fn test_build_scrape_website_sets_no_control_thread() {
        let cfg = Config::default();
        let website = build_scrape_website(&cfg, "https://example.com")
            .expect("build_scrape_website should succeed");
        assert!(website.configuration.no_control_thread);
    }

    #[test]
    fn test_build_scrape_website_chrome_mode_sets_dismiss_dialogs() {
        let cfg = Config {
            render_mode: RenderMode::Chrome,
            ..Config::default()
        };
        let website = build_scrape_website(&cfg, "https://example.com")
            .expect("build_scrape_website should succeed");
        assert_eq!(website.configuration.dismiss_dialogs, Some(true));
        assert!(website.configuration.disable_log);
    }

    #[test]
    fn test_build_scrape_website_chrome_mode_with_csp_bypass() {
        let cfg = Config {
            render_mode: RenderMode::Chrome,
            bypass_csp: true,
            ..Config::default()
        };
        let website = build_scrape_website(&cfg, "https://example.com")
            .expect("build_scrape_website should succeed");
        assert!(website.configuration.bypass_csp);
    }

    #[test]
    fn test_build_scrape_website_http_mode_skips_chrome_options() {
        let cfg = Config {
            render_mode: RenderMode::Http,
            bypass_csp: true,
            ..Config::default()
        };
        let website = build_scrape_website(&cfg, "https://example.com")
            .expect("build_scrape_website should succeed");
        assert_eq!(website.configuration.dismiss_dialogs, None);
        assert!(!website.configuration.disable_log);
        assert!(!website.configuration.bypass_csp);
    }

    #[test]
    fn test_build_scrape_website_accept_invalid_certs() {
        let cfg = Config {
            accept_invalid_certs: true,
            ..Config::default()
        };
        let website = build_scrape_website(&cfg, "https://example.com")
            .expect("build_scrape_website should succeed");
        assert!(website.configuration.accept_invalid_certs);
    }

    #[test]
    fn test_build_scrape_website_wires_user_agent() {
        let cfg = Config {
            chrome_user_agent: Some("TestBot/1.0".to_string()),
            ..Config::default()
        };
        let website = build_scrape_website(&cfg, "https://example.com")
            .expect("build_scrape_website should succeed");
        let ua = website
            .configuration
            .user_agent
            .as_ref()
            .expect("user_agent must be set");
        assert_eq!(ua.as_str(), "TestBot/1.0");
    }

    #[test]
    fn test_build_scrape_website_wires_proxy() {
        let cfg = Config {
            chrome_proxy: Some("http://proxy.example.com:8080".to_string()),
            ..Config::default()
        };
        let website = build_scrape_website(&cfg, "https://example.com")
            .expect("build_scrape_website should succeed");
        let proxies = website
            .configuration
            .proxies
            .as_ref()
            .expect("proxies must be set");
        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0].addr, "http://proxy.example.com:8080");
    }
}
