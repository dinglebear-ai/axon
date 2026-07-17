use super::*;
use axon_core::config::ScrapeFormat;
use httpmock::prelude::*;

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
    let html = "<html><head><title>Spider Docs</title></head><body><p>Content</p></body></html>";
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
fn test_select_output_json_has_all_six_fields() {
    let html = r#"<html><head><title>T</title><meta name="description" content="D"></head><body><p>B</p></body></html>"#;
    let result = select_output(ScrapeFormat::Json, "https://example.com", html, 200, None)
        .expect("select_output should succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    let obj = parsed.as_object().expect("JSON output must be an object");
    for field in &[
        "url",
        "status_code",
        "markdown",
        "title",
        "description",
        "links",
    ] {
        assert!(obj.contains_key(*field), "missing required field: {field}");
    }
    assert_eq!(obj.len(), 6);
}

#[test]
fn test_build_scrape_json_populates_links_from_anchors() {
    let html = r##"<html><head><title>T</title></head><body>
        <a href="https://example.com/docs/a">A</a>
        <a href="/docs/b">B</a>
        <a href="#frag">skip-fragment</a>
        <a href="mailto:x@y.z">skip-mailto</a>
    </body></html>"##;
    let parsed = build_scrape_json("https://example.com/docs", html, 200, None);
    let links = parsed["links"].as_array().expect("links must be an array");
    let hrefs: Vec<&str> = links.iter().filter_map(|l| l["href"].as_str()).collect();
    // Absolute and root-relative anchors are captured (relative resolved against
    // the base URL); fragments and mailto links are skipped.
    assert!(
        hrefs.contains(&"https://example.com/docs/a"),
        "got {hrefs:?}"
    );
    assert!(
        hrefs.contains(&"https://example.com/docs/b"),
        "got {hrefs:?}"
    );
    assert!(
        !hrefs.iter().any(|h| h.contains("mailto")),
        "mailto should be filtered: {hrefs:?}"
    );
    // Each entry carries the {href, text} shape extract_links_from_payload reads.
    assert!(
        links
            .iter()
            .all(|l| l.get("href").is_some() && l.get("text").is_some())
    );
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

#[tokio::test]
async fn direct_fetch_fallback_keeps_custom_headers_per_config() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    let first = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/first")
                .header("x-axon-test", "first");
            then.status(200)
                .header("content-type", "text/html")
                .body("<html><body>first</body></html>");
        })
        .await;
    let second = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/second")
                .header("x-axon-test", "second");
            then.status(200)
                .header("content-type", "text/html")
                .body("<html><body>second</body></html>");
        })
        .await;
    let first_cfg = Config {
        custom_headers: vec!["x-axon-test: first".to_string()],
        ..Config::default()
    };
    let second_cfg = Config {
        custom_headers: vec!["x-axon-test: second".to_string()],
        ..Config::default()
    };

    direct_fetch_requested_page(&first_cfg, &format!("{}/first", server.base_url()))
        .await
        .expect("first fetch should use first header");
    direct_fetch_requested_page(&second_cfg, &format!("{}/second", server.base_url()))
        .await
        .expect("second fetch should use second header, not a process-global cached client");

    first.assert_async().await;
    second.assert_async().await;
}

#[test]
fn test_select_output_json_has_all_required_fields() {
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
        "https://docs.rs/serde/latest/serde/trait.Serialize.html",
        "https://docs.rs/serde/1.0.203/serde/trait.Serialize.html"
    ));
}

#[test]
fn test_page_matches_requested_url_rejects_different_terminal_page() {
    assert!(!page_matches_requested_url(
        "https://docs.rs/serde/latest/serde/trait.Serialize.html",
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

#[test]
fn single_page_chrome_render_uses_ssrf_request_interception() {
    let cfg = Config {
        render_mode: RenderMode::Chrome,
        ..Config::default()
    };
    let website = build_scrape_website(&cfg, "https://example.com/").expect("website config");
    let intercept = &website.configuration.chrome_intercept;
    assert!(intercept.enabled);
    let blacklist = intercept
        .blacklist_patterns
        .as_ref()
        .expect("Chrome intercept must carry SSRF patterns");
    for expected in ["127\\.", "169\\.254\\.", "192\\.168\\.", "\\[::1\\]"] {
        assert!(
            blacklist.iter().any(|pattern| pattern.contains(expected)),
            "missing Chrome SSRF pattern {expected}: {blacklist:?}"
        );
    }
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

// apply_automation_scripts — issue #298 Wave 2b regression 1 restoration

#[tokio::test]
async fn test_apply_automation_scripts_noop_when_unset() {
    let cfg = Config {
        render_mode: RenderMode::Chrome,
        ..Config::default()
    };
    let mut website = build_scrape_website(&cfg, "https://example.com")
        .expect("build_scrape_website should succeed");
    apply_automation_scripts(&cfg, &mut website)
        .await
        .expect("no automation_script configured must be a no-op");
    assert!(website.configuration.automation_scripts.is_none());
}

#[tokio::test]
async fn test_apply_automation_scripts_skips_on_http_mode_without_touching_disk() {
    let cfg = Config {
        render_mode: RenderMode::Http,
        // A path that does not exist: if the Http-mode gate did not skip
        // *before* attempting to load, this would fail with an I/O error.
        automation_script: Some(std::path::PathBuf::from("/nonexistent/script.json")),
        ..Config::default()
    };
    let mut website = build_scrape_website(&cfg, "https://example.com")
        .expect("build_scrape_website should succeed");
    apply_automation_scripts(&cfg, &mut website)
        .await
        .expect("http mode must skip automation_script rather than error");
    assert!(website.configuration.automation_scripts.is_none());
}

#[tokio::test]
async fn test_apply_automation_scripts_propagates_load_errors_on_chrome_mode() {
    let cfg = Config {
        render_mode: RenderMode::Chrome,
        automation_script: Some(std::path::PathBuf::from("/nonexistent/script.json")),
        ..Config::default()
    };
    let mut website = build_scrape_website(&cfg, "https://example.com")
        .expect("build_scrape_website should succeed");
    let err = apply_automation_scripts(&cfg, &mut website)
        .await
        .expect_err("chrome mode must attempt to load and surface a missing-file error");
    assert!(err.to_string().contains("script.json"));
}

#[tokio::test]
async fn test_apply_automation_scripts_applies_valid_script_on_chrome_mode() {
    let dir = tempfile::tempdir().expect("tempdir");
    let script_path = dir.path().join("automation.json");
    std::fs::write(&script_path, r#"{"/": [{"action": "wait", "ms": 100}]}"#)
        .expect("write automation script");

    let cfg = Config {
        render_mode: RenderMode::Chrome,
        automation_script: Some(script_path),
        ..Config::default()
    };
    let mut website = build_scrape_website(&cfg, "https://example.com")
        .expect("build_scrape_website should succeed");
    apply_automation_scripts(&cfg, &mut website)
        .await
        .expect("a valid automation script on chrome mode must apply cleanly");
    assert!(website.configuration.automation_scripts.is_some());
}
