use super::*;

#[tokio::test]
async fn attach_failure_closes_the_created_target() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = tokio_tungstenite::accept_async(stream).await.unwrap();

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
    let (stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/"))
        .await
        .unwrap();
    let (mut tx, mut rx) = stream.split();

    let error = attach_capture_target(&mut tx, &mut rx, "target-1", Duration::from_secs(2))
        .await
        .expect_err("attach without a session id must fail");
    assert!(error.contains("no sessionId"));
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
