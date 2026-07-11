//! Execution-affinity policy.
//!
//! Maps a source's [`SafetyClass`] (how it acquires data — network, local
//! filesystem, tool execution) to the scope required to run it, and decides
//! whether a given caller may run a source at a given
//! [`ExecutionAffinity`] (inline vs. worker/scheduler/provider-bound). Scope
//! checking itself stays in [`crate::policy::ScopeSecurityPolicy`] — this
//! module owns only the safety-class → scope mapping and the affinity-aware
//! decision wrapper described in the auth contract.

use axon_api::source::{
    CallerContext, ExecutionAffinity, SafetyClass, SecurityDecision, SourceWarning,
};

use crate::{AXON_EXECUTE_SCOPE, AXON_LOCAL_SCOPE, AXON_WRITE_SCOPE, scope_satisfies};

/// Resolve the scope required to authorize a source at a given safety class.
///
/// Scope rules (auth contract):
/// - CLI/MCP tool execution sources require `axon:execute`.
/// - Local filesystem sources require `axon:local`.
/// - Public/authenticated network sources fall under the general `axon:write`
///   source-job requirement.
pub fn required_scope_for_safety_class(safety_class: SafetyClass) -> &'static str {
    match safety_class {
        SafetyClass::ToolExecution => AXON_EXECUTE_SCOPE,
        SafetyClass::LocalFilesystem => AXON_LOCAL_SCOPE,
        SafetyClass::PublicNetwork | SafetyClass::AuthenticatedNetwork => AXON_WRITE_SCOPE,
    }
}

/// Execution-affinity policy: decides whether `caller` may run a source at
/// `safety_class` with the given `affinity`.
///
/// `Inline` execution additionally requires local trust for
/// `SafetyClass::LocalFilesystem` and `SafetyClass::ToolExecution` — running
/// those directly on the caller's request thread (rather than a sandboxed
/// worker) is only safe for a trusted-local caller, even if they hold the
/// bare scope. `Worker`/`Scheduler`/`ProviderBound` affinities are scope-only
/// checks: the job runtime is the sandbox boundary for those.
#[derive(Debug, Clone, Copy, Default)]
pub struct AffinityPolicy;

impl AffinityPolicy {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate(
        &self,
        caller: &CallerContext,
        safety_class: SafetyClass,
        affinity: ExecutionAffinity,
    ) -> SecurityDecision {
        let required_scope = required_scope_for_safety_class(safety_class);
        let has_scope = scope_satisfies(&caller.scopes, required_scope);

        let requires_local_trust = affinity == ExecutionAffinity::Inline
            && matches!(
                safety_class,
                SafetyClass::LocalFilesystem | SafetyClass::ToolExecution
            );

        let allowed = has_scope && (!requires_local_trust || caller.trusted_local);

        let reason = if allowed {
            "authorized.affinity_satisfied".to_string()
        } else if !has_scope {
            "denied.scope_missing".to_string()
        } else {
            "denied.affinity_requires_local_trust".to_string()
        };

        let warnings = if allowed {
            Vec::new()
        } else if requires_local_trust && has_scope && !caller.trusted_local {
            vec![SourceWarning {
                code: "affinity_requires_local_trust".to_string(),
                severity: axon_api::source::Severity::Warning,
                message: format!(
                    "inline execution of a {safety_class:?} source requires a trusted-local caller"
                ),
                source_item_key: None,
                retryable: false,
            }]
        } else {
            Vec::new()
        };

        SecurityDecision {
            allowed,
            scope: required_scope.to_string(),
            reason,
            redactions: Vec::new(),
            warnings,
        }
    }
}

#[cfg(test)]
#[path = "affinity_tests.rs"]
mod tests;
