use super::*;

#[test]
fn normal_browser_work_retains_the_network_idle_timeout_floor() {
    let cfg = Config {
        render_mode: RenderMode::Chrome,
        request_timeout_ms: Some(20_000),
        chrome_network_idle_timeout_secs: 15,
        ..Config::default()
    };
    let mut website = Website::new("https://example.com/");

    apply_spider_browser_defaults_with_timeout(
        &cfg,
        &mut website,
        RenderMode::Chrome,
        BrowserTimeoutPolicy::FloorForBrowserWork,
    );

    assert_eq!(
        website.configuration.request_timeout,
        Some(Duration::from_secs(45))
    );
}

#[test]
fn map_browser_work_can_request_an_exact_short_deadline() {
    let cfg = Config {
        render_mode: RenderMode::Chrome,
        request_timeout_ms: Some(8_000),
        chrome_network_idle_timeout_secs: 1,
        ..Config::default()
    };
    let mut website = Website::new("https://example.com/");

    apply_spider_browser_defaults_with_timeout(
        &cfg,
        &mut website,
        RenderMode::Chrome,
        BrowserTimeoutPolicy::Exact,
    );

    assert_eq!(
        website.configuration.request_timeout,
        Some(Duration::from_secs(8))
    );
}

#[tokio::test]
async fn preresolved_ws_url_is_wired_as_the_chrome_connection() {
    let ws = "ws://127.0.0.1:1/devtools/browser/test";
    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        chrome_remote_url: Some(ws.to_string()),
        ..Config::default()
    };
    let website = Website::new("https://example.com/");

    // AutoSwitch skips the Chrome-mode `.build()` so no browser is launched.
    let website = configure_spider_browser(
        &cfg,
        website,
        RenderMode::AutoSwitch,
        BrowserTimeoutPolicy::FloorForBrowserWork,
    )
    .await
    .expect("configure must succeed");

    assert_eq!(
        website.configuration.chrome_connection_url.as_deref(),
        Some(ws)
    );
}

#[tokio::test]
async fn unreachable_remote_leaves_the_chrome_connection_unset() {
    // Inside Docker the probe is skipped and the discovery URL is handed to
    // spider as-is — this test covers the host path only.
    if super::super::engine::cdp_probe_skipped_in_docker() {
        return;
    }

    let cfg = Config {
        render_mode: RenderMode::AutoSwitch,
        chrome_remote_url: Some("http://127.0.0.1:9".to_string()),
        ..Config::default()
    };
    let website = Website::new("https://example.com/");

    let website = configure_spider_browser(
        &cfg,
        website,
        RenderMode::AutoSwitch,
        BrowserTimeoutPolicy::FloorForBrowserWork,
    )
    .await
    .expect("configure must succeed");

    // The dead endpoint must NOT be wired in: spider would redial it and then
    // degrade to a browserless HTTP crawl instead of launching local Chrome.
    assert!(website.configuration.chrome_connection_url.is_none());
}
