use super::state::AppState;
use axon_api::source::{AuthMode, AuthSnapshot, CallerContext, TransportKind, Visibility};
use axon_authz::VisibilityPolicy;
use axum::http::HeaderMap;
use lab_auth::AuthContext;

/// Log a startup warning when `AXON_HTTP_TOKEN` is set but resolves to
/// empty/whitespace — the operator clearly meant to enable auth, and
/// the empty value is ignored and loopback-only tokenless mode may apply.
pub(crate) fn warn_if_ask_token_set_but_empty() {
    if let Ok(raw) = std::env::var("AXON_HTTP_TOKEN")
        && !raw.is_empty()
        && raw.trim().is_empty()
    {
        tracing::warn!(
            context = "v1_ask_startup",
            "AXON_HTTP_TOKEN is set to whitespace — the value is ignored; configure a non-empty token before exposing HTTP beyond loopback"
        );
    }
}

pub(crate) fn caller_context_from_auth(auth: &AuthContext) -> CallerContext {
    let auth_mode = if auth.sub == "static-bearer" {
        AuthMode::StaticToken
    } else {
        AuthMode::Oauth
    };
    let mut caller = CallerContext {
        caller_id: Some(auth.sub.clone()),
        transport: TransportKind::Rest,
        trusted_local: false,
        scopes: auth.scopes.clone(),
        visibility_ceiling: Visibility::Public,
        auth_mode,
        token_id: None,
        display_name: None,
    };
    caller.visibility_ceiling = VisibilityPolicy::new().ceiling_for(&caller);
    caller
}

pub(crate) fn auth_snapshot_from_auth(auth: &AuthContext) -> AuthSnapshot {
    let caller = caller_context_from_auth(auth);
    AuthSnapshot::from_caller(&caller, caller.visibility_ceiling, "runtime")
}

pub fn authorized(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(token) = headers
        .get("x-axon-panel-token")
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };

    state.panel.password.verify(token)
}
