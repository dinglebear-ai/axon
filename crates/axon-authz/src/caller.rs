//! Caller identity construction helpers.
//!
//! `CallerContext` itself is a transport-neutral DTO owned by `axon-api`
//! (`axon_api::source::CallerContext`) — see the DTO ownership note in
//! `crates/axon-authz/src/CLAUDE.md`. This module owns the *policy* around how
//! a `CallerContext` gets built for the handful of well-known caller shapes
//! (trusted local CLI, anonymous/system, and an explicit scoped caller), so
//! transports do not each reinvent the auth-contract "Trusted CLI Context"
//! rule.

use axon_api::source::{AuthMode, CallerContext, TransportKind};

/// Build a `CallerContext` for a locally-trusted CLI invocation.
///
/// Per the auth contract's "Trusted CLI Context": local CLI may be trusted
/// when running as the local user and not through a remote transport. This
/// is the only constructor that sets `trusted_local: true` — REST and MCP
/// callers must never infer local trust from network location alone, so they
/// should use [`scoped_caller`] instead.
pub fn trusted_local_caller(caller_id: impl Into<String>, scopes: Vec<String>) -> CallerContext {
    CallerContext {
        caller_id: Some(caller_id.into()),
        transport: TransportKind::Cli,
        trusted_local: true,
        scopes,
        visibility_ceiling: axon_api::source::Visibility::Internal,
        auth_mode: AuthMode::TrustedLocal,
        token_id: None,
        display_name: None,
    }
}

/// Build a `CallerContext` for an internal system/worker caller (jobs,
/// scheduler ticks, watch runs) that carries no end-user identity.
pub fn system_caller() -> CallerContext {
    CallerContext {
        caller_id: Some("axon-system".to_string()),
        transport: TransportKind::System,
        trusted_local: true,
        scopes: Vec::new(),
        visibility_ceiling: axon_api::source::Visibility::Internal,
        auth_mode: AuthMode::TrustedLocal,
        token_id: None,
        display_name: Some("Axon system".to_string()),
    }
}

/// Build a `CallerContext` for an authenticated remote caller (REST bearer /
/// OAuth, MCP). `trusted_local` is always `false` here — remote transports
/// never infer local trust from network location alone (auth contract).
pub fn scoped_caller(
    caller_id: Option<String>,
    transport: TransportKind,
    scopes: Vec<String>,
    auth_mode: AuthMode,
    token_id: Option<String>,
    display_name: Option<String>,
) -> CallerContext {
    CallerContext {
        caller_id,
        transport,
        trusted_local: false,
        scopes,
        visibility_ceiling: axon_api::source::Visibility::Public,
        auth_mode,
        token_id,
        display_name,
    }
}

/// Build an anonymous caller with no scopes for a given transport (e.g. an
/// unauthenticated loopback-dev request). Carries no implicit trust.
pub fn anonymous_caller(transport: TransportKind) -> CallerContext {
    CallerContext {
        caller_id: None,
        transport,
        trusted_local: false,
        scopes: Vec::new(),
        visibility_ceiling: axon_api::source::Visibility::Public,
        auth_mode: AuthMode::None,
        token_id: None,
        display_name: None,
    }
}

#[cfg(test)]
#[path = "caller_tests.rs"]
mod tests;
