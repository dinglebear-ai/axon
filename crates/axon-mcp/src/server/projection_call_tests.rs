use super::*;
use axon_core::config::McpToolProjection;

fn server(projection: McpToolProjection) -> AxonMcpServer {
    AxonMcpServer::new(Config {
        mcp_tool_projection: projection,
        ..Config::default()
    })
}

#[test]
fn atomic_tool_call_preserves_presented_name_and_resolves_action() {
    let server = server(McpToolProjection::Atomic);
    let mut request = CallToolRequestParams::new("status");

    projection::normalize_projected_tool_call(&server, &mut request)
        .expect("atomic status should normalize");

    assert_eq!(request.name, "status");
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
    let server = server(McpToolProjection::Both);

    let mut legacy = CallToolRequestParams::new("axon").with_arguments(serde_json::Map::from_iter(
        [("action".to_string(), Value::String("status".to_string()))],
    ));
    projection::normalize_projected_tool_call(&server, &mut legacy)
        .expect("legacy call should remain accepted");
    assert_eq!(legacy.name, "axon");

    let mut atomic = CallToolRequestParams::new("status");
    projection::normalize_projected_tool_call(&server, &mut atomic)
        .expect("atomic call should be accepted");
    assert_eq!(atomic.name, "status");
}

#[test]
fn wrong_projection_surface_is_rejected() {
    let mut atomic_in_legacy = CallToolRequestParams::new("status");
    assert!(
        projection::normalize_projected_tool_call(
            &server(McpToolProjection::Legacy),
            &mut atomic_in_legacy,
        )
        .is_err()
    );

    let mut legacy_in_atomic = CallToolRequestParams::new("axon");
    assert!(
        projection::normalize_projected_tool_call(
            &server(McpToolProjection::Atomic),
            &mut legacy_in_atomic,
        )
        .is_err()
    );
}

#[test]
fn auxiliary_router_tool_passes_through_in_atomic_mode() {
    let server = server(McpToolProjection::Atomic);
    let mut request = CallToolRequestParams::new("axon_status_dashboard");

    projection::normalize_projected_tool_call(&server, &mut request)
        .expect("auxiliary router tool should remain callable");

    assert_eq!(request.name, "axon_status_dashboard");
    assert!(request.arguments.is_none());
}

#[test]
fn atomic_tool_rejects_conflicting_action_argument() {
    let server = server(McpToolProjection::Atomic);
    let mut request = CallToolRequestParams::new("status").with_arguments(
        serde_json::Map::from_iter([("action".to_string(), Value::String("query".to_string()))]),
    );

    assert!(projection::normalize_projected_tool_call(&server, &mut request).is_err());
}

#[test]
fn leaf_fixed_fields_are_rejected_even_when_equal_null_or_non_string() {
    let server = server(McpToolProjection::Atomic);
    for field in ["action", "subaction"] {
        for value in [
            serde_json::json!("jobs"),
            serde_json::json!("get"),
            serde_json::json!(null),
            serde_json::json!(42),
            serde_json::json!({}),
        ] {
            let mut request = CallToolRequestParams::new("jobs_get")
                .with_arguments(serde_json::Map::from_iter([(field.to_owned(), value)]));
            assert!(projection::normalize_projected_tool_call(&server, &mut request).is_err());
        }
    }
}

#[test]
fn leaf_resolution_preserves_metadata_and_rejects_auxiliary_task_spoofing() {
    let server = server(McpToolProjection::Atomic);
    let mut request: CallToolRequestParams = serde_json::from_value(serde_json::json!({
        "name":"extract_start", "arguments":{"urls":["https://example.com"], "prompt":"extract"},
        "_meta":{"progressToken":"owned-progress", "io.modelcontextprotocol/tasks":{}}
    }))
    .unwrap();
    let metadata = request.meta.clone();
    assert!(projection::normalize_projected_tool_call(&server, &mut request).unwrap());
    assert_eq!(request.name, "extract_start");
    assert_eq!(request.arguments.as_ref().unwrap()["subaction"], "start");
    assert_eq!(request.meta, metadata);
    let mut auxiliary = CallToolRequestParams::new("axon_status_dashboard").with_arguments(
        serde_json::from_value(serde_json::json!({"action":"extract", "subaction":"start"}))
            .unwrap(),
    );
    assert!(!projection::normalize_projected_tool_call(&server, &mut auxiliary).unwrap());
}
