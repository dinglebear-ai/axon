use crate::mcp::schema::AxonRequest;
use serde::{Deserialize, Serialize};

pub const CLIENT_SERVER_SCHEMA_VERSION: &str = "client-server.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientActionRequest {
    pub request_id: String,
    pub action: AxonRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientActionResponse {
    pub request_id: Option<String>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ClientActionError>,
    pub server: ServerInfo,
}

impl ClientActionResponse {
    pub fn ok(request_id: String, result: serde_json::Value) -> Self {
        Self {
            request_id: Some(request_id),
            ok: true,
            result: Some(result),
            error: None,
            server: ServerInfo::current(),
        }
    }

    pub fn error(request_id: Option<String>, error: ClientActionError) -> Self {
        Self {
            request_id,
            ok: false,
            result: None,
            error: Some(error),
            server: ServerInfo::current(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientActionError {
    pub kind: String,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl ClientActionError {
    pub fn new(
        kind: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        hint: Option<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            retryable,
            hint,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerInfo {
    pub version: String,
    pub schema_version: String,
    pub minimum_client_schema_version: String,
    pub required_request_fields: Vec<String>,
    pub supported_actions: Vec<String>,
}

impl ServerInfo {
    pub fn current() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            schema_version: CLIENT_SERVER_SCHEMA_VERSION.to_string(),
            minimum_client_schema_version: CLIENT_SERVER_SCHEMA_VERSION.to_string(),
            required_request_fields: required_request_fields(),
            supported_actions: supported_actions(),
        }
    }
}

pub fn required_request_fields() -> Vec<String> {
    ["request_id", "action"]
        .into_iter()
        .map(ToString::to_string)
        .collect()
}

pub fn supported_actions() -> Vec<String> {
    [
        "status",
        "crawl.status",
        "crawl.list",
        "crawl.cancel",
        "crawl.cleanup",
        "crawl.clear",
        "crawl.recover",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::schema::{AxonRequest, StatusRequest};

    #[test]
    fn client_server_envelope_serializes_nested_axon_request() {
        let request = ClientActionRequest {
            request_id: "req-1".to_string(),
            action: AxonRequest::Status(StatusRequest {
                subaction: None,
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
                .supported_actions
                .contains(&"status".to_string())
        );
    }
}
