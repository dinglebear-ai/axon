//! Axon OAuth scope constants and scope-satisfaction logic.
//!
//! These scope strings are embedded in issued OAuth tokens. Changing the
//! `axon:read` / `axon:write` string values would invalidate every existing
//! token, so they are a hard security invariant (see the workspace crate
//! extraction inventory, §5.4 "Authz scope constants"). Do not alter the
//! literal values.

#![allow(clippy::too_many_arguments)]

pub mod affinity;
pub mod caller;
pub mod decision;
pub mod http;
pub mod policy;
pub mod visibility;

pub use affinity::{AffinityPolicy, required_scope_for_safety_class};
pub use caller::{anonymous_caller, scoped_caller, system_caller, trusted_local_caller};
pub use decision::{FakePolicyEvaluator, FakePolicyMode, PolicyEvaluator, ScopePolicyEvaluator};
pub use visibility::VisibilityPolicy;

// DTOs owned by `axon-api`, re-exported for ergonomic access to this crate's
// public policy-evaluation API surface (auth-contract "Public API" list) —
// this crate evaluates policy over these shapes, it does not redefine them.
pub use axon_api::source::{AuthScope, CallerContext, ExecutionAffinity, SecurityDecision};

/// OAuth scope granting read access to Axon read/RAG routes.
pub const AXON_READ_SCOPE: &str = "axon:read";
/// OAuth scope granting write access to Axon mutating routes.
pub const AXON_WRITE_SCOPE: &str = "axon:write";
/// OAuth scope granting admin access to destructive/prune/reset routes.
///
/// Per the auth contract, `axon:write` does NOT imply `axon:admin`.
pub const AXON_ADMIN_SCOPE: &str = "axon:admin";
/// OAuth scope granting CLI/MCP tool-execution source access.
///
/// Per the auth contract, `axon:execute` is independent from write/admin.
pub const AXON_EXECUTE_SCOPE: &str = "axon:execute";
/// OAuth scope granting local-filesystem source access.
///
/// Per the auth contract, `axon:local` is independent from write/admin.
pub const AXON_LOCAL_SCOPE: &str = "axon:local";
/// Combined read+write scope string issued to fully-authorized OAuth users.
pub const AXON_FULL_ACCESS_SCOPE: &str = "axon:read axon:write";

/// Returns whether `scopes` satisfies `required_scope`.
///
/// The implication matrix is deliberately narrow: the legacy `axon:read` and
/// `axon:write` pair remain mutually compatible, while admin, execute, and
/// local are independent capabilities and require an exact hold. Non-Axon
/// scopes also require an exact match.
pub fn scope_satisfies(scopes: &[String], required_scope: &str) -> bool {
    scopes
        .iter()
        .flat_map(|scope| scope.split_whitespace())
        .any(|held| {
            held == required_scope
                || (matches!(held, AXON_READ_SCOPE | AXON_WRITE_SCOPE)
                    && matches!(required_scope, AXON_READ_SCOPE | AXON_WRITE_SCOPE))
        })
}

/// Returns whether `scopes` holds `required_scope` **exactly**, with none of
/// [`scope_satisfies`]'s legacy broad-scope compatibility implication.
///
/// ## Why this exists — do not fold it back into `scope_satisfies`
///
/// Use this for operations that require the literal capability even when an
/// implication is otherwise valid, such as lifecycle mutation elevation.
pub fn has_explicit_scope(scopes: &[String], required_scope: &str) -> bool {
    scopes
        .iter()
        .flat_map(|scope| scope.split_whitespace())
        .any(|scope| scope == required_scope)
}

#[path = "lib_tests.rs"]
#[cfg(test)]
mod tests;
