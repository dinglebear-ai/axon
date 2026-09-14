use super::*;

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
