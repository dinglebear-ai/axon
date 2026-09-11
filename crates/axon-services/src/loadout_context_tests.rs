use super::*;

fn capability(member_id: &str) -> CapabilityRef {
    CapabilityRef {
        provider: "labby:test".into(),
        family: "skill".into(),
        member_id: member_id.into(),
        expected_revision: "r1".into(),
    }
}

#[test]
fn context_is_deterministic_and_sorted() {
    let first = build_context(&[capability("z"), capability("a")]).unwrap();
    let second = build_context(&[capability("a"), capability("z")]).unwrap();
    assert_eq!(first, second);
    assert!(first.find(":a@").unwrap() < first.find(":z@").unwrap());
    assert!(first.contains("trust=\"untrusted_metadata_only\""));
}

#[test]
fn binding_limits_fail_closed() {
    let binding = LoadoutBinding {
        integration_id: "x".repeat(257),
        loadout_id: "loadout".into(),
        expected_revision: 1,
        conversation_binding: None,
    };
    assert!(
        validate_binding(&binding)
            .unwrap_err()
            .to_string()
            .contains("integration_id")
    );
}

#[test]
fn execution_context_id_is_revision_and_generation_bound() {
    let preview = Preview {
        loadout_id: "loadout".into(),
        active_revision: 2,
        catalog_generation: "catalog-a".into(),
        runtime_identity: "axon".into(),
        effective: vec![],
        missing: vec![],
        conflicts: vec![],
    };
    assert_ne!(
        context_id("labby", &preview, 1),
        context_id("labby", &preview, 2)
    );
}

#[test]
fn oversized_loadout_body_keeps_stable_error_code() {
    let error = loadout_body_error(axon_core::http::HttpError::ResponseTooLarge { max_bytes: 1 });
    assert!(error.to_string().starts_with("loadout_payload_too_large:"));
}

#[tokio::test]
async fn loadout_resolution_timeout_includes_slow_response_body() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut request = [0_u8; 1024];
        let _ = socket.read(&mut request).await.expect("read request");
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n2\r\n{}\r\n",
            )
            .await
            .expect("write first chunk");
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = socket.write_all(b"0\r\n\r\n").await;
    });

    let request = reqwest::Client::new().get(format!("http://{address}/"));
    let error = read_loadout_response(request, Duration::from_millis(20), 1024)
        .await
        .expect_err("body read must remain within the configured timeout");
    assert_eq!(
        error.to_string(),
        "loadout_unavailable: Labby resolution timed out"
    );
}
