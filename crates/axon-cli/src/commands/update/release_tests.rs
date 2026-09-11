use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn chunked_update_download_is_bounded_and_partial_file_is_removed() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut request = [0_u8; 1024];
        let _ = socket.read(&mut request).await.expect("read request");
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\nA\r\n0123456789\r\n0\r\n\r\n",
            )
            .await
            .expect("write response");
    });

    let output = tempfile::tempdir().expect("output");
    let dest = output.path().join("asset.tar.gz");
    let error = download_to_file_bounded(
        &reqwest::Client::new(),
        &format!("http://{address}/asset"),
        &dest,
        5,
    )
    .await
    .expect_err("chunked asset must not exceed its byte budget");

    assert!(error.to_string().contains("exceeds 5 byte limit"));
    assert!(!dest.exists(), "partial oversized download must be removed");
}
