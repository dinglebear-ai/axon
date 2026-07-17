use super::super::super::AxonMcpServer;
use super::super::super::system_requests::{ArtifactsMcpRequest, ArtifactsSubaction};
use super::MAX_INLINE_CONTENT_BYTES;
use axon_api::mcp_schema::ResponseMode;
use std::path::Path;

/// Seed one artifact (manifest + content file) into `<output_dir>/artifacts`
/// using the store's on-disk layout.
async fn seed_artifact(output_dir: &Path, artifact_id: &str, content: &[u8]) {
    let root = output_dir.join("artifacts");
    tokio::fs::create_dir_all(&root).await.unwrap();
    tokio::fs::write(root.join(format!("{artifact_id}.bin")), content)
        .await
        .unwrap();
    tokio::fs::write(
        root.join(format!("{artifact_id}.json")),
        serde_json::to_vec(&serde_json::json!({
            "handle": {
                "artifact_id": artifact_id,
                "artifact_kind": "report",
                "uri": format!("artifact://{artifact_id}")
            },
            "content_type": "text/plain",
            "content_path": format!("{artifact_id}.bin"),
            "content_kind": "inline_bytes",
            "metadata": { "label": "report.txt", "producer_refs": ["job:test"] }
        }))
        .unwrap(),
    )
    .await
    .unwrap();
}

fn server_over(tmp: &tempfile::TempDir) -> AxonMcpServer {
    AxonMcpServer::new(axon_core::config::Config {
        qdrant_url: String::new(),
        tei_url: String::new(),
        sqlite_path: tmp.path().join("jobs.db"),
        output_dir: tmp.path().to_path_buf(),
        ..axon_core::config::Config::default()
    })
}

fn content_request(artifact_id: &str) -> ArtifactsMcpRequest {
    ArtifactsMcpRequest {
        subaction: Some(ArtifactsSubaction::Content),
        artifact_id: Some(artifact_id.to_string()),
        source_id: None,
        job_id: None,
        kind: None,
        limit: None,
        cursor: None,
        // Inline keeps the test hermetic: path mode would write a response
        // artifact outside the tempdir.
        response_mode: Some(ResponseMode::Inline),
    }
}

#[tokio::test]
async fn content_under_the_inline_cap_is_returned_inline_as_utf8() {
    let tmp = tempfile::tempdir().expect("tempdir");
    seed_artifact(tmp.path(), "art_report_small", b"hello artifact").await;
    let server = server_over(&tmp);

    let response = server
        .handle_artifacts(content_request("art_report_small"))
        .await
        .expect("under-cap content is served inline");

    let inline = response
        .data
        .get("inline")
        .expect("inline response payload");
    assert_eq!(
        inline.pointer("/encoding").and_then(|v| v.as_str()),
        Some("utf8")
    );
    assert_eq!(
        inline.pointer("/content").and_then(|v| v.as_str()),
        Some("hello artifact")
    );
    assert_eq!(
        inline.pointer("/size_bytes").and_then(|v| v.as_u64()),
        Some(14)
    );
}

#[tokio::test]
async fn content_over_the_inline_cap_is_rejected_before_reading() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let oversized = vec![b'x'; (MAX_INLINE_CONTENT_BYTES + 1) as usize];
    seed_artifact(tmp.path(), "art_report_big", &oversized).await;
    let server = server_over(&tmp);

    let err = server
        .handle_artifacts(content_request("art_report_big"))
        .await
        .expect_err("over-cap content must be refused inline");

    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    assert!(
        err.message.contains("/v1/artifacts/art_report_big/content"),
        "cap refusal must point at the REST content route; got: {}",
        err.message
    );
    assert!(
        err.message.contains(&MAX_INLINE_CONTENT_BYTES.to_string()),
        "cap refusal must state the inline byte cap; got: {}",
        err.message
    );
}

#[tokio::test]
async fn missing_artifact_is_invalid_params_but_store_faults_are_internal_and_path_free() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Manifest present but its content file is missing: the service surfaces
    // artifact.read_failed (a store fault), which must map to a redacted
    // internal error rather than invalid params echoing the server path.
    let root = tmp.path().join("artifacts");
    tokio::fs::create_dir_all(&root).await.unwrap();
    tokio::fs::write(
        root.join("art_report_orphan.json"),
        serde_json::to_vec(&serde_json::json!({
            "handle": {
                "artifact_id": "art_report_orphan",
                "artifact_kind": "report",
                "uri": "artifact://art_report_orphan"
            },
            "content_type": "text/plain",
            "content_path": "art_report_orphan.bin",
            "content_kind": "inline_bytes",
            "metadata": {}
        }))
        .unwrap(),
    )
    .await
    .unwrap();
    let server = server_over(&tmp);

    let not_found = server
        .handle_artifacts(content_request("art_report_absent"))
        .await
        .expect_err("unknown artifact id is a caller error");
    assert_eq!(not_found.code, rmcp::model::ErrorCode::INVALID_PARAMS);

    let store_fault = server
        .handle_artifacts(content_request("art_report_orphan"))
        .await
        .expect_err("missing content file is a store fault");
    assert_eq!(store_fault.code, rmcp::model::ErrorCode::INTERNAL_ERROR);
    let tmp_path = tmp.path().to_string_lossy().to_string();
    assert!(
        !store_fault.message.contains(&tmp_path),
        "store-fault message must not leak the server filesystem path; got: {}",
        store_fault.message
    );
}
