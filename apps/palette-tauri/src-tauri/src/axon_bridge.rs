use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::{merged_settings, validate_saved_server_url};

const PALETTE_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// A shared `reqwest::Client` held in Tauri `AppState`.
///
/// Creating a new client per-request is wasteful: it allocates a new
/// connection pool and TLS context each time.  Storing one client in state
/// lets all bridge calls share a single connection pool.
pub(crate) struct BridgeClient(reqwest::Client);

impl BridgeClient {
    pub(crate) fn new() -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .connect_timeout(PALETTE_CONNECT_TIMEOUT)
            .user_agent(concat!("Axon Palette/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self(client))
    }

    pub(crate) fn client(&self) -> &reqwest::Client {
        &self.0
    }
}

/// A shared `reqwest::Client` for SSE streaming requests, held in Tauri `AppState`.
///
/// Unlike `BridgeClient`, this client has no total-request timeout so long
/// RAG streams are not cut off at an arbitrary wall-clock limit.  A connect
/// timeout is still applied to reject unreachable servers quickly.
/// No read-idle timeout is set; a server that stalls mid-stream will hold the
/// connection indefinitely — add a per-request timeout override at the call
/// site if needed for a specific endpoint.
pub(crate) struct StreamClient(reqwest::Client);

impl StreamClient {
    pub(crate) fn new() -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            .connect_timeout(PALETTE_CONNECT_TIMEOUT)
            .user_agent(concat!("Axon Palette/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self(client))
    }

    pub(crate) fn client(&self) -> &reqwest::Client {
        &self.0
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AxonHttpRequest {
    // Intentionally ignored — the real value is loaded from app settings
    #[serde(default, rename = "baseUrl")]
    _base_url: Option<String>,
    // Intentionally ignored — the real value is loaded from app settings
    #[serde(default, rename = "token")]
    _token: Option<String>,
    method: HttpMethod,
    path: String,
    body: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
enum HttpMethod {
    Get,
    Post,
    Delete,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AxonHttpResult {
    ok: bool,
    status: u16,
    path: String,
    method: HttpMethod,
    payload: serde_json::Value,
}

#[tauri::command]
pub(crate) async fn axon_http_request(
    app: AppHandle,
    bridge: tauri::State<'_, BridgeClient>,
    request: AxonHttpRequest,
) -> Result<AxonHttpResult, String> {
    let path = validate_axon_route(&request)?.to_string();
    let method = request.method;
    let settings = merged_settings(&app)?;
    let base_url = validate_saved_server_url(&settings.server_url)?;
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    let client = (*bridge).client();

    let mut builder = match method {
        HttpMethod::Get => client.get(&url),
        HttpMethod::Post => client.post(&url),
        HttpMethod::Delete => client.delete(&url),
    }
    .header(
        reqwest::header::ACCEPT,
        "application/json, text/plain;q=0.9, */*;q=0.5",
    );

    if let Some(token) = settings
        .token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        builder = builder.bearer_auth(token).header("x-api-key", token);
    }
    if let Some(body) = request.body {
        builder = builder.json(&body);
    }

    let response = builder.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    let text = response.text().await.map_err(|err| err.to_string())?;
    let payload = if text.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text))
    };

    Ok(AxonHttpResult {
        ok: status.is_success(),
        status: status.as_u16(),
        path,
        method,
        payload,
    })
}

fn validate_axon_route(request: &AxonHttpRequest) -> Result<&str, String> {
    let path = request.path.trim();
    if path != request.path || !path.starts_with("/v1/") {
        return Err("request path is not an allowed Axon API route".to_string());
    }
    if path.contains("://")
        || path.starts_with("//")
        || path.contains(['\\', '?', '#'])
        || path
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return Err("request path must be a canonical /v1 route path".to_string());
    }
    if matches!(request.method, HttpMethod::Get | HttpMethod::Delete) && request.body.is_some() {
        return Err("GET and DELETE requests cannot include a body".to_string());
    }
    is_allowed_route(request.method, path)
        .then_some(path)
        .ok_or_else(|| "request method and path are not allowed".to_string())
}

fn is_allowed_route(method: HttpMethod, path: &str) -> bool {
    matches!(
        (method, path),
        (
            HttpMethod::Get,
            "/v1/doctor" | "/v1/status" | "/v1/sources" | "/v1/domains" | "/v1/stats" | "/v1/watch"
        ) | (
            HttpMethod::Post,
            "/v1/scrape"
                | "/v1/crawl"
                | "/v1/map"
                | "/v1/summarize"
                | "/v1/ask"
                | "/v1/chat"
                | "/v1/query"
                | "/v1/retrieve"
                | "/v1/suggest"
                | "/v1/evaluate"
                | "/v1/search"
                | "/v1/research"
                | "/v1/embed"
                | "/v1/extract"
                | "/v1/ingest"
                | "/v1/endpoints"
                | "/v1/brand"
                | "/v1/diff"
                | "/v1/screenshot"
                | "/v1/dedupe"
                | "/v1/watch"
                | "/v1/ingest/sessions/prepared"
        ) | (
            HttpMethod::Post,
            "/v1/crawl/cleanup"
                | "/v1/crawl/recover"
                | "/v1/embed/cleanup"
                | "/v1/embed/recover"
                | "/v1/extract/cleanup"
                | "/v1/extract/recover"
                | "/v1/ingest/cleanup"
                | "/v1/ingest/recover"
        ) | (
            HttpMethod::Get | HttpMethod::Delete,
            "/v1/crawl" | "/v1/embed" | "/v1/extract" | "/v1/ingest"
        )
    ) || matches_dynamic_job_route(method, path)
        || matches_dynamic_watch_route(method, path)
}

fn matches_dynamic_job_route(method: HttpMethod, path: &str) -> bool {
    let parts: Vec<_> = path.trim_start_matches('/').split('/').collect();
    match parts.as_slice() {
        ["v1", family, id]
            if matches!(*family, "crawl" | "embed" | "extract" | "ingest")
                && method == HttpMethod::Get =>
        {
            is_uuid(id)
        }
        ["v1", family, id, "cancel"]
            if matches!(*family, "crawl" | "embed" | "extract" | "ingest")
                && method == HttpMethod::Post =>
        {
            is_uuid(id)
        }
        _ => false,
    }
}

fn matches_dynamic_watch_route(method: HttpMethod, path: &str) -> bool {
    let parts: Vec<_> = path.trim_start_matches('/').split('/').collect();
    matches!(parts.as_slice(), ["v1", "watch", id, "run"] if method == HttpMethod::Post && is_uuid(id))
}

fn is_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok()
}

#[cfg(test)]
#[path = "axon_bridge_tests.rs"]
mod tests;
