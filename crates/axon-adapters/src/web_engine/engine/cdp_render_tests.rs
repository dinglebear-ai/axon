use super::*;
use futures_util::{SinkExt, StreamExt};
use spider_transformations::transformation::content::SelectorConfiguration;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn attach_failure_closes_the_created_target() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let websocket_url = format!("ws://{addr}/devtools/browser/test");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = tokio_tungstenite::accept_async(stream).await.unwrap();

        let create = read_command(&mut websocket).await;
        assert_eq!(create["method"], "Target.createTarget");
        reply(
            &mut websocket,
            &create,
            serde_json::json!({ "targetId": "target-1" }),
        )
        .await;

        let attach = read_command(&mut websocket).await;
        assert_eq!(attach["method"], "Target.attachToTarget");
        reply(&mut websocket, &attach, serde_json::json!({})).await;

        let close = read_command(&mut websocket).await;
        assert_eq!(close["method"], "Target.closeTarget");
        assert_eq!(close["params"]["targetId"], "target-1");
        reply(
            &mut websocket,
            &close,
            serde_json::json!({ "success": true }),
        )
        .await;
    });

    let error = open_chrome_session(
        &format!("http://{addr}"),
        &websocket_url,
        "https://example.com",
        Duration::from_secs(2),
        None,
    )
    .await
    .err()
    .expect("attach without a session id must fail");
    assert!(error.contains("empty sessionId"));
    server.await.unwrap();
}

async fn read_command<S>(websocket: &mut tokio_tungstenite::WebSocketStream<S>) -> serde_json::Value
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let Message::Text(text) = websocket.next().await.unwrap().unwrap() else {
        panic!("expected a text CDP command");
    };
    serde_json::from_str(&text).unwrap()
}

async fn reply<S>(
    websocket: &mut tokio_tungstenite::WebSocketStream<S>,
    command: &serde_json::Value,
    result: serde_json::Value,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    websocket
        .send(Message::Text(
            serde_json::json!({ "id": command["id"], "result": result })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
}

#[test]
fn markdown_if_not_thin_honors_selector_config() {
    let html = r#"
        <main>
            <h1>Keep this page</h1>
            <p>Enough selected content to pass the thin page threshold.</p>
            <aside>Drop this excluded navigation text</aside>
        </main>
        <footer>Drop this footer too</footer>
    "#;
    let selectors = SelectorConfiguration {
        root_selector: Some("main".to_string()),
        exclude_selector: Some("aside".to_string()),
    };
    let md = markdown_if_not_thin(html, 10, Some(&selectors)).expect("markdown");

    assert!(md.contains("Keep this page"));
    assert!(!md.contains("Drop this excluded navigation text"));
    assert!(!md.contains("Drop this footer too"));
}
