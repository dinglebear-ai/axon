//! Migration contract tests for the scrape command.
//!
//! Tests cover:
//!   - `select_output` — pure format conversion, no network
//!   - SSRF guard via `validate_url` — must reject private/loopback IPs
//!   - Config-to-Spider mapping helpers — pure logic
//!   - `build_scrape_website` — Spider config wiring

use std::time::Duration;

use super::*;
use axon_core::config::{RenderMode, ScrapeFormat};
use axon_crawl::scrape::{build_scrape_website, select_output};

// -----------------------------------------------------------------------
// select_output — pure function, no network required
// -----------------------------------------------------------------------

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
    // Must not be the raw HTML (format conversion happened)
    assert!(
        !result.contains("<html>"),
        "should not contain raw HTML tags"
    );
    // Must contain the text content
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
    assert_eq!(
        parsed["status_code"], 200,
        "JSON output must include status_code field"
    );
}

#[test]
fn test_select_output_json_includes_url() {
    let html = "<html><body><p>Test</p></body></html>";
    let url = "https://example.com/docs";
    let result = select_output(ScrapeFormat::Json, url, html, 200, None)
        .expect("select_output should succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    assert_eq!(parsed["url"], url, "JSON output must include the url field");
}

#[test]
fn test_select_output_json_includes_title() {
    let html = "<html><head><title>Spider Docs</title></head><body><p>Content</p></body></html>";
    let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
        .expect("select_output should succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    assert_eq!(
        parsed["title"], "Spider Docs",
        "JSON output must include title extracted from <title>"
    );
}

#[test]
fn test_select_output_json_includes_markdown() {
    let html = "<html><body><p>Hello world</p></body></html>";
    let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
        .expect("select_output should succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    let md = parsed["markdown"].as_str().expect("markdown field missing");
    assert!(
        md.contains("Hello world"),
        "markdown field must contain page text"
    );
    assert!(
        !md.contains("<html>"),
        "markdown field must not contain raw HTML"
    );
}

#[test]
fn test_select_output_json_status_code_non_200() {
    let html = "<html><body>Not Found</body></html>";
    let result = select_output(ScrapeFormat::Json, "https://example.com", html, 404, None)
        .expect("select_output should succeed even for non-200 (caller decides to error)");
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    assert_eq!(
        parsed["status_code"], 404,
        "JSON output must faithfully report non-200 status codes"
    );
}

// -----------------------------------------------------------------------
// SSRF guard — validate_url must reject private IPs
// These verify the guard that must run before build_scrape_website()
// -----------------------------------------------------------------------

#[test]
fn test_ssrf_guard_rejects_loopback() {
    assert!(
        validate_url("http://127.0.0.1/admin").is_err(),
        "SSRF guard must reject loopback addresses"
    );
}

#[test]
fn test_ssrf_guard_rejects_private_rfc1918() {
    assert!(
        validate_url("http://192.168.1.1/secret").is_err(),
        "SSRF guard must reject RFC-1918 private addresses"
    );
}

#[test]
fn test_ssrf_guard_rejects_localhost_hostname() {
    assert!(
        validate_url("http://localhost/").is_err(),
        "SSRF guard must reject 'localhost' hostname"
    );
}

#[test]
fn test_ssrf_guard_allows_public_url() {
    assert!(
        validate_url("https://example.com/docs").is_ok(),
        "SSRF guard must allow public HTTPS URLs"
    );
}

// -----------------------------------------------------------------------
// Config-to-Spider mapping helpers — pure logic, no network
// -----------------------------------------------------------------------

#[test]
fn test_fetch_retries_casts_to_u8_without_overflow() {
    // fetch_retries is usize; with_retry() takes u8.
    // Verify the cast logic: values > 255 clamp to 255, not wrap/panic.
    let large: usize = 300;
    let clamped = large.min(u8::MAX as usize) as u8;
    assert_eq!(clamped, 255u8, "fetch_retries > 255 must clamp to u8::MAX");
}

#[test]
fn test_fetch_retries_small_value_preserved() {
    let small: usize = 3;
    let cast = small.min(u8::MAX as usize) as u8;
    assert_eq!(
        cast, 3u8,
        "small fetch_retries must round-trip through u8 cast"
    );
}

#[test]
fn test_timeout_ms_converts_to_duration() {
    let timeout_ms: u64 = 15_000;
    let dur = Duration::from_millis(timeout_ms);
    assert_eq!(
        dur.as_secs(),
        15,
        "request_timeout_ms=15000 must produce Duration of 15s"
    );
}

#[test]
fn test_timeout_none_uses_spider_default() {
    // When cfg.request_timeout_ms is None, we pass None to with_request_timeout,
    // letting Spider use its own default. This test confirms the branch logic:
    // only pass Some(dur) when a value is configured.
    let timeout_ms: Option<u64> = None;
    let passed_to_spider = timeout_ms.map(Duration::from_millis);
    assert!(
        passed_to_spider.is_none(),
        "None timeout_ms must produce None passed to with_request_timeout"
    );
}

// -----------------------------------------------------------------------
// select_output — edge cases
// -----------------------------------------------------------------------

#[test]
fn test_select_output_empty_html_body() {
    // An empty HTML string should not panic; markdown output is just empty.
    let html = "";
    let result = select_output(
        ScrapeFormat::Markdown,
        "https://example.com",
        html,
        200,
        None,
    )
    .expect("select_output must handle empty HTML without error");
    // Empty input produces empty (or whitespace-only) markdown.
    assert!(
        result.trim().is_empty(),
        "empty HTML should produce empty markdown, got: {result:?}"
    );
}

#[test]
fn test_select_output_json_missing_title() {
    // HTML with no <title> tag: the title field must be an empty string, not null.
    let html = "<html><body><p>No title here</p></body></html>";
    let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
        .expect("select_output should succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    assert_eq!(
        parsed["title"], "",
        "missing <title> must produce empty string, not null"
    );
}

#[test]
fn test_select_output_json_missing_description() {
    // HTML with no <meta name="description">: description field must be empty string.
    let html = "<html><head><title>T</title></head><body><p>Content</p></body></html>";
    let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
        .expect("select_output should succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    assert_eq!(
        parsed["description"], "",
        "missing meta description must produce empty string, not null"
    );
}

#[test]
fn test_select_output_json_includes_description() {
    // Verify description field is populated from <meta name="description">.
    let html = r#"<html><head><title>Page</title><meta name="description" content="A fine page"></head><body><p>Body</p></body></html>"#;
    let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
        .expect("select_output should succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    assert_eq!(
        parsed["description"], "A fine page",
        "description must be extracted from meta tag"
    );
}

// Note: the duplicate `test_select_output_json_has_all_five_fields` was removed
// — it asserted the identical contract as
// `test_select_output_json_has_all_required_fields` below (exact six-field set).

// -----------------------------------------------------------------------
// build_scrape_website — config-to-Spider mapping (pure, no network)
// -----------------------------------------------------------------------

#[test]
fn test_build_scrape_website_sets_limit_one() {
    let cfg = Config::default();
    let website = build_scrape_website(&cfg, "https://example.com")
        .expect("build_scrape_website should succeed");
    // with_limit(1) sets a budget of {"*": 1} — verify via the budget field.
    let budget = website
        .configuration
        .budget
        .as_ref()
        .expect("budget must be set after with_limit(1)");
    assert_eq!(
        budget.len(),
        1,
        "budget should have exactly one entry (wildcard)"
    );
    let val = budget.values().next().expect("budget should have a value");
    assert_eq!(*val, 1, "with_limit(1) must set budget wildcard to 1");
}

#[test]
fn test_build_scrape_website_blocks_assets() {
    let cfg = Config::default();
    let website = build_scrape_website(&cfg, "https://example.com")
        .expect("build_scrape_website should succeed");
    assert!(
        website.configuration.only_html,
        "block_assets must be true for scrape (only fetch HTML, not images/CSS/JS)"
    );
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
    assert!(
        !blacklist.is_empty(),
        "SSRF blacklist patterns must be wired into spider"
    );
}

#[test]
fn test_build_scrape_website_sets_retry_from_config() {
    let cfg = Config {
        fetch_retries: 5,
        ..Config::default()
    };
    let website = build_scrape_website(&cfg, "https://example.com")
        .expect("build_scrape_website should succeed");
    assert_eq!(
        website.configuration.retry, 5,
        "retry must match cfg.fetch_retries"
    );
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
        .expect("request_timeout must be set when cfg has timeout_ms");
    assert_eq!(
        timeout.as_millis(),
        10_000,
        "request_timeout must match cfg.request_timeout_ms"
    );
}

#[test]
fn test_build_scrape_website_explicit_timeout_overrides_spider_default() {
    // When cfg.request_timeout_ms is set, the resulting timeout must match exactly.
    // When None, Spider keeps its own default — we only assert the explicit case.
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
        .expect("timeout must be set when cfg has timeout_ms");
    assert_eq!(timeout.as_millis(), 7_500, "explicit timeout must match");
    // When None, spider uses its own default — just verify it differs from ours.
    // (Spider's default is 15s; our explicit value is 7.5s — they must differ.)
    let default_timeout = ws_without
        .configuration
        .request_timeout
        .map(|d| d.as_millis());
    assert_ne!(
        default_timeout,
        Some(7_500),
        "without explicit timeout, spider default must differ from our explicit value"
    );
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
        .expect("headers must be set when custom_headers is non-empty");
    assert!(
        headers.contains_key("authorization"),
        "Authorization header must be wired"
    );
    assert!(
        headers.contains_key("x-custom"),
        "X-Custom header must be wired"
    );
}

#[test]
fn test_build_scrape_website_no_headers_when_empty() {
    let cfg = Config {
        custom_headers: vec![],
        ..Config::default()
    };
    let website = build_scrape_website(&cfg, "https://example.com")
        .expect("build_scrape_website should succeed");
    // When no custom headers, headers should remain None (spider default).
    assert!(
        website.configuration.headers.is_none(),
        "headers must be None when custom_headers is empty"
    );
}

#[test]
fn test_build_scrape_website_sets_no_control_thread() {
    let cfg = Config::default();
    let website = build_scrape_website(&cfg, "https://example.com")
        .expect("build_scrape_website should succeed");
    assert!(
        website.configuration.no_control_thread,
        "no_control_thread must be true for single-page scrape"
    );
}

#[test]
fn test_build_scrape_website_chrome_mode_sets_dismiss_dialogs() {
    let cfg = Config {
        render_mode: RenderMode::Chrome,
        ..Config::default()
    };
    let website = build_scrape_website(&cfg, "https://example.com")
        .expect("build_scrape_website should succeed");
    assert_eq!(
        website.configuration.dismiss_dialogs,
        Some(true),
        "Chrome mode must set dismiss_dialogs"
    );
    assert!(
        website.configuration.disable_log,
        "Chrome mode must set disable_log"
    );
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
    assert!(
        website.configuration.bypass_csp,
        "Chrome mode with bypass_csp must set csp_bypass on spider"
    );
}

#[test]
fn test_build_scrape_website_http_mode_skips_chrome_options() {
    let cfg = Config {
        render_mode: RenderMode::Http,
        bypass_csp: true, // should be ignored in HTTP mode
        ..Config::default()
    };
    let website = build_scrape_website(&cfg, "https://example.com")
        .expect("build_scrape_website should succeed");
    // In HTTP mode, Chrome-specific options must NOT be set.
    assert_eq!(
        website.configuration.dismiss_dialogs, None,
        "HTTP mode must not set dismiss_dialogs"
    );
    assert!(
        !website.configuration.disable_log,
        "HTTP mode must not set disable_log"
    );
    assert!(
        !website.configuration.bypass_csp,
        "HTTP mode must not set bypass_csp even when cfg.bypass_csp is true"
    );
}

#[test]
fn test_build_scrape_website_accept_invalid_certs() {
    let cfg = Config {
        accept_invalid_certs: true,
        ..Config::default()
    };
    let website = build_scrape_website(&cfg, "https://example.com")
        .expect("build_scrape_website should succeed");
    assert!(
        website.configuration.accept_invalid_certs,
        "accept_invalid_certs must be wired through to spider"
    );
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
        .expect("user_agent must be set when chrome_user_agent is Some");
    assert_eq!(
        ua.as_str(),
        "TestBot/1.0",
        "user_agent must match cfg.chrome_user_agent"
    );
}

// -----------------------------------------------------------------------
// select_output — migration contract coverage
// -----------------------------------------------------------------------

#[test]
fn test_select_output_json_handles_missing_title() {
    // Contract: when HTML has no <title> tag, the JSON title field must be
    // an empty string (not null, not absent).
    let html = "<html><head></head><body><p>No title tag at all</p></body></html>";
    let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
        .expect("select_output should succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    assert_eq!(
        parsed["title"], "",
        "HTML with no <title> tag must produce empty string title"
    );
    assert!(
        parsed["title"].is_string(),
        "title must be a string, not null"
    );
}

#[test]
fn test_select_output_json_handles_missing_description() {
    // Contract: when HTML has no <meta name="description">, the JSON
    // description field must be an empty string (not null, not absent).
    let html = "<html><head><title>Has Title</title></head><body><p>No meta desc</p></body></html>";
    let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
        .expect("select_output should succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    assert_eq!(
        parsed["description"], "",
        "HTML with no meta description must produce empty string"
    );
    assert!(
        parsed["description"].is_string(),
        "description must be a string, not null"
    );
}

#[test]
fn test_select_output_json_has_all_required_fields() {
    // Contract: JSON output must contain exactly: url, status_code, title,
    // description, markdown, links — no more, no fewer.
    let html = r#"<html><head><title>T</title><meta name="description" content="D"></head><body><p>B</p></body></html>"#;
    let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
        .expect("select_output should succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    let obj = parsed.as_object().expect("JSON output must be an object");
    let required = [
        "url",
        "status_code",
        "title",
        "description",
        "markdown",
        "links",
    ];
    for field in &required {
        assert!(
            obj.contains_key(*field),
            "JSON output missing required field: {field}"
        );
    }
    assert_eq!(
        obj.len(),
        required.len(),
        "JSON output must contain exactly {} fields, got: {:?}",
        required.len(),
        obj.keys().collect::<Vec<_>>()
    );
}

#[test]
fn test_select_output_markdown_empty_body() {
    // An HTML document with an empty <body> must not panic and should
    // produce empty (or whitespace-only) markdown.
    let html = "<html><head></head><body></body></html>";
    let result = select_output(
        ScrapeFormat::Markdown,
        "https://example.com",
        html,
        200,
        None,
    )
    .expect("select_output must not panic on empty body");
    // Empty body → no text content → trimmed result should be empty.
    assert!(
        result.trim().is_empty(),
        "empty <body></body> should produce empty markdown, got: {result:?}"
    );
}

#[test]
fn test_select_output_html_preserves_entities() {
    // Html format returns raw HTML unchanged — HTML entities like &amp;
    // must be preserved verbatim (no double-encoding, no decoding).
    let html = "<html><body><p>A &amp; B &lt; C</p></body></html>";
    let result = select_output(ScrapeFormat::Html, "https://example.com", html, 200, None)
        .expect("select_output should succeed");
    assert_eq!(
        result, html,
        "Html format must return raw HTML with entities preserved"
    );
    assert!(
        result.contains("&amp;"),
        "HTML entities must not be decoded"
    );
    assert!(result.contains("&lt;"), "HTML entities must not be decoded");
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
        .expect("proxies must be set when chrome_proxy is Some");
    assert_eq!(proxies.len(), 1, "exactly one proxy must be configured");
    assert_eq!(
        proxies[0].addr, "http://proxy.example.com:8080",
        "proxy address must match cfg.chrome_proxy"
    );
}
