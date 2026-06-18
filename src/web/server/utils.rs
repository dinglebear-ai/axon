use super::state::AppState;
use crate::mcp::auth::configured_mcp_http_token;
use axum::http::HeaderMap;
use subtle::ConstantTimeEq;

/// Log a startup warning when `AXON_MCP_HTTP_TOKEN` is set but resolves to
/// empty/whitespace — the operator clearly meant to enable auth, and
/// the empty value is ignored and loopback-only tokenless mode may apply.
pub(crate) fn warn_if_ask_token_set_but_empty() {
    if let Ok(raw) = std::env::var("AXON_MCP_HTTP_TOKEN")
        && !raw.is_empty()
        && raw.trim().is_empty()
    {
        tracing::warn!(
            context = "v1_ask_startup",
            "AXON_MCP_HTTP_TOKEN is set to whitespace — the value is ignored; configure a non-empty token before exposing HTTP beyond loopback"
        );
    }
}

pub fn authorized(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(token) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| {
            headers
                .get("x-axon-panel-token")
                .and_then(|v| v.to_str().ok())
        })
        .or_else(|| headers.get("x-api-key").and_then(|v| v.to_str().ok()))
    else {
        return false;
    };

    state.panel.password.verify(token) || verify_configured_api_token(token)
}

fn verify_configured_api_token(candidate: &str) -> bool {
    configured_mcp_http_token()
        .map(|expected| {
            expected
                .as_bytes()
                .ct_eq(candidate.trim().as_bytes())
                .into()
        })
        .unwrap_or(false)
}
