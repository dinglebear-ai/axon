use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn chunked_response() -> reqwest::Response {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut request = [0_u8; 1024];
        let _ = socket.read(&mut request).await.expect("read request");
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nabc\r\n3\r\ndef\r\n0\r\n\r\n",
            )
            .await
            .expect("write response");
    });
    reqwest::Client::new()
        .get(format!("http://{address}/"))
        .send()
        .await
        .expect("response")
}

#[tokio::test]
async fn bounded_reader_rejects_chunked_body_as_soon_as_budget_is_exceeded() {
    let response = chunked_response().await;
    let error = read_response_bytes_bounded(response, 4)
        .await
        .expect_err("second chunk must exceed the budget");
    assert!(matches!(
        error,
        HttpError::ResponseTooLarge { max_bytes: 4 }
    ));
}

#[tokio::test]
async fn bounded_text_reader_accepts_chunked_body_within_budget() {
    let response = chunked_response().await;
    assert_eq!(
        read_response_text_bounded(response, 6).await.expect("body"),
        "abcdef"
    );
}

async fn encoded_response() -> reqwest::Response {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut request = [0_u8; 1024];
        let _ = socket.read(&mut request).await.expect("read request");
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=windows-1252\r\nContent-Length: 1\r\nConnection: close\r\n\r\n\xe9",
            )
            .await
            .expect("write response");
    });
    reqwest::Client::new()
        .get(format!("http://{address}/"))
        .send()
        .await
        .expect("response")
}

#[tokio::test]
async fn bounded_text_reader_preserves_content_type_charset_decoding() {
    let response = encoded_response().await;
    assert_eq!(
        read_response_text_bounded(response, 1).await.expect("body"),
        "é"
    );
}

#[tokio::test]
async fn bounded_json_reader_deserializes_within_budget() {
    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Payload {
        ok: bool,
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut request = [0_u8; 1024];
        let _ = socket.read(&mut request).await.expect("read request");
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\nB\r\n{\"ok\":true}\r\n0\r\n\r\n",
            )
            .await
            .expect("write response");
    });
    let response = reqwest::Client::new()
        .get(format!("http://{address}/"))
        .send()
        .await
        .expect("response");

    let payload: Payload = read_response_json_bounded(response, 11)
        .await
        .expect("json");
    assert_eq!(payload, Payload { ok: true });
}

#[test]
fn default_response_budget_is_finite() {
    assert_eq!(DEFAULT_MAX_RESPONSE_BODY_BYTES, 64 * 1024 * 1024);
}
