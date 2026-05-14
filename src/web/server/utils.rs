use super::state::AppState;
use axum::http::HeaderMap;

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

pub(super) fn authorized(state: &AppState, headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| {
            headers
                .get("x-axon-panel-token")
                .and_then(|v| v.to_str().ok())
        })
        .map(|token| state.panel.password.verify(token))
        .unwrap_or(false)
}
