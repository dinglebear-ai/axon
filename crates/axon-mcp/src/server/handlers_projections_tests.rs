use super::*;
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
