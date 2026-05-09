use crate::mcp::schema::AxonRequest;
use serde::{Deserialize, Serialize};
use std::path::Path;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactHandle {
    pub kind: String,
    pub relative_path: String,
    pub display_path: String,
    pub bytes: u64,
    pub line_count: Option<u64>,
    pub job_id: Option<String>,
    pub url: Option<String>,
}

impl ArtifactHandle {
    pub fn new(
        kind: impl Into<String>,
        relative_path: impl Into<String>,
        display_path: impl Into<String>,
        bytes: u64,
        line_count: Option<u64>,
        job_id: Option<String>,
        url: Option<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            relative_path: normalize_relative_path(relative_path.into()),
            display_path: display_path.into(),
            bytes,
            line_count,
            job_id,
            url,
        }
    }

    pub fn try_from_path(
        kind: impl Into<String>,
        root: &Path,
        path: &Path,
        bytes: u64,
        line_count: Option<u64>,
        job_id: Option<String>,
        url: Option<String>,
    ) -> Option<Self> {
        if !path.is_absolute() || !root.is_absolute() {
            return None;
        }
        let relative_path = path
            .strip_prefix(root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        if relative_path_is_unsafe(&relative_path) {
            return None;
        }
        Some(Self::new(
            kind,
            relative_path,
            path.to_string_lossy().into_owned(),
            bytes,
            line_count,
            job_id,
            url,
        ))
    }
}

fn normalize_relative_path(path: String) -> String {
    path.replace('\\', "/").trim_start_matches('/').to_string()
}

fn relative_path_is_unsafe(path: &str) -> bool {
    Path::new(path).components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    })
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
        assert_eq!(handle.relative_path, "scrape/page.json");

        assert!(
            ArtifactHandle::try_from_path("json", root, outside, 10, Some(1), None, None).is_none()
        );
        assert!(
            ArtifactHandle::try_from_path("json", root, traversal, 10, Some(1), None, None)
                .is_none()
        );
    }
}
