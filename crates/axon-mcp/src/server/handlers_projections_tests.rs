use super::*;

#[test]
fn idempotency_collision_is_a_typed_invalid_params_error() {
    let error = ApiError::new(
        "projection.idempotency_collision",
        ErrorStage::Storage,
        "idempotency key was already used for a different request",
    );

    let mapped = projection_execution_error(error);
    assert_eq!(mapped.code.0, -32602);
    let data = mapped.data.expect("typed collision details");
    assert_eq!(data["code"], "projection.idempotency_collision");
}
use axon_api::QueryResult;

#[test]
fn projection_actions_use_the_canonical_wire_names() {
    let result = BatchResult::<QueryResult> {
        batch_id: BatchId::new(uuid::Uuid::nil()),
        status: BatchStatus::Completed,
        items: Vec::new(),
        summary: BatchSummary {
            total: 0,
            completed: 0,
            queued: 0,
            failed: 0,
            canceled: 0,
        },
    };
    let response = projection_response(ProjectionOperation::CodeSearch, result).unwrap();
    assert_eq!(response.action, "code_search");
}

#[tokio::test]
async fn every_projection_handler_rejects_an_empty_batch_consistently() {
    let server = AxonMcpServer::new(axon_core::config::Config::default());
    let source_errors = [
        server
            .handle_scrape_projection(ScrapeRequest {
                inputs: vec![],
                options: ScrapeOptions::default(),
            })
            .await,
        server
            .handle_crawl_projection(CrawlRequest {
                inputs: vec![],
                options: CrawlOptions::default(),
            })
            .await,
        server
            .handle_embed_projection(EmbedRequest {
                inputs: vec![],
                options: EmbedOptions::default(),
            })
            .await,
        server
            .handle_ingest_projection(IngestRequest {
                inputs: vec![],
                options: IngestOptions::default(),
            })
            .await,
    ];
    for result in source_errors {
        assert_eq!(result.unwrap_err().code.0, -32602);
    }
    let code_search = server
        .handle_code_search_projection(CodeSearchRequest {
            inputs: vec![],
            options: CodeSearchProjectionOptions::default(),
        })
        .await
        .unwrap_err();
    assert_eq!(code_search.code.0, -32602);
}

#[tokio::test]
async fn projection_handler_rejects_oversized_input_before_runtime_access() {
    let server = AxonMcpServer::new(axon_core::config::Config::default());
    let error = server
        .handle_crawl_projection(CrawlRequest {
            inputs: vec![SourceProjectionInput {
                input: "x".repeat(16 * 1024 + 1),
                idempotency_key: None,
            }],
            options: CrawlOptions::default(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code.0, -32602);
}
