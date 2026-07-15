use base64::Engine as _;
use futures_util::{Stream, StreamExt as _};
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::{merged_settings, validate_saved_server_url};

const PALETTE_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const MAX_ARTIFACT_PREVIEW_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_ARTIFACT_ERROR_MESSAGE_BYTES: u64 = 2048;
const ARTIFACT_TOO_LARGE: &str = "artifact is too large to preview";
const RASTER_ARTIFACT_CONTENT_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
    "image/avif",
];

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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AxonArtifactResult {
    ok: bool,
    status: u16,
    content_type: String,
    message: Option<String>,
    body_base64: String,
}

#[tauri::command]
pub(crate) async fn axon_http_request(
    app: AppHandle,
    bridge: tauri::State<'_, BridgeClient>,
    oauth_state: tauri::State<'_, crate::oauth::OauthState>,
    request: AxonHttpRequest,
) -> Result<AxonHttpResult, String> {
    let path = validate_axon_route(&request)?.to_string();
    let method = request.method;
    let settings = merged_settings(&app)?;
    let base_url = validate_saved_server_url(&settings.server_url)?;
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    let client = (*bridge).client();

    let static_token = settings
        .token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let body = request.body;
    let make = |token: Option<&str>| {
        let mut b = match method {
            HttpMethod::Get => client.get(&url),
            HttpMethod::Post => client.post(&url),
            HttpMethod::Delete => client.delete(&url),
        }
        .header(
            reqwest::header::ACCEPT,
            "application/json, text/plain;q=0.9, */*;q=0.5",
        );
        if let Some(t) = token {
            b = b.bearer_auth(t).header("x-api-key", t);
        }
        if let Some(body) = &body {
            b = b.json(body);
        }
        b
    };
    let response =
        crate::oauth::send_with_reauth(&app, client, &base_url, static_token, &oauth_state, make)
            .await?;
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

#[tauri::command]
pub(crate) async fn axon_artifact_request(
    app: AppHandle,
    bridge: tauri::State<'_, BridgeClient>,
    oauth_state: tauri::State<'_, crate::oauth::OauthState>,
    relative_path: String,
) -> Result<AxonArtifactResult, String> {
    let settings = merged_settings(&app)?;
    let base_url = validate_saved_server_url(&settings.server_url)?;
    let url = artifact_url(&base_url, &relative_path)?;
    let client = (*bridge).client();

    let static_token = settings
        .token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let make = |token: Option<&str>| {
        let mut b = client.get(url.clone()).header(
            reqwest::header::ACCEPT,
            "image/png, image/jpeg, image/webp, image/gif, image/avif",
        );
        if let Some(t) = token {
            b = b.bearer_auth(t).header("x-api-key", t);
        }
        b
    };
    let response =
        crate::oauth::send_with_reauth(&app, client, &base_url, static_token, &oauth_state, make)
            .await?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    if !status.is_success() {
        let message = read_limited_text_body(response, MAX_ARTIFACT_ERROR_MESSAGE_BYTES)
            .await
            .unwrap_or_default();
        return Ok(AxonArtifactResult {
            ok: false,
            status: status.as_u16(),
            content_type,
            message: Some(message).filter(|value| !value.trim().is_empty()),
            body_base64: String::new(),
        });
    }
    if !is_allowed_artifact_content_type(&content_type) {
        return Err("artifact content type is not previewable".to_string());
    }
    if response.content_length().unwrap_or(0) > MAX_ARTIFACT_PREVIEW_BYTES {
        return Err(ARTIFACT_TOO_LARGE.to_string());
    }
    let bytes = read_limited_artifact_body(response).await?;

    Ok(AxonArtifactResult {
        ok: true,
        status: status.as_u16(),
        content_type,
        message: None,
        body_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
    })
}

async fn read_limited_artifact_body(response: reqwest::Response) -> Result<Vec<u8>, String> {
    read_limited_artifact_stream(response.bytes_stream()).await
}

pub(crate) async fn read_limited_text_body(
    response: reqwest::Response,
    max_bytes: u64,
) -> Result<String, String> {
    let bytes = read_limited_stream(response.bytes_stream(), max_bytes).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

async fn read_limited_artifact_stream<S, B, E>(stream: S) -> Result<Vec<u8>, String>
where
    S: Stream<Item = Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
    E: std::fmt::Display,
{
    read_limited_stream(stream, MAX_ARTIFACT_PREVIEW_BYTES).await
}

async fn read_limited_stream<S, B, E>(mut stream: S, max_bytes: u64) -> Result<Vec<u8>, String>
where
    S: Stream<Item = Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
    E: std::fmt::Display,
{
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| err.to_string())?;
        let chunk = chunk.as_ref();
        if (body.len() + chunk.len()) as u64 > max_bytes {
            return Err(ARTIFACT_TOO_LARGE.to_string());
        }
        body.extend_from_slice(chunk);
    }
    Ok(body)
}

fn validate_artifact_relative_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\0')
        || path.contains('\\')
        || path.contains(':')
    {
        return Err("artifact path must be a safe relative path".to_string());
    }
    let decoded = percent_decode_str(path).decode_utf8_lossy();
    if decoded.contains(':')
        || decoded.contains('\\')
        || decoded
            .split('/')
            .any(|segment| segment == "." || segment == "..")
        || decoded.split('/').any(str::is_empty)
        || std::path::Path::new(decoded.as_ref())
            .components()
            .any(|part| {
                matches!(
                    part,
                    std::path::Component::CurDir
                        | std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
    {
        return Err("artifact path must be a safe relative path".to_string());
    }
    Ok(())
}

fn artifact_url(base_url: &str, relative_path: &str) -> Result<url::Url, String> {
    validate_artifact_relative_path(relative_path)?;
    let mut url = url::Url::parse(base_url).map_err(|err| err.to_string())?;
    url.set_path("/v1/artifacts");
    url.query_pairs_mut().append_pair("path", relative_path);
    Ok(url)
}

fn is_allowed_artifact_content_type(value: &str) -> bool {
    let media_type = value.split(';').next().unwrap_or("").trim();
    RASTER_ARTIFACT_CONTENT_TYPES.contains(&media_type)
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
            // scrape/crawl/embed/ingest submit through the unified source
            // pipeline now (see actionRequest.ts) — the legacy verb routes
            // were removed server-side (confirmed 404 by
            // crates/axon-web/src/server/handlers/rest_tests.rs) and are
            // deliberately absent from this allowlist.
            "/v1/sources"
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
                | "/v1/extract"
                | "/v1/endpoints"
                | "/v1/brand"
                | "/v1/diff"
                | "/v1/screenshot"
                | "/v1/dedupe"
                | "/v1/purge"
                | "/v1/watch"
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
