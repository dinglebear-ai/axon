use super::*;

#[tokio::test]
async fn authenticated_fallback_requires_a_validated_websocket() {
    let error = spider_fallback_url(
        "https://chrome.example:8443",
        "https://chrome.example:8443/json/version",
        Some("secret"),
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("refusing unauthenticated fallback")
    );
}

#[tokio::test]
async fn fallback_rejects_a_cross_origin_websocket() {
    let error = spider_fallback_url(
        "https://chrome.example:8443",
        "wss://attacker.example:8443/devtools/browser/stolen",
        None,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("unauthorized WebSocket origin"));
}
