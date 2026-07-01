use async_trait::async_trait;
use axon_api::source::*;

use crate::scope_satisfies;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait SecurityPolicy: Send + Sync {
    async fn authorize_source(&self, request: SecurityPolicyRequest) -> Result<SecurityDecision>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}

#[derive(Debug, Clone)]
pub struct ScopeSecurityPolicy {
    required_scope: String,
}

impl ScopeSecurityPolicy {
    pub fn new(required_scope: impl Into<String>) -> Self {
        Self {
            required_scope: required_scope.into(),
        }
    }
}

#[async_trait]
impl SecurityPolicy for ScopeSecurityPolicy {
    async fn authorize_source(&self, request: SecurityPolicyRequest) -> Result<SecurityDecision> {
        let allowed = scope_satisfies(&request.caller.scopes, &self.required_scope);
        Ok(SecurityDecision {
            allowed,
            scope: self.required_scope.clone(),
            reason: if allowed {
                "scope satisfied".to_string()
            } else {
                "scope missing".to_string()
            },
            redactions: Vec::new(),
            warnings: Vec::new(),
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(ProviderCapability {
            provider_id: ProviderId::new("scope-security-policy"),
            provider_kind: ProviderKind::Security,
            implementation: "scope".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            health: HealthStatus::Healthy,
            limits: ProviderLimits::default(),
            features: vec!["source_authorization".to_string()],
            cooldown_until: None,
            last_error: None,
            reservation_policy: ReservationPolicy {
                supports_reservations: false,
                queue_policy: QueuePolicy::Fifo,
                interactive_reserve: 0,
                cooldown_after_failures: 0,
                cooldown_secs: 0,
                retry_backoff_ms: None,
            },
            reservation_state: ReservationStateSnapshot {
                queued: 0,
                active: 0,
                available_units: 1,
                oldest_queued_ms: None,
                priority_breakdown: Default::default(),
                states: Vec::new(),
            },
            cost_class: ProviderCostClass::Internal,
            degraded_modes: Vec::new(),
            fake_overrides_supported: false,
            embedding: None,
            llm: None,
            vector_store: None,
            fetch: None,
            render: None,
            credential: None,
        })
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
