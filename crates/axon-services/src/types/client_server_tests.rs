use super::*;
use axon_api::mcp_schema::{AxonRequest, StatusRequest};

#[test]
fn client_server_envelope_serializes_nested_axon_request() {
    let request = ClientActionRequest {
        request_id: "req-1".to_string(),
        action: AxonRequest::Status(StatusRequest {
            response_mode: None,
        }),
    };

    let value = match serde_json::to_value(&request) {
        Ok(value) => value,
        Err(err) => panic!("serialize request failed: {err}"),
    };

    assert_eq!(value["request_id"], "req-1");
    assert_eq!(value["action"]["action"], "status");
}

#[test]
fn client_server_response_includes_server_info() {
    let response = ClientActionResponse::ok(
        "req-2".to_string(),
        serde_json::json!({ "totals": { "crawl": 0 } }),
    );

    assert!(response.ok);
    assert_eq!(response.request_id.as_deref(), Some("req-2"));
    assert_eq!(
        response.server.schema_version,
        CLIENT_SERVER_SCHEMA_VERSION.to_string()
    );
    assert!(
        response
            .server
            .supported_routes
            .contains(&"GET /v1/status".to_string())
    );
    assert!(
        response
            .server
            .supported_actions
            .contains(&"status".to_string())
    );
    assert!(
        response
            .server
            .supported_actions
            .contains(&"source".to_string())
    );
    for removed in [
        "crawl.start",
        "crawl.status",
        "embed.start",
        "embed.status",
        "ingest.start",
        "ingest.status",
        "extract.status",
        "extract.cancel",
        "extract.cleanup",
        "extract.recover",
    ] {
        assert!(
            !response
                .server
                .supported_actions
                .contains(&removed.to_string()),
            "legacy client-server action still advertised: {removed}"
        );
    }
}

#[test]
fn rest_capabilities_omit_action_contract() {
    let info = ServerInfo::rest_capabilities();

    assert!(info.required_request_fields.is_empty());
    assert!(info.supported_actions.is_empty());
    assert!(
        info.supported_routes
            .contains(&"POST /v1/sources".to_string())
    );

    let value = serde_json::to_value(&info).expect("serialize server info");
    assert!(value.get("supported_routes").is_some());
    assert!(value.get("required_request_fields").is_none());
    assert!(value.get("supported_actions").is_none());
}
