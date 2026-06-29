use super::*;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[test]
fn parse_request_target_extracts_path_and_query() {
    assert_eq!(
        parse_request_target("GET /callback?code=abc&state=xyz HTTP/1.1"),
        Some("/callback?code=abc&state=xyz")
    );
    assert_eq!(parse_request_target("GET / HTTP/1.1"), Some("/"));
    assert_eq!(parse_request_target("garbage"), None);
    assert_eq!(parse_request_target("GET notapath HTTP/1.1"), None);
}

#[test]
fn parse_callback_params_reads_code_state_and_error() {
    let ok = parse_callback_params("/callback?code=abc&state=xyz");
    assert_eq!(ok.code.as_deref(), Some("abc"));
    assert_eq!(ok.state.as_deref(), Some("xyz"));
    assert!(ok.error.is_none());

    let denied = parse_callback_params("/callback?error=access_denied&state=xyz");
    assert_eq!(denied.error.as_deref(), Some("access_denied"));
    assert!(denied.code.is_none());

    let empty = parse_callback_params("/callback");
    assert!(empty.code.is_none() && empty.state.is_none() && empty.error.is_none());
}

async fn send_request(port: u16, line: &str) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream
        .write_all(format!("{line}\r\nHost: localhost\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf).await; // drain so the server write completes
}

#[tokio::test]
async fn await_code_returns_code_for_matching_state() {
    let listener = bind().await.unwrap();
    let port = listener.listener.local_addr().unwrap().port();
    assert_eq!(
        listener.redirect_uri,
        format!("http://localhost:{port}/callback")
    );

    let client = tokio::spawn(async move {
        send_request(port, "GET /callback?code=the-code&state=expected HTTP/1.1").await;
    });
    let code = listener
        .await_code("expected", Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(code, "the-code");
    client.await.unwrap();
}

#[tokio::test]
async fn await_code_ignores_state_mismatch_and_resolves_on_later_match() {
    // A racing/wrong-state request must NOT abort the flow; the real callback
    // (correct state) arriving afterward still resolves the login.
    let listener = bind().await.unwrap();
    let port = listener.listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        send_request(port, "GET /callback?code=evil&state=wrong HTTP/1.1").await;
        send_request(port, "GET /favicon.ico HTTP/1.1").await;
        send_request(port, "GET /callback?code=real-code&state=expected HTTP/1.1").await;
    });
    let code = listener
        .await_code("expected", Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(code, "real-code");
}

#[tokio::test]
async fn await_code_returns_error_for_matching_state_denial() {
    let listener = bind().await.unwrap();
    let port = listener.listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        send_request(
            port,
            "GET /callback?error=access_denied&state=expected HTTP/1.1",
        )
        .await;
    });
    let err = listener
        .await_code("expected", Duration::from_secs(5))
        .await
        .unwrap_err();
    assert!(err.contains("denied"), "unexpected error: {err}");
}
