use axon_api::source::*;

use crate::AXON_READ_SCOPE;
use crate::policy::{
    CredentialProvider, FakeCredentialProvider, ScopeSecurityPolicy, SecurityPolicy,
};

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

#[tokio::test]
async fn fake_credential_provider_resolves_redacted_material() {
    let provider = FakeCredentialProvider::new();
    let material = provider
        .resolve(CredentialRequest {
            credential_kind: CredentialKind::ApiKey,
            secret_ref: SecretRef {
                provider: "env".to_string(),
                key: "TOKEN".to_string(),
                label: "token".to_string(),
            },
            scope: Some("github".to_string()),
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap();
    assert_eq!(material.redacted_value, "redacted");
    assert_eq!(material.credential_kind, CredentialKind::ApiKey);

    let capability = CredentialProvider::capabilities(&provider).await.unwrap();
    assert_eq!(capability.provider_kind, ProviderKind::Credential);
}
