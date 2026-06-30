//! OAuth wire protocol: RFC 8414 discovery, RFC 7591 dynamic client
//! registration, and the PKCE authorization-code + refresh token exchanges.
//! Pure builders/validators are unit-tested; the async wrappers are thin
//! reqwest calls. Token-bearing error strings never echo response bodies.

use serde::Deserialize;
use std::time::Duration;

use crate::oauth::secret::Secret;

/// Subset of the RFC 8414 authorization-server metadata the client needs.
/// Extra fields in the document are ignored. `registration_endpoint` is
/// optional — a DCR-disabled server omits it.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct AuthServerMetadata {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub registration_endpoint: Option<String>,
    #[serde(default)]
    pub native_callback_endpoint: Option<String>,
    #[serde(default)]
    pub native_poll_endpoint: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct NativePollResponse {
    code: Option<String>,
}

/// The `/token` success response (lab-auth omits `refresh_token` when the
/// upstream IdP did not return one). Token fields use `Secret`, which redacts
/// itself in `Debug`, so the derived `Debug` is safe.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct TokenResponse {
    pub access_token: Secret,
    // Part of the `/token` wire response but unused by the client; kept so the
    // field deserializes. The derived `Debug` no longer reads it (the previous
    // hand-written impl did), so silence dead-code here.
    #[allow(dead_code)]
    pub token_type: String,
    pub expires_in: u64,
    #[serde(default)]
    pub refresh_token: Option<Secret>,
    pub scope: String,
}

#[derive(Deserialize)]
struct ClientRegistrationResponse {
    client_id: String,
}

/// Why a `/token` request failed. `rejected` means a definitive grant rejection
/// (400/401/403/410) — the refresh token is dead and the session should be
/// cleared. Otherwise (network / 5xx / 429 rate-limit / bad body) it is
/// transient: keep the session and retry later.
#[derive(Debug)]
pub(crate) struct TokenError {
    pub rejected: bool,
    pub message: String,
}

/// Whether a `/token` HTTP status is a definitive grant rejection (clear the
/// session) rather than a transient failure. A 429 rate-limit or 408 is NOT a
/// rejection — wiping a valid session on a transient burst would force a
/// needless re-login.
fn is_grant_rejection(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::BAD_REQUEST
            | reqwest::StatusCode::UNAUTHORIZED
            | reqwest::StatusCode::FORBIDDEN
            | reqwest::StatusCode::GONE
    )
}

pub(crate) fn discovery_url(base_url: &str) -> String {
    format!(
        "{}/.well-known/oauth-authorization-server",
        base_url.trim_end_matches('/')
    )
}

/// Reject any URL that is not `https`, or `http` on a loopback host. OAuth
/// secrets (auth code, PKCE verifier, refresh token) must never traverse
/// cleartext to a non-loopback host.
pub(crate) fn require_secure_url(raw: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(raw).map_err(|err| format!("invalid OAuth URL `{raw}`: {err}"))?;
    match url.scheme() {
        "https" => Ok(url),
        // `host_str()` returns the bracketed form `[::1]` for IPv6 literals.
        "http"
            if matches!(
                url.host_str(),
                Some("127.0.0.1" | "localhost" | "::1" | "[::1]")
            ) =>
        {
            Ok(url)
        }
        _ => Err(format!(
            "refusing OAuth over an insecure URL `{raw}` — https is required for non-loopback hosts"
        )),
    }
}

pub(crate) fn build_authorize_url(
    meta: &AuthServerMetadata,
    client_id: &str,
    redirect_uri: &str,
    scope: &str,
    state: &str,
    code_challenge: &str,
) -> Result<String, String> {
    let mut url = url::Url::parse(&meta.authorization_endpoint)
        .map_err(|err| format!("invalid authorization_endpoint: {err}"))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", scope)
        .append_pair("state", state)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.to_string())
}

pub(crate) fn registration_body(redirect_uri: &str) -> serde_json::Value {
    serde_json::json!({ "redirect_uris": [redirect_uri] })
}

pub(crate) fn authorization_code_form(
    code: &str,
    client_id: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("client_id", client_id.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("code_verifier", code_verifier.to_string()),
    ]
}

pub(crate) fn refresh_form(client_id: &str, refresh_token: &str) -> Vec<(&'static str, String)> {
    vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.to_string()),
        ("client_id", client_id.to_string()),
    ]
}

pub(crate) async fn discover(
    client: &reqwest::Client,
    base_url: &str,
) -> Result<AuthServerMetadata, String> {
    let url = discovery_url(base_url);
    let response = client
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|err| format!("OAuth discovery request failed: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "OAuth discovery returned HTTP {} — is the server in OAuth mode (AXON_MCP_AUTH_MODE=oauth)?",
            response.status()
        ));
    }
    response
        .json()
        .await
        .map_err(|err| format!("OAuth discovery returned an invalid document: {err}"))
}

pub(crate) async fn register_client(
    client: &reqwest::Client,
    registration_endpoint: &str,
    redirect_uri: &str,
) -> Result<String, String> {
    let response = client
        .post(registration_endpoint)
        .json(&registration_body(redirect_uri))
        .send()
        .await
        .map_err(|err| format!("client registration request failed: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "client registration returned HTTP {}",
            response.status()
        ));
    }
    let registered: ClientRegistrationResponse = response
        .json()
        .await
        .map_err(|err| format!("client registration returned an invalid response: {err}"))?;
    Ok(registered.client_id)
}

pub(crate) async fn exchange_code(
    client: &reqwest::Client,
    token_endpoint: &str,
    code: &str,
    client_id: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<TokenResponse, String> {
    post_token_form(
        client,
        token_endpoint,
        &authorization_code_form(code, client_id, redirect_uri, code_verifier),
    )
    .await
    .map_err(|e| e.message)
}

pub(crate) async fn poll_native_code(
    client: &reqwest::Client,
    poll_endpoint: &str,
    state: &str,
    timeout: Duration,
) -> Result<String, String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Err("timed out waiting for the OAuth redirect".to_string());
        }
        let remaining = deadline.saturating_duration_since(now);
        let response = tokio::time::timeout(
            remaining,
            client
                .get(poll_endpoint)
                .query(&[("state", state)])
                .header(reqwest::header::ACCEPT, "application/json")
                .send(),
        )
        .await
        .map_err(|_| "timed out waiting for the OAuth redirect".to_string())?
        .map_err(|err| format!("native OAuth poll request failed: {err}"))?;
        if response.status() == reqwest::StatusCode::ACCEPTED {
            tokio::time::sleep(Duration::from_millis(500).min(remaining)).await;
            continue;
        }
        if !response.status().is_success() {
            return Err(format!(
                "native OAuth poll returned HTTP {}",
                response.status()
            ));
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let body: NativePollResponse = tokio::time::timeout(remaining, response.json())
            .await
            .map_err(|_| "timed out waiting for the OAuth redirect".to_string())?
            .map_err(|err| format!("native OAuth poll returned an invalid response: {err}"))?;
        if let Some(code) = body.code {
            return Ok(code);
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        tokio::time::sleep(Duration::from_millis(500).min(remaining)).await;
    }
}

pub(crate) async fn refresh_access_token(
    client: &reqwest::Client,
    token_endpoint: &str,
    client_id: &str,
    refresh_token: &str,
) -> Result<TokenResponse, TokenError> {
    post_token_form(
        client,
        token_endpoint,
        &refresh_form(client_id, refresh_token),
    )
    .await
}

async fn post_token_form(
    client: &reqwest::Client,
    token_endpoint: &str,
    form: &[(&'static str, String)],
) -> Result<TokenResponse, TokenError> {
    let response = client
        .post(token_endpoint)
        .form(form)
        .send()
        .await
        .map_err(|err| TokenError {
            rejected: false,
            message: format!("token request failed: {err}"),
        })?;
    let status = response.status();
    if !status.is_success() {
        // Do NOT echo the response body — a non-standard server could reflect
        // submitted token material back in its error body.
        return Err(TokenError {
            rejected: is_grant_rejection(status),
            message: format!("token endpoint returned HTTP {status}"),
        });
    }
    let text = response.text().await.map_err(|err| TokenError {
        rejected: false,
        message: err.to_string(),
    })?;
    serde_json::from_str(&text).map_err(|_| TokenError {
        rejected: false,
        message: "token endpoint returned an invalid response".to_string(),
    })
}

#[cfg(test)]
#[path = "flow_tests.rs"]
mod tests;
