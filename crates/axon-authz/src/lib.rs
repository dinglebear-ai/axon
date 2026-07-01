//! Axon OAuth scope constants and scope-satisfaction logic.
//!
//! These scope strings are embedded in issued OAuth tokens. Changing the
//! `axon:read` / `axon:write` string values would invalidate every existing
//! token, so they are a hard security invariant (see the workspace crate
//! extraction inventory, §5.4 "Authz scope constants"). Do not alter the
//! literal values.

pub mod http;
pub mod policy;

/// OAuth scope granting read access to Axon read/RAG routes.
pub const AXON_READ_SCOPE: &str = "axon:read";
/// OAuth scope granting write access to Axon mutating routes.
pub const AXON_WRITE_SCOPE: &str = "axon:write";
/// Combined read+write scope string issued to fully-authorized OAuth users.
pub const AXON_FULL_ACCESS_SCOPE: &str = "axon:read axon:write";

/// Returns whether `scopes` satisfies `required_scope`.
///
/// Either Axon scope satisfies any Axon-scoped route (read and write are
/// treated as interchangeable for Axon routes, matching the OAuth dual-scope
/// compatibility contract). Non-Axon scopes require an exact match.
pub fn scope_satisfies(scopes: &[String], required_scope: &str) -> bool {
    if is_axon_scope(required_scope) {
        return scopes.iter().any(|scope| is_axon_scope(scope));
    }
    scopes.iter().any(|scope| scope == required_scope)
}

fn is_axon_scope(scope: &str) -> bool {
    matches!(scope, AXON_READ_SCOPE | AXON_WRITE_SCOPE)
}

#[path = "lib_tests.rs"]
#[cfg(test)]
mod tests;
