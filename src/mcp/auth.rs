//! Authentication policy and middleware for the MCP HTTP server.
//!
//! Guards `axon mcp --transport http` (and `--transport both`) endpoints.
//! Supports two modes, selected at startup:
//!
//! - **Bearer-only** (`AXON_MCP_HTTP_TOKEN` set, `AXON_MCP_AUTH_MODE=bearer`):
//!   static constant-time token comparison via lab-auth `AuthLayer`.
//! - **OAuth** (`AXON_MCP_AUTH_MODE=oauth`): Google OAuth 2.0 + JWT validation
//!   via lab-auth `AuthLayer`, with the OAuth router mounted alongside `/mcp`.
//!   The static bearer token continues to work in dual-mode (both static and
//!   JWT bearer are accepted simultaneously).
//!
//! Token resolution order (first non-empty value wins):
//! 1. `Authorization: Bearer <token>` — matched against static token (const-time)
//!    or validated as a JWT issued by the local auth state.
//! 2. `x-api-key: <token>` — same resolution (lab-auth handles both).
//!
//! The `AuthPolicy` enum centralises the startup decision:
//!
//! | `AXON_MCP_AUTH_MODE` | `AXON_MCP_HTTP_TOKEN` | bind      | policy                            |
//! |----------------------|-----------------------|-----------|-----------------------------------|
//! | `oauth`              | any                   | any       | `Mounted { auth_state: Some(_) }` |
//! | `bearer` (default)   | set                   | any       | `Mounted { auth_state: None }`    |
//! | `bearer` (default)   | unset                 | loopback  | `LoopbackDev`                     |
//! | `bearer` (default)   | unset                 | non-loop  | rejected at startup               |
//!
//! The old `mcp_auth_middleware` free function is retained only for the
//! existing unit tests in this module; production code uses `AuthPolicy` +
//! `build_auth_layer` from `mcp.rs`.

use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;

use axum::{body::Body, http::Request, middleware::Next, response::Response};
use lab_auth::{AuthLayer, state::AuthState};

#[cfg(test)]
use axum::http::{HeaderMap, StatusCode};
#[cfg(test)]
use axum::response::IntoResponse;
#[cfg(test)]
use subtle::ConstantTimeEq;

// ── AuthPolicy ────────────────────────────────────────────────────────────────

/// Authentication policy attached to the MCP router.
///
/// This is an enum (not `Option<Arc<AuthState>>` and not a `bool`) so that
/// constructing the router requires an *explicit* choice between "no auth
/// wired (loopback dev)" and "auth wired". There is no `Default` impl —
/// callers must name the variant.
///
/// Locked post-spike: when `auth_state` is `Some`, the shared
/// `lab_auth::state::AuthState` backs both the dual-mode middleware and the
/// OAuth router. When `None`, only static-bearer auth is active — middleware
/// validates the token but no OAuth flow is wired. `AuthContext` flows
/// per-request via axum extension propagation (rmcp 1.5+ injects
/// `http::request::Parts` into `RequestContext::extensions`).
#[derive(Clone)]
pub enum AuthPolicy {
    /// No authentication wired. Only legal when the MCP listener is bound to a
    /// loopback address. Scope checks are bypassed — the bind itself is the
    /// trust boundary. Also used unconditionally for stdio mode.
    LoopbackDev,
    /// Authentication middleware is mounted. Scope checks MUST run.
    ///
    /// - `Some(_)` — OAuth active: Google flow + JWKS issuance; OAuth router
    ///   is also mounted on `/.well-known/*`, `/authorize`, `/token`, etc.
    /// - `None` — bearer-only: middleware validates `AXON_MCP_HTTP_TOKEN` via
    ///   lab-auth's `AuthLayer::with_static_token`; no OAuth router mounted.
    Mounted { auth_state: Option<Arc<AuthState>> },
}

// Manual Debug: `lab_auth::state::AuthState` holds RSA signing keys that
// must never be printed.
impl std::fmt::Debug for AuthPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthPolicy::LoopbackDev => f.write_str("AuthPolicy::LoopbackDev"),
            AuthPolicy::Mounted {
                auth_state: Some(_),
            } => f.write_str("AuthPolicy::Mounted { auth_state: Some(<lab_auth::AuthState>) }"),
            AuthPolicy::Mounted { auth_state: None } => {
                f.write_str("AuthPolicy::Mounted { auth_state: None /* bearer-only */ }")
            }
        }
    }
}

/// Build the `lab_auth::AuthLayer` from an `AuthPolicy`, or return `None` for
/// `LoopbackDev` (no layer needed — loopback bind is the trust boundary).
///
/// Centralises `AuthLayer` construction so both `http.rs` can call it.
///
/// # Invariants
/// - `AuthLayer` MUST NOT add any DB write path. JWT validation is stateless
///   RS256 verify; static token is constant-time compare.
/// - `allow_session_cookie` is always `false` for axon — no browser UI.
pub fn build_auth_layer(
    policy: &AuthPolicy,
    static_token: Option<Arc<str>>,
    resource_url: Option<Arc<str>>,
) -> Option<AuthLayer> {
    match policy {
        AuthPolicy::LoopbackDev => None,
        AuthPolicy::Mounted { auth_state: None } => Some(
            // Bearer-only mode: explicitly grant both scopes to the static
            // token so that callers with a valid token can reach write actions
            // (matching how the OAuth path sets static_token_scopes in
            // AuthConfigBuilder). Without this, `static_token_scopes` is an
            // empty Vec and every scope check would fail even with a valid token.
            AuthLayer::new()
                .with_static_token(static_token)
                .with_static_token_scopes(vec!["axon:read".into(), "axon:write".into()])
                .with_resource_url(resource_url)
                .with_allow_session_cookie(false),
        ),
        AuthPolicy::Mounted {
            auth_state: Some(state),
        } => Some(
            // OAuth mode: AuthConfig already sets static_token_scopes via
            // AuthConfigBuilder::static_token_scopes; with_auth_state pulls
            // them from config automatically.
            AuthLayer::new()
                .with_static_token(static_token)
                .with_auth_state(Some(state.clone()))
                .with_resource_url(resource_url)
                .with_allow_session_cookie(false),
        ),
    }
}

/// OAuth protected-resource metadata URL base for `WWW-Authenticate`.
///
/// `lab-auth` still uses `AXON_MCP_PUBLIC_URL + /mcp` as the canonical
/// protected resource audience. This value intentionally stays at the public
/// origin because the unified Axum server mounts RFC 9728 metadata at
/// `/.well-known/oauth-protected-resource`, beside `/mcp`, not under it.
///
/// Only OAuth mode advertises this URL. Bearer-only and loopback development
/// modes intentionally omit it so static-token responses do not imply OAuth
/// discovery is mounted.
pub fn oauth_resource_url(policy: &AuthPolicy) -> Option<Arc<str>> {
    let oauth_active = matches!(
        policy,
        AuthPolicy::Mounted {
            auth_state: Some(_)
        }
    );
    oauth_resource_url_from_parts(oauth_active, std::env::var("AXON_MCP_PUBLIC_URL").ok())
}

fn oauth_resource_url_from_parts(
    oauth_active: bool,
    public_url: Option<String>,
) -> Option<Arc<str>> {
    oauth_active.then_some(())?;
    public_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|url| Arc::from(url.trim_end_matches('/')))
}

/// Decide which `AuthPolicy` to install for a given host + transport.
///
/// - Stdio mode: always `LoopbackDev` (process isolation is the trust
///   boundary). OAuth config is ignored with a warning.
/// - Non-loopback without any auth: rejected (non-loopback bind requires
///   either `AXON_MCP_HTTP_TOKEN` or `AXON_MCP_AUTH_MODE=oauth`).
/// - OAuth mode: builds `lab_auth::AuthState` and returns
///   `Mounted { auth_state: Some(_) }`.
/// - Bearer-only with a token: `Mounted { auth_state: None }`.
/// - Loopback without auth: `LoopbackDev` (dev/test shortcut).
pub async fn build_auth_policy(
    host: &str,
    is_stdio: bool,
) -> Result<AuthPolicy, Box<dyn std::error::Error>> {
    // Stdio always gets LoopbackDev regardless of env vars.
    if is_stdio {
        let auth_mode = std::env::var("AXON_MCP_AUTH_MODE")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if auth_mode == "oauth" {
            tracing::warn!(
                "AXON_MCP_AUTH_MODE=oauth is set but axon is starting in stdio mode — \
                 OAuth config is ignored; LoopbackDev policy applies (process isolation \
                 is the trust boundary). Use HTTP transport for auth enforcement."
            );
        }
        tracing::info!(
            "axon auth policy: LoopbackDev (stdio mode — process isolation is the trust boundary)"
        );
        return Ok(AuthPolicy::LoopbackDev);
    }

    let auth_mode = std::env::var("AXON_MCP_AUTH_MODE")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let oauth_active = auth_mode == "oauth";
    let static_token = configured_mcp_http_token();
    let static_token_active = static_token.is_some();

    if oauth_active {
        // Build lab-auth AuthState from env vars. The AuthConfigBuilder reads
        // from a Vec<(String,String)> source so we never call std::env::var
        // inside lab-auth — all values come from our typed extraction here.
        let public_url = std::env::var("AXON_MCP_PUBLIC_URL")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let google_client_id = std::env::var("AXON_MCP_GOOGLE_CLIENT_ID")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let google_client_secret = std::env::var("AXON_MCP_GOOGLE_CLIENT_SECRET")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let admin_email = std::env::var("AXON_MCP_AUTH_ADMIN_EMAIL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_default();

        let mut vars: Vec<(String, String)> = Vec::with_capacity(16);
        push_var(&mut vars, "AXON_MCP_AUTH_MODE", "oauth");
        if let Some(url) = public_url.as_deref() {
            push_var(&mut vars, "AXON_MCP_PUBLIC_URL", url);
        }
        if let Some(id) = google_client_id.as_deref() {
            push_var(&mut vars, "AXON_MCP_GOOGLE_CLIENT_ID", id);
        }
        if let Some(secret) = google_client_secret.as_deref() {
            push_var(&mut vars, "AXON_MCP_GOOGLE_CLIENT_SECRET", secret);
        }
        if !admin_email.is_empty() {
            push_var(&mut vars, "AXON_MCP_AUTH_ADMIN_EMAIL", &admin_email);
        }
        // Pass allowed redirect URIs; always include claude.ai as a default.
        let allowed_uris = build_allowed_redirect_uris();
        if !allowed_uris.is_empty() {
            push_var(
                &mut vars,
                "AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS",
                &allowed_uris,
            );
        }

        let auth_config = lab_auth::config::AuthConfigBuilder::new()
            .env_prefix("AXON_MCP")
            .session_cookie_name("axon_mcp_session")
            .scopes_supported(vec!["axon:read".into(), "axon:write".into()])
            .default_scope("axon:read")
            .resource_path("/mcp")
            .static_token_scopes(vec!["axon:read".into(), "axon:write".into()])
            .enable_dynamic_registration(true)
            .disable_static_token_with_oauth(false) // static bearer keeps working alongside OAuth
            .build_from_sources(vars)
            .map_err(|e| format!("failed to build lab-auth AuthConfig: {e}"))?;

        let auth_state = AuthState::new(auth_config)
            .await
            .map_err(|e| format!("failed to initialize lab-auth AuthState: {e}"))?;

        tracing::info!(
            oauth_active = true,
            static_token_active,
            "axon auth policy: Mounted (OAuth + lab-auth state initialized)"
        );

        return Ok(AuthPolicy::Mounted {
            auth_state: Some(Arc::new(auth_state)),
        });
    }

    // Bearer-only mode.
    if static_token_active {
        tracing::info!(
            host = %host,
            "axon auth policy: Mounted {{ auth_state: None }} (bearer-only; OAuth not wired)"
        );
        return Ok(AuthPolicy::Mounted { auth_state: None });
    }

    // No auth at all — only legal on loopback.
    // Strip IPv6 brackets ([::1] → ::1) before parsing so that bracketed
    // literals are recognised. Only strip if both brackets are present.
    let host_trimmed = host.trim();
    let host_for_parse = host_trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host_trimmed);
    let bind_is_loopback = IpAddr::from_str(host_for_parse)
        .map(|ip| ip.is_loopback())
        .unwrap_or_else(|_| host_trimmed.eq_ignore_ascii_case("localhost"));

    if bind_is_loopback {
        tracing::info!(
            host = %host,
            "axon auth policy: LoopbackDev (no auth wired; loopback bind)"
        );
        return Ok(AuthPolicy::LoopbackDev);
    }

    // Non-loopback without auth — refuse to start.
    Err(format!(
        "refusing to start unauthenticated MCP HTTP server on non-loopback host '{host}'; \
         set AXON_MCP_HTTP_TOKEN or set AXON_MCP_AUTH_MODE=oauth and configure OAuth env vars, \
         or bind AXON_MCP_HTTP_HOST to 127.0.0.1/localhost"
    )
    .into())
}

/// Build the `AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS` value.
///
/// Always includes `https://claude.ai/api/mcp/auth_callback` so claude.ai
/// MCP clients can complete DCR registration. Additional URIs from the env var
/// are appended.
fn build_allowed_redirect_uris() -> String {
    let mut uris: Vec<String> = vec!["https://claude.ai/api/mcp/auth_callback".into()];
    if let Ok(extra) = std::env::var("AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS") {
        for u in extra.split(',') {
            let u = u.trim();
            if !u.is_empty() && !uris.iter().any(|existing| existing == u) {
                uris.push(u.to_string());
            }
        }
    }
    uris.join(",")
}

fn push_var(vars: &mut Vec<(String, String)>, key: &str, value: &str) {
    vars.push((key.to_string(), value.to_string()));
}

// ── Legacy static-token helpers (test-only; production uses lab_auth::AuthLayer) ──

pub(crate) fn configured_mcp_http_token() -> Option<String> {
    std::env::var("AXON_MCP_HTTP_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Authorize headers using the same static-token semantics as MCP HTTP.
///
/// `None` means token auth is not configured and the request is allowed. A
/// configured empty/whitespace token should be handled by the env-aware wrapper
/// below and fail closed.
#[cfg(test)]
pub(crate) fn authorize_mcp_http_headers(
    headers: &HeaderMap,
    configured_token: Option<&str>,
) -> Result<(), StatusCode> {
    let Some(expected) = configured_token.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .and_then(|v| {
            let (scheme, token) = v.split_once(' ')?;
            scheme
                .eq_ignore_ascii_case("Bearer")
                .then_some(token.trim())
                .filter(|s| !s.is_empty())
        });
    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let matches_expected = |token: &str| bool::from(token.as_bytes().ct_eq(expected.as_bytes()));
    if bearer.is_some_and(matches_expected) || api_key.is_some_and(matches_expected) {
        return Ok(());
    }
    Err(StatusCode::UNAUTHORIZED)
}

/// Rewrite `x-api-key: <token>` to `Authorization: Bearer <token>` so legacy
/// clients continue to work with `lab_auth::AuthLayer`, which reads bearer
/// authorization only.
pub(crate) async fn normalize_api_key_header(mut req: Request<Body>, next: Next) -> Response {
    if !req.headers().contains_key("authorization")
        && let Some(key_val) = req
            .headers()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| format!("Bearer {v}").parse().ok())
    {
        req.headers_mut().insert("authorization", key_val);
    }
    next.run(req).await
}

#[cfg(test)]
#[path = "auth_legacy_tests.rs"]
mod legacy;

#[cfg(test)]
#[path = "auth/tests.rs"]
mod tests;
