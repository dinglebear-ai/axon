use crate::{BackendProduct, BackendProfile, merged_settings, validate_saved_server_url};
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Mutex, time::Duration};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

const MAX_REQUEST_BODY_BYTES: usize = 1024 * 1024;
const MAX_RESPONSE_BODY_BYTES: u64 = 8 * 1024 * 1024;

pub(crate) struct BackendTransport {
    client: reqwest::Client,
    stream_client: reqwest::Client,
    cancellations: Mutex<HashMap<String, oneshot::Sender<()>>>,
}

impl BackendTransport {
    pub(crate) fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(60))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("Axon Palette/", env!("CARGO_PKG_VERSION")))
                .build()?,
            stream_client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("Axon Palette/", env!("CARGO_PKG_VERSION")))
                .build()?,
            cancellations: Mutex::new(HashMap::new()),
        })
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
enum BackendMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackendRequest {
    profile_id: String,
    product: BackendProduct,
    request_id: String,
    method: BackendMethod,
    path: String,
    body: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackendStreamRequest {
    profile_id: String,
    product: BackendProduct,
    request_id: String,
    generation: u64,
    path: String,
    #[serde(default)]
    params: HashMap<String, String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct BackendStreamEvent {
    request_id: String,
    generation: u64,
    event: String,
    id: Option<String>,
    data: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackendResult {
    ok: bool,
    status: u16,
    profile_id: String,
    product: BackendProduct,
    request_id: String,
    payload: serde_json::Value,
}

#[tauri::command]
pub(crate) async fn backend_http_request(
    app: AppHandle,
    transport: tauri::State<'_, BackendTransport>,
    request: BackendRequest,
) -> Result<BackendResult, String> {
    validate_request(&request)?;
    let settings = merged_settings(&app)?;
    let profile = resolve_profile(&settings.backend_profiles, &request)?;
    let origin = validate_profile_origin(profile)?;
    let body = request
        .body
        .as_ref()
        .map(serde_json::to_vec)
        .transpose()
        .map_err(redact)?;
    if body
        .as_ref()
        .is_some_and(|value| value.len() > MAX_REQUEST_BODY_BYTES)
    {
        return Err("backend request body exceeds the 1 MiB limit".into());
    }
    let upstream_path = upstream_path(request.product, &request.path)?;
    let url = format!("{origin}{upstream_path}");
    let mut builder = match request.method {
        BackendMethod::Get => transport.client.get(url),
        BackendMethod::Post => transport.client.post(url),
        BackendMethod::Put => transport.client.put(url),
        BackendMethod::Patch => transport.client.patch(url),
        BackendMethod::Delete => transport.client.delete(url),
    }
    .header(reqwest::header::ACCEPT, "application/json");
    if let Some(body) = body {
        builder = builder
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body);
    }
    if profile.credential_handle.as_deref() == Some("legacy-axon")
        && profile.product == BackendProduct::Axon
        && let Some(token) = settings.token.as_deref().filter(|token| !token.is_empty())
    {
        builder = builder.bearer_auth(token).header("x-api-key", token);
    } else if let Some(handle) = profile.credential_handle.as_deref() {
        let credential = crate::backend_credentials::load(&app, handle)?
            .ok_or("backend credential is unavailable")?;
        validate_credential_binding(profile, &origin, &credential)?;
        builder = builder
            .bearer_auth(&credential.token)
            .header("x-api-key", &credential.token)
            .header("x-palette-credential-generation", &credential.generation);
    }
    let (cancel_tx, cancel_rx) = oneshot::channel();
    {
        let mut map = transport
            .cancellations
            .lock()
            .map_err(|_| "request registry unavailable")?;
        if map.insert(request.request_id.clone(), cancel_tx).is_some() {
            return Err("request ID is already active".into());
        }
    }
    let response = tokio::select! { _ = cancel_rx => Err("backend request cancelled".into()), result = builder.send() => result.map_err(redact) };
    transport
        .cancellations
        .lock()
        .map_err(|_| "request registry unavailable")?
        .remove(&request.request_id);
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            transport
                .cancellations
                .lock()
                .map_err(|_| "request registry unavailable")?
                .remove(&request.request_id);
            return Err(error);
        }
    };
    if response.status().is_redirection() {
        return Err("backend redirects are forbidden".into());
    }
    let status = response.status();
    let bytes = read_bounded(response.bytes_stream()).await?;
    let payload = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            serde_json::Value::String(String::from_utf8_lossy(&bytes).into_owned())
        })
    };
    Ok(BackendResult {
        ok: status.is_success(),
        status: status.as_u16(),
        profile_id: request.profile_id,
        product: request.product,
        request_id: request.request_id,
        payload,
    })
}

async fn read_bounded(
    mut stream: impl futures_util::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(redact)?;
        if body.len() + chunk.len() > MAX_RESPONSE_BODY_BYTES as usize {
            return Err("backend response exceeds the 8 MiB limit".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[tauri::command]
pub(crate) fn backend_cancel_request(
    transport: tauri::State<'_, BackendTransport>,
    request_id: String,
) -> Result<bool, String> {
    validate_request_id(&request_id)?;
    Ok(transport
        .cancellations
        .lock()
        .map_err(|_| "request registry unavailable")?
        .remove(&request_id)
        .is_some_and(|sender| sender.send(()).is_ok()))
}

#[tauri::command]
pub(crate) async fn backend_http_stream(
    app: AppHandle,
    window: tauri::Window,
    transport: tauri::State<'_, BackendTransport>,
    request: BackendStreamRequest,
) -> Result<(), String> {
    let ordinary = BackendRequest {
        profile_id: request.profile_id.clone(),
        product: request.product,
        request_id: request.request_id.clone(),
        method: BackendMethod::Get,
        path: request.path.clone(),
        body: None,
    };
    validate_request(&ordinary)?;
    if request.product != BackendProduct::Cortex
        || !matches!(
            request.path.as_str(),
            "/api/streams/logs" | "/api/streams/sessions"
        )
    {
        return Err("backend stream route is not allowed".into());
    }
    if request.params.len() > 8
        || request
            .params
            .iter()
            .any(|(key, value)| key.len() > 64 || value.len() > 4096)
    {
        return Err("backend stream parameters exceed limits".into());
    }
    let settings = merged_settings(&app)?;
    let profile = resolve_profile(&settings.backend_profiles, &ordinary)?;
    let origin = validate_profile_origin(profile)?;
    let upstream_path = upstream_path(request.product, &request.path)?;
    let mut url = reqwest::Url::parse(&format!("{origin}{upstream_path}")).map_err(redact)?;
    url.query_pairs_mut().extend_pairs(request.params.iter());
    let (cancel_tx, mut cancel_rx) = oneshot::channel();
    let mut builder = transport
        .stream_client
        .get(url)
        .header(reqwest::header::ACCEPT, "text/event-stream");
    if let Some(handle) = profile.credential_handle.as_deref() {
        let credential = crate::backend_credentials::load(&app, handle)?
            .ok_or("backend credential is unavailable")?;
        validate_credential_binding(profile, &origin, &credential)?;
        builder = builder
            .bearer_auth(&credential.token)
            .header("x-api-key", &credential.token)
            .header("x-palette-credential-generation", &credential.generation);
    }
    if transport
        .cancellations
        .lock()
        .map_err(|_| "request registry unavailable")?
        .insert(request.request_id.clone(), cancel_tx)
        .is_some()
    {
        return Err("request ID is already active".into());
    }
    let response = tokio::select! {
        _ = &mut cancel_rx => Err("backend request cancelled".into()),
        result = builder.send() => result.map_err(redact),
    };
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            transport
                .cancellations
                .lock()
                .map_err(|_| "request registry unavailable")?
                .remove(&request.request_id);
            return Err(error);
        }
    };
    if !response.status().is_success() {
        transport
            .cancellations
            .lock()
            .map_err(|_| "request registry unavailable")?
            .remove(&request.request_id);
        return Err(format!(
            "backend stream failed with HTTP {}",
            response.status()
        ));
    }
    let mut bytes = response.bytes_stream();
    let mut pending = Vec::new();
    let result = async {
        loop {
            let chunk = tokio::select! {
                _ = &mut cancel_rx => return Err("backend request cancelled".into()),
                chunk = bytes.next() => chunk,
            };
            let Some(chunk) = chunk else { break };
            let chunk = chunk.map_err(redact)?;
            pending.extend_from_slice(&chunk);
            if pending.len() > 256 * 1024 {
                return Err("backend SSE frame exceeds 256 KiB".into());
            }
            while let Some(end) = pending.windows(2).position(|pair| pair == b"\n\n") {
                let frame: Vec<u8> = pending.drain(..end + 2).collect();
                emit_sse_frame(&window, &request, &frame)?;
            }
        }
        Ok(())
    }
    .await;
    transport
        .cancellations
        .lock()
        .map_err(|_| "request registry unavailable")?
        .remove(&request.request_id);
    result
}

fn emit_sse_frame(
    window: &tauri::Window,
    request: &BackendStreamRequest,
    frame: &[u8],
) -> Result<(), String> {
    let text = std::str::from_utf8(frame).map_err(|_| "invalid UTF-8 in backend SSE stream")?;
    let mut event = "message";
    let mut id = None;
    let mut data = String::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event = value.trim();
        } else if let Some(value) = line.strip_prefix("id:") {
            id = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push_str(value.trim());
        }
    }
    if data.is_empty() {
        return Ok(());
    }
    let data = serde_json::from_str(&data).map_err(|_| "invalid JSON in backend SSE event")?;
    window
        .emit(
            "palette://backend-stream",
            BackendStreamEvent {
                request_id: request.request_id.clone(),
                generation: request.generation,
                event: event.to_string(),
                id,
                data,
            },
        )
        .map_err(redact)
}

fn validate_request(request: &BackendRequest) -> Result<(), String> {
    validate_request_id(&request.request_id)?;
    if request.path.contains(['#', '\\'])
        || !(request.path.starts_with("/v1/") || request.path.starts_with("/api/"))
        || request.path.contains("..")
    {
        return Err("backend path must be an allowed product route".into());
    }
    upstream_path(request.product, &request.path).map(|_| ())
}

fn upstream_path(product: BackendProduct, path_and_query: &str) -> Result<&str, String> {
    let path = path_and_query.split('?').next().unwrap_or(path_and_query);
    let allowed = match product {
        BackendProduct::Axon => path.starts_with("/v1/"),
        BackendProduct::Labby => {
            path == "/v1/integration/identity"
                || path == "/v1/gateway"
                || path.starts_with("/v1/palette/")
        }
        BackendProduct::Cortex => path == "/v1/integration/identity" || path.starts_with("/api/"),
    };
    allowed
        .then_some(path_and_query)
        .ok_or_else(|| "backend route does not belong to the selected product".into())
}
fn validate_request_id(value: &str) -> Result<(), String> {
    ((16..=128).contains(&value.len())
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')))
    .then_some(())
    .ok_or_else(|| "request ID must be an unguessable 16-128 character identifier".into())
}
fn resolve_profile<'a>(
    profiles: &'a [BackendProfile],
    request: &BackendRequest,
) -> Result<&'a BackendProfile, String> {
    profiles
        .iter()
        .find(|p| p.id == request.profile_id && p.product == request.product)
        .ok_or_else(|| "backend profile/product mismatch".into())
}
fn validate_profile_origin(profile: &BackendProfile) -> Result<String, String> {
    if profile.accepted_api_major != 1 {
        return Err("backend API major is not supported".into());
    }
    validate_saved_server_url(&profile.origin)
}
fn validate_credential_binding(
    profile: &BackendProfile,
    origin: &str,
    credential: &crate::backend_credentials::StoredBackendCredential,
) -> Result<(), String> {
    (credential.profile_id == profile.id
        && credential.product == profile.product
        && credential.origin == origin
        && profile.pinned_server_id.as_deref() == Some(credential.server_id.as_str())
        && profile.credential_generation.as_deref() == Some(credential.generation.as_str()))
    .then_some(())
    .ok_or_else(|| "backend credential trust binding does not match this profile".into())
}
pub(crate) fn normalize_profiles(profiles: Vec<BackendProfile>) -> Vec<BackendProfile> {
    let mut ids = std::collections::HashSet::new();
    profiles
        .into_iter()
        .filter_map(|mut p| {
            p.id = p.id.trim().into();
            p.label = p.label.trim().into();
            p.origin = validate_saved_server_url(&p.origin).ok()?;
            ((!p.id.is_empty())
                && ids.insert(p.id.clone())
                && p.id.len() <= 128
                && p.id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
                && p.accepted_api_major == 1)
                .then_some(p)
        })
        .collect()
}
fn redact(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    if message.contains("Bearer ") || message.contains("x-api-key") {
        "backend transport failed".into()
    } else {
        message
    }
}

#[cfg(test)]
#[path = "backend_transport_tests.rs"]
mod tests;
