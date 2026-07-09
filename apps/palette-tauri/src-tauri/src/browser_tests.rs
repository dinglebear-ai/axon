use super::*;

#[test]
fn validate_browser_url_accepts_https() {
    assert_eq!(
        validate_browser_url("https://example.com").unwrap(),
        "https://example.com"
    );
}

#[test]
fn validate_browser_url_accepts_http() {
    assert_eq!(
        validate_browser_url("http://127.0.0.1:8001").unwrap(),
        "http://127.0.0.1:8001"
    );
}

#[test]
fn validate_browser_url_accepts_about_blank() {
    assert_eq!(validate_browser_url("about:blank").unwrap(), "about:blank");
}

#[test]
fn validate_browser_url_trims_whitespace() {
    assert_eq!(
        validate_browser_url("  https://example.com  ").unwrap(),
        "https://example.com"
    );
}

#[test]
fn validate_browser_url_rejects_empty() {
    assert!(validate_browser_url("").is_err());
    assert!(validate_browser_url("   ").is_err());
}

#[test]
fn validate_browser_url_rejects_non_http_schemes() {
    assert!(validate_browser_url("file:///etc/passwd").is_err());
    assert!(validate_browser_url("tauri://localhost").is_err());
    assert!(validate_browser_url("javascript:alert(1)").is_err());
    assert!(validate_browser_url("data:text/html,hi").is_err());
}

#[test]
fn validate_browser_url_rejects_unparseable_input() {
    assert!(validate_browser_url("not a url at all").is_err());
}

#[test]
fn webview_url_for_maps_https_to_external() {
    let webview_url = webview_url_for("https://example.com").expect("valid url");
    match webview_url {
        WebviewUrl::External(url) => assert_eq!(url.as_str(), "https://example.com/"),
        other => panic!("expected External variant, got {other:?}"),
    }
}

#[test]
fn webview_url_for_maps_about_blank_to_app() {
    let webview_url = webview_url_for("about:blank").expect("valid url");
    assert!(matches!(webview_url, WebviewUrl::App(_)));
}
