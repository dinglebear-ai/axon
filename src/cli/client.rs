use crate::core::http::build_client_without_ssrf_resolver;
use futures_util::StreamExt;
use reqwest::StatusCode;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::error::Error;
use std::fmt;
use std::net::IpAddr;

pub const SERVER_ACTION_TIMEOUT_SECS: u64 = 300;
pub const SERVER_POLL_TIMEOUT_SECS: u64 = 30;

const TOKEN_ENV: &str = "AXON_MCP_HTTP_TOKEN";
const INSECURE_ENV: &str = "AXON_SERVER_INSECURE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerClientErrorKind {
    CleartextBearer,
    Connect,
    Status,
    Auth,
    VersionMismatch,
    Decode,
    BuildClient,
}

#[derive(Debug)]
pub struct ServerClientError {
    kind: ServerClientErrorKind,
    message: String,
}

impl ServerClientError {
    pub fn new(kind: ServerClientErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> ServerClientErrorKind {
        self.kind
    }
}

impl fmt::Display for ServerClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for ServerClientError {}

pub struct ServerClient {
    base_url: reqwest::Url,
    client: reqwest::Client,
}

impl ServerClient {
    pub fn new(base_url: reqwest::Url) -> Result<Self, ServerClientError> {
        Self::with_timeout(base_url, SERVER_ACTION_TIMEOUT_SECS)
    }

    pub fn with_timeout(
        base_url: reqwest::Url,
        timeout_secs: u64,
    ) -> Result<Self, ServerClientError> {
        let client = build_client_without_ssrf_resolver(timeout_secs, None).map_err(|e| {
            ServerClientError::new(
                ServerClientErrorKind::BuildClient,
                format!("build server HTTP client: {e}"),
            )
        })?;
        Ok(Self { base_url, client })
    }

    pub async fn get_json<R>(
        &self,
        path: &str,
        response_label: &'static str,
    ) -> Result<R, ServerClientError>
    where
        R: DeserializeOwned,
    {
        let endpoint = self.endpoint(path);
        let mut req = self.client.get(endpoint.clone());
        if let Some(token) = bearer_token() {
            check_cleartext_token_allowed(&self.base_url)?;
            req = req.bearer_auth(token);
        }

        let resp = req.send().await.map_err(|e| {
            ServerClientError::new(
                ServerClientErrorKind::Connect,
                format!("connect to {endpoint}: {e}"),
            )
        })?;
        decode_response(resp, &endpoint, response_label).await
    }

    pub async fn post_json<T, R>(
        &self,
        path: &str,
        request: &T,
        response_label: &'static str,
    ) -> Result<R, ServerClientError>
    where
        T: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        let endpoint = self.endpoint(path);
        let mut req = self.client.post(endpoint.clone()).json(request);
        if let Some(token) = bearer_token() {
            check_cleartext_token_allowed(&self.base_url)?;
            req = req.bearer_auth(token);
        }

        let resp = req.send().await.map_err(|e| {
            ServerClientError::new(
                ServerClientErrorKind::Connect,
                format!("connect to {endpoint}: {e}"),
            )
        })?;
        decode_response(resp, &endpoint, response_label).await
    }

    pub async fn post_json_sse<T, F>(
        &self,
        path: &str,
        request: &T,
        response_label: &'static str,
        mut on_delta: F,
    ) -> Result<serde_json::Value, ServerClientError>
    where
        T: Serialize + ?Sized,
        F: FnMut(&str),
    {
        let endpoint = self.endpoint(path);
        let mut req = self
            .client
            .post(endpoint.clone())
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .json(request);
        if let Some(token) = bearer_token() {
            check_cleartext_token_allowed(&self.base_url)?;
            req = req.bearer_auth(token);
        }

        let resp = req.send().await.map_err(|e| {
            ServerClientError::new(
                ServerClientErrorKind::Connect,
                format!("connect to {endpoint}: {e}"),
            )
        })?;
        decode_sse_response(resp, &endpoint, response_label, &mut on_delta).await
    }

    pub async fn delete_json<R>(
        &self,
        path: &str,
        response_label: &'static str,
    ) -> Result<R, ServerClientError>
    where
        R: DeserializeOwned,
    {
        let endpoint = self.endpoint(path);
        let mut req = self.client.delete(endpoint.clone());
        if let Some(token) = bearer_token() {
            check_cleartext_token_allowed(&self.base_url)?;
            req = req.bearer_auth(token);
        }

        let resp = req.send().await.map_err(|e| {
            ServerClientError::new(
                ServerClientErrorKind::Connect,
                format!("connect to {endpoint}: {e}"),
            )
        })?;
        decode_response(resp, &endpoint, response_label).await
    }

    fn endpoint(&self, path: &str) -> reqwest::Url {
        let mut endpoint = self.base_url.clone();
        let mut base_path = endpoint.path().trim_end_matches('/').to_string();
        if !base_path.is_empty() {
            base_path.push('/');
        }
        let path = path.trim_start_matches('/');
        let (path, query) = path
            .split_once('?')
            .map_or((path, None), |(path, query)| (path, Some(query)));
        base_path.push_str(path);
        endpoint.set_path(&base_path);
        endpoint.set_query(query.filter(|query| !query.is_empty()));
        endpoint
    }
}

async fn decode_sse_response<F>(
    resp: reqwest::Response,
    endpoint: &reqwest::Url,
    response_label: &'static str,
    on_delta: &mut F,
) -> Result<serde_json::Value, ServerClientError>
where
    F: FnMut(&str),
{
    let status = resp.status();
    if !status.is_success() {
        let body = resp
            .text()
            .await
            .unwrap_or_else(|e| format!("<body read failed: {e}>"));
        let kind = classify_status(status, &body);
        return Err(ServerClientError::new(
            kind,
            format!("server returned {status}: {body}"),
        ));
    }

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    let mut final_result: Option<serde_json::Value> = None;
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| {
            ServerClientError::new(
                ServerClientErrorKind::Decode,
                format!("decode {response_label} stream from {endpoint}: {e}"),
            )
        })?;
        let text = std::str::from_utf8(&bytes).map_err(|e| {
            ServerClientError::new(
                ServerClientErrorKind::Decode,
                format!("decode {response_label} utf8 stream from {endpoint}: {e}"),
            )
        })?;
        buffer.push_str(text);
        while let Some((frame, rest)) = buffer.split_once("\n\n") {
            let frame = frame.to_string();
            buffer = rest.to_string();
            if let Some(result) = handle_sse_frame(&frame, response_label, endpoint, on_delta)? {
                final_result = Some(result);
            }
        }
    }
    if !buffer.trim().is_empty()
        && let Some(result) = handle_sse_frame(&buffer, response_label, endpoint, on_delta)?
    {
        final_result = Some(result);
    }
    final_result.ok_or_else(|| {
        ServerClientError::new(
            ServerClientErrorKind::Decode,
            format!("decode {response_label} stream from {endpoint}: missing done event"),
        )
    })
}

fn handle_sse_frame<F>(
    frame: &str,
    response_label: &'static str,
    endpoint: &reqwest::Url,
    on_delta: &mut F,
) -> Result<Option<serde_json::Value>, ServerClientError>
where
    F: FnMut(&str),
{
    let data = frame
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    if data.trim().is_empty() {
        return Ok(None);
    }
    let value: serde_json::Value = serde_json::from_str(&data).map_err(|e| {
        ServerClientError::new(
            ServerClientErrorKind::Decode,
            format!("decode {response_label} SSE JSON from {endpoint}: {e}"),
        )
    })?;
    match value.get("type").and_then(serde_json::Value::as_str) {
        Some("delta") => {
            if let Some(text) = value.get("text").and_then(serde_json::Value::as_str) {
                on_delta(text);
            }
            Ok(None)
        }
        Some("done") => Ok(Some(
            value
                .get("result")
                .cloned()
                .or_else(|| value.get("payload").cloned())
                .or_else(|| {
                    value.get("answer").map(|answer| {
                        serde_json::json!({
                            "answer": answer,
                            "query": "",
                            "timing_ms": {
                                "retrieval": 0,
                                "context_build": 0,
                                "llm": 0,
                                "total": 0
                            }
                        })
                    })
                })
                .ok_or_else(|| {
                    ServerClientError::new(
                        ServerClientErrorKind::Decode,
                        format!(
                            "decode {response_label} done event from {endpoint}: missing result"
                        ),
                    )
                })?,
        )),
        Some("error") => Err(ServerClientError::new(
            ServerClientErrorKind::Status,
            value
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("stream returned error")
                .to_string(),
        )),
        _ => Ok(None),
    }
}

async fn decode_response<R: DeserializeOwned>(
    resp: reqwest::Response,
    endpoint: &reqwest::Url,
    response_label: &'static str,
) -> Result<R, ServerClientError> {
    let status = resp.status();
    if !status.is_success() {
        let body = resp
            .text()
            .await
            .unwrap_or_else(|e| format!("<body read failed: {e}>"));
        let kind = classify_status(status, &body);
        return Err(ServerClientError::new(
            kind,
            format!("server returned {status}: {body}"),
        ));
    }

    resp.json().await.map_err(|e| {
        ServerClientError::new(
            ServerClientErrorKind::Decode,
            format!("decode {response_label} from {endpoint}: {e}"),
        )
    })
}

fn classify_status(status: StatusCode, body: &str) -> ServerClientErrorKind {
    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        return ServerClientErrorKind::Auth;
    }
    let body_lower = body.to_ascii_lowercase();
    if status == StatusCode::UPGRADE_REQUIRED
        || body_lower.contains("schema")
        || body_lower.contains("version mismatch")
    {
        return ServerClientErrorKind::VersionMismatch;
    }
    ServerClientErrorKind::Status
}

fn bearer_token() -> Option<String> {
    std::env::var(TOKEN_ENV)
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

/// Returns true when `host_str` represents a loopback destination
/// (127.0.0.0/8, ::1, or the literal "localhost").
pub(crate) fn is_loopback_host(host_str: &str) -> bool {
    if host_str.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let trimmed = host_str
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host_str);
    if let Ok(ip) = trimmed.parse::<IpAddr>() {
        return ip.is_loopback();
    }
    false
}

/// Refuse cleartext bearer tokens over `http://` to non-loopback hosts unless
/// explicitly allowed.
pub fn check_cleartext_token_allowed(url: &reqwest::Url) -> Result<(), ServerClientError> {
    if url.scheme() != "http" {
        return Ok(());
    }
    let host = url.host_str().unwrap_or("");
    if is_loopback_host(host) {
        return Ok(());
    }
    if std::env::var(INSECURE_ENV).ok().as_deref() == Some("1") {
        return Ok(());
    }
    Err(ServerClientError::new(
        ServerClientErrorKind::CleartextBearer,
        format!(
            "refusing to send AXON_MCP_HTTP_TOKEN over plaintext HTTP to non-loopback host '{host}'; set AXON_SERVER_INSECURE=1 to override (not recommended)"
        ),
    ))
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
