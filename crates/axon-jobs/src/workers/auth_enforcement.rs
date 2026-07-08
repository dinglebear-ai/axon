use axon_api::source::{ApiError, AuthScope, AuthSnapshot, ErrorStage, JobKind};

/// The `AuthScope` a job of the given kind must hold before it may execute.
///
/// `Reset` and `Prune` are destructive/admin-only. Every other kind was
/// already scope-checked at submission time (REST/MCP intake maps
/// `SafetyClass` to `axon:local`/`axon:execute`/`axon:write` before a job row
/// is even created), so no additional fine-grained scope is required here —
/// this is a second, execution-time gate for the operations where a stale or
/// tampered snapshot would otherwise cause real damage.
pub(crate) fn required_scope_for_kind(kind: JobKind) -> Option<AuthScope> {
    match kind {
        JobKind::Reset | JobKind::Prune => Some(AuthScope::Admin),
        _ => None,
    }
}

/// Deny execution if `snapshot` was not granted `required`. Callers must run
/// this before dispatching to a runner, not after — it fails closed before
/// any side effect.
pub(crate) fn require_job_scope(
    snapshot: &AuthSnapshot,
    required: AuthScope,
) -> Result<(), ApiError> {
    if snapshot.granted_scopes.contains(&required) {
        return Ok(());
    }
    Err(ApiError::new(
        "auth.scope_required",
        ErrorStage::Authorizing,
        format!("operation requires {required:?} scope"),
    )
    .with_context("required_scope", format!("{required:?}")))
}

/// Project a parent job's granted scopes onto a child job's auth snapshot.
///
/// The child inherits exactly the parent's grants — never more. A job
/// created by the system on the parent's behalf (a watch spawning a crawl, a
/// retry, a stale reclaim) must not gain scope the original request never
/// held.
///
/// `pub` (not `pub(crate)`): `axon-services` (a downstream crate per the
/// `axon-jobs` -> `axon-services` layering direction) calls this directly
/// when tracking child `graph`/`prune` jobs of a parent `Source` job, so the
/// child job's stored auth snapshot reflects the real caller's grants
/// instead of a hardcoded elevated default.
pub fn child_auth_snapshot(parent: &AuthSnapshot) -> AuthSnapshot {
    parent.clone()
}

#[cfg(test)]
#[path = "auth_enforcement_tests.rs"]
mod tests;
