use super::*;
use axon_api::mcp_schema::{AxonRequest, StatusRequest};
use std::path::Path;

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

#[test]
fn artifact_handle_serializes_root_relative_identifier() {
    let handle = ArtifactHandle::new(
        "json",
        "crawl/status.json",
        "/srv/axon/artifacts/crawl/status.json",
        128,
        Some(12),
        Some("job-1".to_string()),
        Some("https://example.com".to_string()),
    );

    let value = serde_json::to_value(&handle).expect("serialize handle");
    assert_eq!(value["kind"], "json");
    assert_eq!(value["relative_path"], "crawl/status.json");
    assert_eq!(
        value["display_path"],
        "/srv/axon/artifacts/crawl/status.json"
    );
    assert_eq!(value["bytes"], 128);
    assert_eq!(value["line_count"], 12);
    assert_eq!(value["job_id"], "job-1");
    assert_eq!(value["url"], "https://example.com");
}

#[test]
fn artifact_handle_from_path_refuses_outside_root() {
    let root = Path::new("/srv/axon/artifacts");
    let inside = Path::new("/srv/axon/artifacts/scrape/page.json");
    let outside = Path::new("/tmp/page.json");
    let traversal = Path::new("/srv/axon/artifacts/../outside.json");

    let handle = ArtifactHandle::try_from_path("json", root, inside, 10, Some(1), None, None)
        .expect("inside root");
    assert_eq!(handle.relative_path(), "scrape/page.json");

    assert!(
        ArtifactHandle::try_from_path("json", root, outside, 10, Some(1), None, None).is_none()
    );
    assert!(
        ArtifactHandle::try_from_path("json", root, traversal, 10, Some(1), None, None).is_none()
    );
}
