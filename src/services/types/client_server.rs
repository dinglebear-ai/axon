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
        Self::legacy_action_contract()
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

    pub fn legacy_action_contract() -> Self {
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
        "scrape",
        "summarize",
        "screenshot",
        "crawl.start",
        "crawl.status",
        "crawl.list",
        "crawl.cancel",
        "crawl.cleanup",
        "crawl.clear",
        "crawl.recover",
        "extract.start",
        "extract.status",
        "extract.list",
        "extract.cancel",
        "extract.cleanup",
        "extract.clear",
        "extract.recover",
        "embed.start",
        "embed.status",
        "embed.list",
        "embed.cancel",
        "embed.cleanup",
        "embed.clear",
        "embed.recover",
        "ingest.start",
        "ingest.status",
        "ingest.list",
        "ingest.cancel",
        "ingest.cleanup",
        "ingest.clear",
        "ingest.recover",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

pub fn supported_routes() -> Vec<String> {
    [
        "GET /healthz",
        "GET /readyz",
        "GET /v1/capabilities",
        "GET /v1/sources",
        "GET /v1/domains",
        "GET /v1/stats",
        "GET /v1/status",
        "GET /v1/doctor",
        "POST /v1/ask",
        "POST /v1/ask/stream",
        "POST /v1/chat",
        "POST /v1/chat/stream",
        "POST /v1/query",
        "POST /v1/retrieve",
        "POST /v1/evaluate",
        "POST /v1/suggest",
        "POST /v1/scrape",
        "POST /v1/summarize",
        "POST /v1/map",
        "POST /v1/endpoints",
        "POST /v1/brand",
        "POST /v1/diff",
        "POST /v1/screenshot",
        "POST /v1/search",
        "POST /v1/research",
        "POST /v1/crawl",
        "GET /v1/crawl",
        "GET /v1/crawl/{id}",
        "POST /v1/crawl/{id}/cancel",
        "POST /v1/crawl/cleanup",
        "DELETE /v1/crawl",
        "POST /v1/crawl/recover",
        "POST /v1/embed",
        "GET /v1/embed",
        "GET /v1/embed/{id}",
        "POST /v1/embed/{id}/cancel",
        "POST /v1/embed/cleanup",
        "DELETE /v1/embed",
        "POST /v1/embed/recover",
        "POST /v1/extract",
        "GET /v1/extract",
        "GET /v1/extract/{id}",
        "POST /v1/extract/{id}/cancel",
        "POST /v1/extract/cleanup",
        "DELETE /v1/extract",
        "POST /v1/extract/recover",
        "POST /v1/ingest",
        "POST /v1/ingest/sessions/prepared",
        "GET /v1/ingest",
        "GET /v1/ingest/{id}",
        "POST /v1/ingest/{id}/cancel",
        "POST /v1/ingest/cleanup",
        "DELETE /v1/ingest",
        "POST /v1/ingest/recover",
        "POST /v1/dedupe",
        "GET /v1/watch",
        "POST /v1/watch",
        "POST /v1/watch/{id}/run",
        "GET /api-docs/openapi.json",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
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
#[path = "client_server_tests.rs"]
mod tests;
