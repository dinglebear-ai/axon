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
