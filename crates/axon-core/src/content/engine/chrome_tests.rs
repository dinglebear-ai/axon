use super::*;
use httpmock::{Method::GET, MockServer};

#[tokio::test]
async fn authenticated_extract_uses_the_local_spider_relay() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream = format!(
        "ws://{}/devtools/browser/extract-contract",
        listener.local_addr().unwrap()
    );
    let connection = spider_connection_url(&upstream, &upstream, Some("extract-contract-token"))
        .await
        .expect("authenticated extract configuration must succeed");
    assert_ne!(connection, upstream);
    assert!(connection.starts_with("ws://127.0.0.1:"));
}

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

    let resolved = resolve_chrome_url(&server.base_url()).await.unwrap();

    assert_eq!(resolved, websocket_url);
    discovery.assert_async().await;
}
