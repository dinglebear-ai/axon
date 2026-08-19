//! External-caller smoke for transport-neutral source orchestration.

use axon_api::source::{SourceKind, SourceRequest};

#[test]
fn canonical_source_router_is_reachable_from_external_callers() {
    let routed = axon_services::source::routing::resolve_source_route(&SourceRequest::new(
        "https://example.com/docs",
    ))
    .expect("web source should route");
    assert_eq!(routed.kind, SourceKind::Web);
}

#[test]
fn canonical_source_result_helpers_are_reachable_from_external_callers() {
    let result = axon_services::source::result_map::unsupported_result(
        "unsupported:fixture",
        "integration fixture",
    );
    assert!(!result.source_id.0.is_empty());
}
