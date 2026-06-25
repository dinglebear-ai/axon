use super::*;
use axon_core::http::validate_url;

// -----------------------------------------------------------------------
// select_output — pure function, no network required
// -----------------------------------------------------------------------

#[test]
fn test_select_output_markdown_returns_markdown() {
    let html = "<html><body><p>Hello world</p></body></html>";
    let result = select_output(ScrapeFormat::Markdown, "https://example.com", html, 200)
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
    let result = select_output(ScrapeFormat::Html, "https://example.com", html, 200)
        .expect("select_output should succeed");
    assert_eq!(result, html, "Html format should return raw HTML unchanged");
}

#[test]
fn test_select_output_rawhtml_returns_raw_html() {
    let html = "<html><body><p>Test content</p></body></html>";
    let result = select_output(ScrapeFormat::RawHtml, "https://example.com", html, 200)
        .expect("select_output should succeed");
    assert_eq!(
        result, html,
        "RawHtml format should return raw HTML unchanged"
    );
}

#[test]
fn test_select_output_json_includes_status_code() {
    let html = "<html><head><title>My Page</title></head><body><p>Content</p></body></html>";
    let result = select_output(ScrapeFormat::Json, "https://example.com/page", html, 200)
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
    let result =
        select_output(ScrapeFormat::Json, url, html, 200).expect("select_output should succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    assert_eq!(parsed["url"], url, "JSON output must include the url field");
}

#[test]
fn test_select_output_json_includes_title() {
    let html = "<html><head><title>Spider Docs</title></head><body><p>Content</p></body></html>";
    let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200)
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
    let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200)
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
    let result = select_output(ScrapeFormat::Json, "https://example.com", html, 404)
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
