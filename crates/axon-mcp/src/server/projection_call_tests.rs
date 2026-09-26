use super::*;
use axon_core::config::McpProjection;

fn server(projection: McpProjection) -> AxonMcpServer {
    AxonMcpServer::new(Config {
        mcp_projection: projection,
        ..Config::default()
    })
}

#[test]
fn atomic_tool_call_normalizes_to_legacy_router() {
    let server = server(McpProjection::Atomic);
    let mut request = CallToolRequestParams::new("axon_status");

    projection::normalize_projected_tool_call(&server, &mut request)
        .expect("atomic status should normalize");

    assert_eq!(request.name, "axon");
    assert_eq!(
        request
            .arguments
            .as_ref()
            .and_then(|args| args.get("action"))
            .and_then(Value::as_str),
        Some("status")
    );
}

#[test]
fn both_projection_accepts_legacy_and_atomic_calls() {
    let server = server(McpProjection::Both);

    let mut legacy = CallToolRequestParams::new("axon").with_arguments(serde_json::Map::from_iter(
        [("action".to_string(), Value::String("status".to_string()))],
    ));
    projection::normalize_projected_tool_call(&server, &mut legacy)
        .expect("legacy call should remain accepted");
    assert_eq!(legacy.name, "axon");

    let mut atomic = CallToolRequestParams::new("axon_status");
    projection::normalize_projected_tool_call(&server, &mut atomic)
        .expect("atomic call should be accepted");
    assert_eq!(atomic.name, "axon");
}

#[test]
fn wrong_projection_surface_is_rejected() {
    let mut atomic_in_legacy = CallToolRequestParams::new("axon_status");
    assert!(
        projection::normalize_projected_tool_call(
            &server(McpProjection::Legacy),
            &mut atomic_in_legacy,
        )
        .is_err()
    );

    let mut legacy_in_atomic = CallToolRequestParams::new("axon");
    assert!(
        projection::normalize_projected_tool_call(
            &server(McpProjection::Atomic),
            &mut legacy_in_atomic,
        )
        .is_err()
    );
}

#[test]
fn auxiliary_router_tool_passes_through_in_atomic_mode() {
    let server = server(McpProjection::Atomic);
    let mut request = CallToolRequestParams::new("axon_status_dashboard");

    projection::normalize_projected_tool_call(&server, &mut request)
        .expect("auxiliary router tool should remain callable");

    assert_eq!(request.name, "axon_status_dashboard");
    assert!(request.arguments.is_none());
}

#[test]
fn atomic_tool_rejects_conflicting_action_argument() {
    let server = server(McpProjection::Atomic);
    let mut request = CallToolRequestParams::new("axon_status").with_arguments(
        serde_json::Map::from_iter([("action".to_string(), Value::String("query".to_string()))]),
    );

    assert!(projection::normalize_projected_tool_call(&server, &mut request).is_err());
}
