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
/// The broad `axon:read` / `axon:write` pair remains interchangeable for the
/// broad read/write route groups (OAuth dual-scope compatibility contract):
/// either broad Axon scope satisfies a required broad Axon scope.
///
/// The fine-grained `axon:admin` / `axon:execute` / `axon:local` scopes are
/// NOT satisfied by the broad read/write scopes — they require the caller to
/// actually hold that exact scope (auth contract: "`axon:write` does not imply
/// `axon:admin`, `axon:execute`, or `axon:local`"). A caller that holds one of
/// the fine-grained scopes still satisfies broad read/write routes, because any
/// Axon scope counts as authenticated Axon access for the broad groups.
///
/// Non-Axon scopes require an exact match.
pub fn scope_satisfies(scopes: &[String], required_scope: &str) -> bool {
    if is_fine_grained_axon_scope(required_scope) {
        // Fine-grained scopes require the caller to hold that exact scope.
        return scopes
            .iter()
            .flat_map(|scope| scope.split_whitespace())
            .any(|scope| scope == required_scope);
    }
    if is_broad_axon_scope(required_scope) {
        return scopes.iter().any(|scope| is_axon_scope(scope));
    }
    scopes.iter().any(|scope| scope == required_scope)
}

/// True for any Axon scope string (broad or fine-grained). Used to decide
/// whether a *held* scope counts as authenticated Axon access for broad routes.
fn is_axon_scope(scope: &str) -> bool {
    scope.split_whitespace().any(|scope| {
        matches!(
            scope,
            AXON_READ_SCOPE
                | AXON_WRITE_SCOPE
                | AXON_ADMIN_SCOPE
                | AXON_EXECUTE_SCOPE
                | AXON_LOCAL_SCOPE
        )
    })
}

/// True only for the broad read/write scopes that are interchangeable.
fn is_broad_axon_scope(scope: &str) -> bool {
    scope
        .split_whitespace()
        .any(|scope| matches!(scope, AXON_READ_SCOPE | AXON_WRITE_SCOPE))
}

/// True only for the fine-grained admin/execute/local scopes that must be held
/// explicitly and are never implied by broad read/write.
fn is_fine_grained_axon_scope(scope: &str) -> bool {
    matches!(
        scope,
        AXON_ADMIN_SCOPE | AXON_EXECUTE_SCOPE | AXON_LOCAL_SCOPE
    )
}

/// Returns whether `scopes` holds `required_scope` **exactly**, with none of
/// [`scope_satisfies`]'s broad `axon:read`/`axon:write` interchangeability.
///
/// ## Why this exists — do not fold it back into `scope_satisfies`
///
/// `scope_satisfies` treats `axon:read` and `axon:write` as interchangeable
/// for the ordinary broad read/write route groups. That widening is a
/// deliberate OAuth dual-scope compatibility affordance — newly issued
/// tokens default to holding both scopes together
/// (`AXON_FULL_ACCESS_SCOPE`), and existing tokens issued before some route's
/// classification changed must keep working. See root `CLAUDE.md`'s "MCP
/// Security Env" section and
/// `docs/reference/runtime/security.md`'s "Contract"
/// paragraph for the compatibility rationale, and
/// `docs/reference/runtime/auth.md`'s "Scope Rules" for
/// the documented exception.
///
/// A small number of call sites intentionally opt **out** of that widening:
/// conditional scope *elevation* checks, where a route is nominally
/// `axon:read` for schema/docs purposes but actually mutates state today
/// (`require_mutates_if_write_scope` in `axon-web`'s
/// `handlers/exploration.rs`, mirrored by `axon-mcp`'s
/// `server::authz::mutates_if_upgrade`/`check_scope_explicit`). If those
/// checks used `scope_satisfies`, the elevation would be a silent no-op:
/// `is_broad_axon_scope(AXON_WRITE_SCOPE)` is true, so
/// `scope_satisfies(["axon:read"], "axon:write")` already returns `true`
/// before this function is ever consulted — a read-only caller would sail
/// through a check that exists specifically to stop them (CWE-863,
/// documented in
/// `docs/reference/runtime/auth.md`'s "Scope Rules").
/// `has_explicit_scope` closes that hole by requiring the caller to hold the
/// exact scope string, with no broad-scope widening in either direction.
///
/// Use this function **only** for elevation/upgrade checks layered on top of
/// a nominal scope class. Use `scope_satisfies` for every ordinary
/// route/action scope check — replacing it here would break every existing
/// deployed token that was issued before dual-scope compatibility, which is
/// exactly the behavior `scope_satisfies` exists to preserve.
pub fn has_explicit_scope(scopes: &[String], required_scope: &str) -> bool {
    scopes
        .iter()
        .flat_map(|scope| scope.split_whitespace())
        .any(|scope| scope == required_scope)
}

#[path = "lib_tests.rs"]
#[cfg(test)]
mod tests;
