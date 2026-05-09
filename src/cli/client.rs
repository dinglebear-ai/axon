use crate::core::http::build_client;
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
        let client = build_client(timeout_secs, None).map_err(|e| {
            ServerClientError::new(
                ServerClientErrorKind::BuildClient,
                format!("build server HTTP client: {e}"),
            )
        })?;
        Ok(Self { base_url, client })
    }

    pub async fn post_action<T, R>(&self, request: &T) -> Result<R, ServerClientError>
    where
        T: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        self.post_json("v1/actions", request, "server action response")
            .await
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

    fn endpoint(&self, path: &str) -> reqwest::Url {
        let mut endpoint = self.base_url.clone();
        let mut base_path = endpoint.path().trim_end_matches('/').to_string();
        if !base_path.is_empty() {
            base_path.push('/');
        }
        base_path.push_str(path.trim_start_matches('/'));
        endpoint.set_path(&base_path);
        endpoint
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
mod tests;
