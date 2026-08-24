use super::*;

#[test]
fn source_projection_http_status_matches_batch_state() {
    assert_eq!(
        source_projection_status(BatchStatus::Accepted),
        StatusCode::ACCEPTED
    );
    assert_eq!(
        source_projection_status(BatchStatus::Completed),
        StatusCode::OK
    );
    assert_eq!(
        source_projection_status(BatchStatus::CompletedDegraded),
        StatusCode::OK
    );
}

#[test]
fn code_search_contract_has_no_freshness_or_idempotency_controls() {
    let request = serde_json::to_value(CodeSearchRequest {
        inputs: vec![QueryProjectionInput {
            input: "needle".to_string(),
        }],
        options: CodeSearchProjectionOptions::default(),
    })
    .unwrap();
    assert!(request.get("ensure_fresh").is_none());
    assert!(request.get("idempotency_key").is_none());
}
