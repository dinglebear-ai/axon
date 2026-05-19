pub(crate) const AXON_READ_SCOPE: &str = "axon:read";
pub(crate) const AXON_WRITE_SCOPE: &str = "axon:write";
pub(crate) const AXON_FULL_ACCESS_SCOPE: &str = "axon:read axon:write";

pub(crate) fn scope_satisfies(scopes: &[String], required_scope: &str) -> bool {
    if is_axon_scope(required_scope) {
        return scopes.iter().any(|scope| is_axon_scope(scope));
    }
    scopes.iter().any(|scope| scope == required_scope)
}

fn is_axon_scope(scope: &str) -> bool {
    matches!(scope, AXON_READ_SCOPE | AXON_WRITE_SCOPE)
}

#[path = "authz_tests.rs"]
#[cfg(test)]
mod tests;
