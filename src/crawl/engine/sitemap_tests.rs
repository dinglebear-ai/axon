use super::*;
use crate::core::config::Config;

/// RAII guard for the global SSRF loopback-bypass flag. Restores the previous value on
/// drop so a panicking test cannot leak `allow_loopback=true` into later SSRF tests.
struct LoopbackGuard(bool);

impl LoopbackGuard {
    fn new(allow: bool) -> Self {
        let prev = crate::core::http::get_allow_loopback();
        crate::core::http::set_allow_loopback(allow);
        Self(prev)
    }
}

impl Drop for LoopbackGuard {
    fn drop(&mut self) {
        crate::core::http::set_allow_loopback(self.0);
    }
}

/// Unit test for `sitemap_loc_in_scope` using real domain names.
/// The integration test uses a loopback mock server (IP address) where
/// IP addresses have no subdomain relationship — this test exercises
/// the actual subdomain branching logic directly with real hostnames.
#[test]
fn sitemap_loc_in_scope_subdomain_branching() {
    let cfg_no_sub = Config {
        include_subdomains: false,
        ..Config::default()
    };
    let cfg_with_sub = Config {
        include_subdomains: true,
        ..Config::default()
    };

    // Same host: included regardless of include_subdomains setting.
    assert!(
        loc_in_scope(
            &cfg_no_sub,
            "https://docs.example.com/page",
            "docs.example.com",
            "/",
            true
        )
        .is_some(),
        "same host should always be in scope"
    );

    // Subdomain with include_subdomains=false: excluded.
    assert!(
        loc_in_scope(
            &cfg_no_sub,
            "https://api.example.com/page",
            "example.com",
            "/",
            true
        )
        .is_none(),
        "subdomain should be excluded when include_subdomains=false"
    );

    // Subdomain with include_subdomains=true: included.
    assert!(
        loc_in_scope(
            &cfg_with_sub,
            "https://api.example.com/page",
            "example.com",
            "/",
            true
        )
        .is_some(),
        "subdomain should be included when include_subdomains=true"
    );

    // Completely different domain: excluded with both settings.
    assert!(
        loc_in_scope(
            &cfg_with_sub,
            "https://other.com/page",
            "example.com",
            "/",
            true
        )
        .is_none(),
        "unrelated domain should never be in scope"
    );
}

#[test]
fn request_timeout_secs_rounds_up_with_minimum_one_second() {
    let cfg = Config {
        request_timeout_ms: Some(1),
        ..Config::default()
    };
    assert_eq!(request_timeout_secs(&cfg), 1);

    let cfg = Config {
        request_timeout_ms: Some(999),
        ..Config::default()
    };
    assert_eq!(request_timeout_secs(&cfg), 1);

    let cfg = Config {
        request_timeout_ms: Some(1_001),
        ..Config::default()
    };
    assert_eq!(request_timeout_secs(&cfg), 2);
}

#[test]
fn should_retry_status_permanent_dead_hosts_not_retried() {
    use reqwest::StatusCode;
    // 525 (DNS/NXDOMAIN) and 526 (host/TLS unreachable) are permanent —
    // retrying re-resolves a host that will never resolve (dead-host bead axon_rust-6i30).
    assert!(!should_retry_status(StatusCode::from_u16(525).unwrap()));
    assert!(!should_retry_status(StatusCode::from_u16(526).unwrap()));
}

#[test]
fn should_retry_status_transient_5xx_and_52x_still_retried() {
    use reqwest::StatusCode;
    // Genuine upstream 5xx and transient spider synthetic codes (refused /
    // timeout) remain retryable.
    assert!(should_retry_status(StatusCode::INTERNAL_SERVER_ERROR));
    assert!(should_retry_status(StatusCode::SERVICE_UNAVAILABLE));
    assert!(should_retry_status(StatusCode::TOO_MANY_REQUESTS));
    assert!(should_retry_status(StatusCode::from_u16(521).unwrap())); // refused
    assert!(should_retry_status(StatusCode::from_u16(524).unwrap())); // timeout
}

#[test]
fn should_retry_status_success_and_4xx_not_retried() {
    use reqwest::StatusCode;
    assert!(!should_retry_status(StatusCode::OK));
    assert!(!should_retry_status(StatusCode::NOT_FOUND));
    assert!(!should_retry_status(StatusCode::FORBIDDEN));
}

#[test]
fn markdown_url_uses_passthrough() {
    assert!(is_already_markdown("https://x.com/docs/api.md"));
    assert!(is_already_markdown("https://x.com/llms.txt"));
    assert!(is_already_markdown("https://x.com/a/b.MD")); // case-insensitive
    assert!(!is_already_markdown("https://x.com/docs/page"));
    assert!(!is_already_markdown("https://x.com/index.html"));
    // Query string is stripped before the extension check.
    assert!(is_already_markdown("https://x.com/a.md?v=1"));
    // .markdown extension is recognized alongside .md.
    assert!(is_already_markdown("https://x.com/a.markdown"));
    // Fragment is stripped before the (case-insensitive) extension check.
    assert!(is_already_markdown("https://x.com/a.MD#h"));
}

#[tokio::test]
#[serial_test::serial]
async fn fetch_text_rejects_oversized_body() {
    let server = httpmock::MockServer::start();
    let big = "x".repeat(600 * 1024); // 600 KB > 512 KB cap
    let m = server.mock(|when, then| {
        when.method(httpmock::Method::GET).path("/big.txt");
        then.status(200).body(&big);
    });
    let _loopback = LoopbackGuard::new(true);
    let client = build_client(5, None).unwrap();
    let url = server.url("/big.txt");
    let got = fetch_text_with_retry(&client, &url, 0, 0, Some(DISCOVERY_MAX_BODY_BYTES)).await;
    m.assert();
    assert!(
        got.is_none(),
        "oversized body (content-length fast-reject) must be rejected, not buffered"
    );
}

/// Mid-stream streamed-abort path: the body exceeds the cap but the server omits
/// Content-Length (chunked), so the fast-reject can't fire — the abort must happen
/// mid-stream during chunk accumulation. Distinct from the content-length fast-reject.
#[tokio::test]
#[serial_test::serial]
async fn fetch_text_rejects_oversized_body_without_content_length() {
    let server = httpmock::MockServer::start();
    let big = "x".repeat(600 * 1024); // 600 KB > 512 KB cap
    let m = server.mock(|when, then| {
        when.method(httpmock::Method::GET).path("/chunked.txt");
        // No explicit content-length header; chunked transfer forces the streamed path.
        then.status(200)
            .header("transfer-encoding", "chunked")
            .body(&big);
    });
    let _loopback = LoopbackGuard::new(true);
    let client = build_client(5, None).unwrap();
    let url = server.url("/chunked.txt");
    let got = fetch_text_with_retry(&client, &url, 0, 0, Some(DISCOVERY_MAX_BODY_BYTES)).await;
    m.assert();
    assert!(
        got.is_none(),
        "oversized chunked body must be rejected mid-stream, not buffered"
    );
}
