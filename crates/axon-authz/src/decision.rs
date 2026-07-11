//! `PolicyEvaluator`: the single entry point transports/services call before
//! executing a security-sensitive operation.
//!
//! Composes the three policy primitives this crate owns — scope checking
//! ([`crate::policy::ScopeSecurityPolicy`]), visibility ceiling derivation
//! ([`crate::visibility::VisibilityPolicy`]), and execution-affinity decisions
//! ([`crate::affinity::AffinityPolicy`]) — behind one trait so callers ask one
//! question ("can this caller do this?") instead of duplicating policy logic
//! per transport, per the auth contract.

use async_trait::async_trait;
use axon_api::source::{
    CallerContext, ExecutionAffinity, SafetyClass, SecurityDecision, SecurityPolicyRequest,
    Visibility,
};
use axon_error::ApiError;

use crate::affinity::AffinityPolicy;
use crate::policy::{ScopeSecurityPolicy, SecurityPolicy};
use crate::visibility::VisibilityPolicy;

pub type Result<T> = std::result::Result<T, ApiError>;

/// The composed policy-evaluation surface. Transports and services call this
/// instead of hand-rolling scope checks, visibility ceilings, or affinity
/// decisions themselves.
#[async_trait]
pub trait PolicyEvaluator: Send + Sync {
    /// Authorize a source acquisition/execution request (scope check).
    async fn authorize_source(&self, request: SecurityPolicyRequest) -> Result<SecurityDecision>;

    /// Derive the visibility ceiling a caller may see.
    fn visibility_ceiling(&self, caller: &CallerContext) -> Visibility;

    /// Decide whether `caller` may run a source at `safety_class` with the
    /// given execution `affinity`.
    fn execution_affinity(
        &self,
        caller: &CallerContext,
        safety_class: SafetyClass,
        affinity: ExecutionAffinity,
    ) -> SecurityDecision;
}

/// Production `PolicyEvaluator`: scope checks stay the core (delegated to
/// [`ScopeSecurityPolicy`]); visibility and affinity are derived from the
/// caller per the auth contract.
#[derive(Debug, Clone)]
pub struct ScopePolicyEvaluator {
    scope_policy: ScopeSecurityPolicy,
    visibility_policy: VisibilityPolicy,
    affinity_policy: AffinityPolicy,
}

impl ScopePolicyEvaluator {
    pub fn new(required_scope: impl Into<String>) -> Self {
        Self {
            scope_policy: ScopeSecurityPolicy::new(required_scope),
            visibility_policy: VisibilityPolicy::new(),
            affinity_policy: AffinityPolicy::new(),
        }
    }
}

#[async_trait]
impl PolicyEvaluator for ScopePolicyEvaluator {
    async fn authorize_source(&self, request: SecurityPolicyRequest) -> Result<SecurityDecision> {
        self.scope_policy.authorize_source(request).await
    }

    fn visibility_ceiling(&self, caller: &CallerContext) -> Visibility {
        self.visibility_policy.ceiling_for(caller)
    }

    fn execution_affinity(
        &self,
        caller: &CallerContext,
        safety_class: SafetyClass,
        affinity: ExecutionAffinity,
    ) -> SecurityDecision {
        self.affinity_policy
            .evaluate(caller, safety_class, affinity)
    }
}

/// Test/fake `PolicyEvaluator` that can force allow, deny, or degrade paths
/// per the crate's fixture-and-fakes contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakePolicyMode {
    AllowAll,
    DenyAll,
    /// Allowed, but every decision carries a synthetic warning — used to
    /// exercise degraded-path handling in callers without a real degraded
    /// provider.
    Degrade,
}

#[derive(Debug, Clone)]
pub struct FakePolicyEvaluator {
    mode: FakePolicyMode,
    ceiling: Visibility,
}

impl FakePolicyEvaluator {
    pub fn new(mode: FakePolicyMode) -> Self {
        Self {
            mode,
            ceiling: Visibility::Public,
        }
    }

    pub fn allow_all() -> Self {
        Self::new(FakePolicyMode::AllowAll)
    }

    pub fn deny_all() -> Self {
        Self::new(FakePolicyMode::DenyAll)
    }

    pub fn degrade() -> Self {
        Self::new(FakePolicyMode::Degrade)
    }

    pub fn with_ceiling(mut self, ceiling: Visibility) -> Self {
        self.ceiling = ceiling;
        self
    }
}

#[async_trait]
impl PolicyEvaluator for FakePolicyEvaluator {
    async fn authorize_source(&self, request: SecurityPolicyRequest) -> Result<SecurityDecision> {
        Ok(self.decide(request.target))
    }

    fn visibility_ceiling(&self, _caller: &CallerContext) -> Visibility {
        self.ceiling
    }

    fn execution_affinity(
        &self,
        _caller: &CallerContext,
        _safety_class: SafetyClass,
        _affinity: ExecutionAffinity,
    ) -> SecurityDecision {
        self.decide(String::new())
    }
}

impl FakePolicyEvaluator {
    fn decide(&self, target: String) -> SecurityDecision {
        match self.mode {
            FakePolicyMode::AllowAll => SecurityDecision {
                allowed: true,
                scope: "fake:allow-all".to_string(),
                reason: "authorized.fake_allow_all".to_string(),
                redactions: Vec::new(),
                warnings: Vec::new(),
            },
            FakePolicyMode::DenyAll => SecurityDecision {
                allowed: false,
                scope: "fake:deny-all".to_string(),
                reason: "denied.fake_deny_all".to_string(),
                redactions: Vec::new(),
                warnings: Vec::new(),
            },
            FakePolicyMode::Degrade => SecurityDecision {
                allowed: true,
                scope: "fake:degrade".to_string(),
                reason: "authorized.fake_degrade".to_string(),
                redactions: Vec::new(),
                warnings: vec![axon_api::source::SourceWarning {
                    code: "fake_policy_degraded".to_string(),
                    severity: axon_api::source::Severity::Degraded,
                    message: format!(
                        "fake policy evaluator forced a degraded decision for {target}"
                    ),
                    source_item_key: None,
                    retryable: true,
                }],
            },
        }
    }
}

#[cfg(test)]
#[path = "decision_tests.rs"]
mod tests;
