use async_trait::async_trait;
use axon_api::source::*;

use crate::scope_satisfies;
#[cfg(test)]
use crate::{AXON_FULL_ACCESS_SCOPE, AXON_READ_SCOPE};

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait SecurityPolicy: Send + Sync {
    async fn authorize_source(&self, request: SecurityPolicyRequest) -> Result<SecurityDecision>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}

#[async_trait]
pub trait CredentialProvider: Send + Sync {
    async fn resolve(&self, request: CredentialRequest) -> Result<CredentialMaterial>;
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

#[derive(Debug, Clone, Default)]
pub struct FakeCredentialProvider;

impl FakeCredentialProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CredentialProvider for FakeCredentialProvider {
    async fn resolve(&self, request: CredentialRequest) -> Result<CredentialMaterial> {
        Ok(CredentialMaterial {
            secret_ref: request.secret_ref,
            credential_kind: request.credential_kind,
            redacted_value: "redacted".to_string(),
            expires_at: None,
            metadata: request.metadata,
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(provider_capability(
            ProviderKind::Credential,
            "fake-credential-provider",
            "fake",
            vec!["resolve".to_string()],
            true,
        ))
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
                "authorized.scope_satisfied".to_string()
            } else {
                "denied.scope_missing".to_string()
            },
            redactions: Vec::new(),
            warnings: Vec::new(),
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(provider_capability(
            ProviderKind::Security,
            "scope-security-policy",
            "scope",
            vec!["source_authorization".to_string()],
            false,
        ))
    }
}

fn provider_capability(
    provider_kind: ProviderKind,
    provider_id: &str,
    implementation: &str,
    features: Vec<String>,
    fake_overrides_supported: bool,
) -> ProviderCapability {
    ProviderCapability {
        provider_id: ProviderId::new(provider_id),
        provider_kind,
        implementation: implementation.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        health: HealthStatus::Healthy,
        limits: ProviderLimits::default(),
        features,
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
        fake_overrides_supported,
        embedding: None,
        llm: None,
        vector_store: None,
        fetch: None,
        render: None,
        credential: if provider_kind == ProviderKind::Credential {
            Some(CredentialProviderCapability {
                auth_schemes: vec!["api_key".to_string(), "bearer".to_string()],
                redaction_policy: RedactionPolicy::Strict,
            })
        } else {
            None
        },
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
