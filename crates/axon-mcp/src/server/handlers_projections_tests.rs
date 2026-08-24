use super::*;

#[test]
fn idempotency_collision_is_a_typed_invalid_params_error() {
    let error = ApiError::new(
        "projection.idempotency_collision",
        axon_error::ErrorStage::Storage,
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
