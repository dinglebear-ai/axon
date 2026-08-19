//! External-caller smoke for the pipeline DTO contract.

use axon_api::source::{JobPriority, SourceRequest, SourceScope};

#[test]
fn canonical_source_request_is_constructible_by_external_callers() {
    let mut request = SourceRequest::new("https://example.com/docs");
    request.scope = Some(SourceScope::Site);
    request.execution.priority = JobPriority::High;
    assert_eq!(request.scope, Some(SourceScope::Site));
    assert_eq!(request.execution.priority, JobPriority::High);
}
