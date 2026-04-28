//! HTTP authentication middleware for the MCP HTTP server.
//!
//! Guards `axon mcp --transport http` (and `--transport both`) endpoints.
//! Set `AXON_MCP_HTTP_TOKEN` to enable token-based auth. If unset, the server
//! logs a one-time warning and allows all requests through.
//!
//! Token resolution order (first non-empty value wins):
//! 1. `Authorization: Bearer <token>` request header
//! 2. `x-api-key: <token>` request header

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use subtle::ConstantTimeEq;

/// Constant-time byte comparison to prevent timing attacks on API token checks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

/// Extract the bearer or x-api-key token from request headers.
fn extract_token(req: &Request<Body>) -> Option<&str> {
    req.headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .and_then(|v| {
            let (scheme, token) = v.split_once(' ')?;
            scheme
                .eq_ignore_ascii_case("Bearer")
                .then_some(token.trim())
                .filter(|s| !s.is_empty())
        })
        .or_else(|| {
            req.headers()
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
}

/// Axum middleware that enforces `AXON_MCP_HTTP_TOKEN` on all MCP HTTP requests.
///
/// - Token set: 401 for missing or mismatched token.
/// - Token NOT set: warn once at startup and allow through.
pub async fn mcp_auth_middleware(request: Request<Body>, next: Next) -> Response {
    let configured_token = std::env::var("AXON_MCP_HTTP_TOKEN").ok();
    let configured_token = configured_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    match configured_token {
        Some(expected) => {
            let provided = extract_token(&request).unwrap_or("").trim();
            if provided.is_empty() {
                return (StatusCode::UNAUTHORIZED, "missing token").into_response();
            }
            if !constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
                return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
            }
            next.run(request).await
        }
        None => {
            static WARNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
            WARNED.get_or_init(|| {
                tracing::warn!(
                    context = "mcp_auth",
                    "AXON_MCP_HTTP_TOKEN not set \u{2014} MCP HTTP server is unauthenticated"
                );
            });
            next.run(request).await
        }
    }
}
