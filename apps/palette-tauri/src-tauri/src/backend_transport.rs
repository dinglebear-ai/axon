use crate::{BackendProduct, BackendProfile, merged_settings, validate_saved_server_url};
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Mutex, time::Duration};
use tauri::AppHandle;
use tokio::sync::oneshot;

const MAX_REQUEST_BODY_BYTES: usize = 1024 * 1024;
const MAX_RESPONSE_BODY_BYTES: u64 = 8 * 1024 * 1024;

pub(crate) struct BackendTransport {
    client: reqwest::Client,
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
    let url = format!("{origin}{}", request.path);
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
    }
    let response = tokio::select! { _ = cancel_rx => Err("backend request cancelled".into()), result = builder.send() => result.map_err(redact) };
    transport
        .cancellations
        .lock()
        .map_err(|_| "request registry unavailable")?
        .remove(&request.request_id);
    let response = response?;
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

fn validate_request(request: &BackendRequest) -> Result<(), String> {
    validate_request_id(&request.request_id)?;
    if request.path.contains(['?', '#', '\\'])
        || !request.path.starts_with("/v1/")
        || request.path.contains("..")
    {
        return Err("backend path must be a product-qualified /v1 route".into());
    }
    match request.product {
        BackendProduct::Axon => true,
        BackendProduct::Labby => request.path.starts_with("/v1/labby/"),
        BackendProduct::Cortex => request.path.starts_with("/v1/cortex/"),
    }
    .then_some(())
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
pub(crate) fn normalize_profiles(profiles: Vec<BackendProfile>) -> Vec<BackendProfile> {
    profiles
        .into_iter()
        .filter_map(|mut p| {
            p.id = p.id.trim().into();
            p.label = p.label.trim().into();
            p.origin = validate_saved_server_url(&p.origin).ok()?;
            ((!p.id.is_empty())
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
