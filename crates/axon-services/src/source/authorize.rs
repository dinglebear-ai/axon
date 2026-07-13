//! Authorizing stage: turns a routed [`RoutePlan`]'s
//! `credential_requirements` into an access decision before acquisition.
//!
//! Per `docs/pipeline-unification/foundation/source-pipeline.md` Stage
//! Registry, `authorizing` consumes the route plan and produces an
//! access/execution decision — it must not degrade or mutate. This is the
//! first real consumer of `RoutePlan::credential_requirements`: previously the
//! router computed them (see `axon-route`'s `AdapterDefinition::with_credential`)
//! but nothing read them, so a source route that a family adapter cannot
//! authenticate for was only ever caught later, mid-acquisition, by that
//! family's own fetch helper (e.g. `fetch_reddit_dump`).
//!
//! Credential *presence* is checked via the well-known environment variables
//! Axon already documents per adapter (see repo root `CLAUDE.md`, "Ingest
//! credentials"). A [`CredentialRequirement`] with an explicit `secret_ref`
//! is assumed pre-resolved by the caller and is not re-checked here.

use axon_api::source::{AuthMode, AuthScope, AuthSnapshot, CredentialKind, RoutePlan, SafetyClass};
use axon_authz::required_scope_for_safety_class;
use axon_error::{ApiError, ErrorStage};

/// Authorize a routed source against its adapter's declared credential
/// requirements. Returns `Ok(())` when every `required` credential is either
/// pre-resolved (`secret_ref` set) or available in the environment; otherwise
/// returns the first missing requirement as an `Authorizing`-stage error.
pub fn authorize_route(route: &RoutePlan) -> Result<(), ApiError> {
    for requirement in &route.credential_requirements {
        if !requirement.required || requirement.secret_ref.is_some() {
            continue;
        }
        if credential_present_in_env(&route.adapter.name, requirement.credential_kind) {
            continue;
        }
        return Err(ApiError::new(
            "auth.credential_missing",
            ErrorStage::Authorizing,
            requirement.reason.clone(),
        )
        .with_context("adapter", route.adapter.name.clone())
        .with_context(
            "credential_kind",
            format!("{:?}", requirement.credential_kind),
        ));
    }
    Ok(())
}

/// Well-known env vars backing each adapter's declared credential
/// requirements. Adapters with no declared requirements (the common case)
/// never reach this function's body via `authorize_route`'s early `continue`.
///
/// **Fail-closed by design.** An `(adapter, kind)` pair with no known env
/// mapping below returns `false` (not authorized), not `true`. A `required`
/// credential requirement paired with an adapter/kind this function doesn't
/// recognize means Axon has no way to verify the credential is actually
/// available, so `authorize_route` must deny rather than silently let
/// acquisition through unauthenticated. When wiring a new adapter's required
/// credential, add its mapping here — do not rely on the default arm to
/// "just work".
fn credential_present_in_env(adapter_name: &str, kind: CredentialKind) -> bool {
    match (adapter_name, kind) {
        ("reddit", CredentialKind::ApiKey) => {
            env_present("REDDIT_CLIENT_ID") && env_present("REDDIT_CLIENT_SECRET")
        }
        _ => false,
    }
}

fn env_present(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| !value.trim().is_empty())
}

/// Re-authorize the routed source kind against the caller snapshot at the
/// actual execution boundary. `None` represents trusted local/loopback/CLI
/// execution and mirrors the transport-level convention.
pub fn authorize_safety_class(
    safety_class: SafetyClass,
    auth_snapshot: Option<&AuthSnapshot>,
) -> Result<(), ApiError> {
    let Some(snapshot) = auth_snapshot else {
        return Ok(());
    };

    let required_scope = required_scope_for_safety_class(safety_class);
    let Some(required) = AuthScope::from_scope_str(required_scope) else {
        return Err(ApiError::new(
            "auth.scope_unrecognized",
            ErrorStage::Authorizing,
            format!("unrecognized safety-class scope requirement: {required_scope}"),
        ));
    };

    if snapshot_allows_scope(snapshot, required) {
        return Ok(());
    }

    Err(ApiError::new(
        "auth.scope_required",
        ErrorStage::Authorizing,
        format!("source requires scope: {required_scope}"),
    )
    .with_context("required_scope", required_scope)
    .with_context("safety_class", format!("{safety_class:?}")))
}

/// Whether a persisted caller snapshot is trusted for a concrete scope at the
/// source execution boundary.
pub(crate) fn snapshot_allows_scope(snapshot: &AuthSnapshot, required: AuthScope) -> bool {
    matches!(snapshot.auth_mode, AuthMode::TrustedLocal)
        || snapshot.granted_scopes.contains(&required)
}

#[cfg(test)]
#[path = "authorize_tests.rs"]
mod tests;
