use super::*;

#[tokio::test]
async fn mcp_ingest_start_maps_service_parser_errors_to_invalid_params() {
    let server = AxonMcpServer::new(axon_core::config::Config::default());
    let req = IngestRequest {
        source_type: Some(crate::schema::IngestSourceType::Github),
        target: Some("owner/repo/extra".to_string()),
        ..Default::default()
    };

    let result = server.handle_ingest(req).await;
    assert!(result.is_err(), "invalid target should fail");
    let err = result.unwrap_err();

    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
}

#[tokio::test]
async fn embed_start_local_path_runs_in_process_not_enqueued() {
    // Pins the behavioral guarantee this fix exists for: a local filesystem path
    // must be embedded in-process, never enqueued onto the shared jobs DB where a
    // worker that cannot see the path could claim it.
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("note.md"), "hello world").expect("write file");

    let cfg = axon_core::config::Config {
        mcp_embed_allowed_roots: vec![dir.path().to_path_buf()],
        tei_url: String::new(), // force the in-process embed to fail fast
        ..axon_core::config::Config::default()
    };

    let server = AxonMcpServer::new(cfg);
    let file = dir.path().join("note.md").to_string_lossy().to_string();

    let result = server.handle_embed_start(Some(file), None, None).await;

    // The in-process embed reaches the embed pipeline and errors ("TEI_URL not
    // configured"). The enqueue path returns Ok({job_id}) WITHOUT touching TEI, so
    // an error here proves the local path was embedded in-process, not enqueued. A
    // branch inversion / dropped early return / reordering would make this Ok.
    let err = result.expect_err("local path must run in-process and surface the TEI error");
    assert!(
        err.message.contains("TEI_URL not configured")
            || err.code == rmcp::model::ErrorCode::INTERNAL_ERROR,
        "expected an in-process embed failure, got: {err:?}"
    );
}
