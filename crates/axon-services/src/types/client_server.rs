use crate::types::supported_routes;
use axon_api::mcp_schema::AxonRequest;
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct ServerInfo {
    pub version: String,
    pub schema_version: String,
    pub minimum_client_schema_version: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required_request_fields: Vec<String>,
    /// Legacy internal action names retained for the panel command path.
    ///
    /// Public HTTP clients should use `supported_routes`; `/v1/actions` is no
    /// longer mounted after the direct REST cutover.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub supported_actions: Vec<String>,
    pub supported_routes: Vec<String>,
}

impl ServerInfo {
    pub fn current() -> Self {
        Self::action_contract()
    }

    pub fn rest_capabilities() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            schema_version: CLIENT_SERVER_SCHEMA_VERSION.to_string(),
            minimum_client_schema_version: CLIENT_SERVER_SCHEMA_VERSION.to_string(),
            required_request_fields: Vec::new(),
            supported_actions: Vec::new(),
            supported_routes: supported_routes(),
        }
    }

    pub fn action_contract() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            schema_version: CLIENT_SERVER_SCHEMA_VERSION.to_string(),
            minimum_client_schema_version: CLIENT_SERVER_SCHEMA_VERSION.to_string(),
            required_request_fields: required_request_fields(),
            supported_actions: supported_actions(),
            supported_routes: supported_routes(),
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
        "source",
        "jobs.get",
        "jobs.events",
        "jobs.cancel",
        "jobs.retry",
        "jobs.cleanup",
        "jobs.recover",
        "extract.start",
        "watch",
        "memory",
        "prune",
        "ask",
        "brand",
        "diff",
        "doctor",
        "endpoints",
        "evaluate",
        "help",
        "map",
        "query",
        "research",
        "retrieve",
        "screenshot",
        "search",
        "sources",
        "domains",
        "stats",
        "suggest",
        "summarize",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

pub use axon_api::contract::ArtifactHandle;

#[cfg(test)]
#[path = "client_server_tests.rs"]
mod tests;
