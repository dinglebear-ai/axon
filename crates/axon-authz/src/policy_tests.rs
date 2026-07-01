use axon_api::source::*;

use crate::AXON_READ_SCOPE;
use crate::policy::{ScopeSecurityPolicy, SecurityPolicy};

#[tokio::test]
async fn scope_security_policy_authorizes_source_requests() {
    let policy = ScopeSecurityPolicy::new(AXON_READ_SCOPE);
    let allowed = policy
        .authorize_source(SecurityPolicyRequest {
            caller: CallerContext {
                actor: Some("tester".to_string()),
                transport: TransportKind::Cli,
                scopes: vec![AXON_READ_SCOPE.to_string()],
                visibility_ceiling: Visibility::Internal,
            },
            safety_class: SafetyClass::LocalFilesystem,
            target: "file:///repo".to_string(),
        })
        .await
        .unwrap();
    assert!(allowed.allowed);

    let capability = policy.capabilities().await.unwrap();
    assert_eq!(capability.provider_kind, ProviderKind::Security);
}
