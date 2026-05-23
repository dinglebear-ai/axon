use super::*;

#[tokio::test]
async fn mcp_ingest_start_maps_service_parser_errors_to_invalid_params() {
    let server = AxonMcpServer::new(crate::core::config::Config::default());
    let req = IngestRequest {
        source_type: Some(crate::mcp::schema::IngestSourceType::Github),
        target: Some("owner/repo/extra".to_string()),
        ..Default::default()
    };

    let result = server.handle_ingest(req).await;
    assert!(result.is_err(), "invalid target should fail");
    let err = result.unwrap_err();

    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
}
