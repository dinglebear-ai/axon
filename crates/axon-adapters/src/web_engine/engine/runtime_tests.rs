use super::*;
use httpmock::{Method::GET, MockServer};

#[tokio::test]
async fn chrome_discovery_allows_a_trusted_loopback_provider() {
    let server = MockServer::start_async().await;
    let websocket_url = format!("ws://{}/devtools/browser/trusted", server.address());
    let discovery = server
        .mock_async(|when, then| {
            when.method(GET).path("/json/version");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "webSocketDebuggerUrl": websocket_url,
                }));
        })
        .await;

    let resolved = resolve_cdp_ws_url(&server.base_url()).await;

    assert_eq!(resolved.as_deref(), Some(websocket_url.as_str()));
    discovery.assert_async().await;
}
