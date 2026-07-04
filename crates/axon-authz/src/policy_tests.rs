use axon_api::source::*;

use super::*;

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
    assert_eq!(allowed.reason, "authorized.scope_satisfied");

    let capability = policy.capabilities().await.unwrap();
    assert_eq!(capability.provider_kind, ProviderKind::Security);
}

#[tokio::test]
async fn scope_security_policy_accepts_combined_full_access_scope() {
    let policy = ScopeSecurityPolicy::new(AXON_READ_SCOPE);
    let allowed = policy
        .authorize_source(SecurityPolicyRequest {
            caller: CallerContext {
                actor: Some("tester".to_string()),
                transport: TransportKind::Cli,
                scopes: vec![AXON_FULL_ACCESS_SCOPE.to_string()],
                visibility_ceiling: Visibility::Internal,
            },
            safety_class: SafetyClass::LocalFilesystem,
            target: "file:///repo".to_string(),
        })
        .await
        .unwrap();
    assert!(allowed.allowed);
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

#[tokio::test]
async fn fake_credential_provider_reports_health_override() {
    let provider = FakeCredentialProvider::new().with_health(HealthStatus::Cooling);

    let capability = CredentialProvider::capabilities(&provider).await.unwrap();

    assert_eq!(capability.health, HealthStatus::Cooling);
}

#[tokio::test]
async fn fake_credential_provider_capabilities_reflect_failure_mode() {
    let timeout = FakeCredentialProvider::new().with_mode(FakeCredentialMode::Timeout);
    assert_eq!(
        CredentialProvider::capabilities(&timeout)
            .await
            .unwrap()
            .health,
        HealthStatus::Degraded
    );

    let rate_limited = FakeCredentialProvider::new().with_mode(FakeCredentialMode::RateLimited);
    let capability = CredentialProvider::capabilities(&rate_limited)
        .await
        .unwrap();
    assert_eq!(capability.health, HealthStatus::Cooling);
    assert!(capability.cooldown_until.is_some());
    assert_eq!(
        capability.last_error.unwrap().code.to_string(),
        "provider.rate_limited"
    );

    let provider = FakeCredentialProvider::new().with_mode(FakeCredentialMode::Fatal);

    let capability = CredentialProvider::capabilities(&provider).await.unwrap();

    assert_eq!(capability.health, HealthStatus::Unavailable);
    let error = capability.last_error.unwrap();
    assert_eq!(error.code.to_string(), "provider.fatal");
    assert_eq!(
        error.provider_id,
        Some("fake-credential-provider".to_string())
    );
    assert!(!error.retryable);
}

#[tokio::test]
async fn fake_credential_provider_returns_failure_modes_and_records_calls() {
    let provider = FakeCredentialProvider::new().with_mode(FakeCredentialMode::Fatal);

    let err = provider
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
        .unwrap_err();

    assert_eq!(err.code.to_string(), "provider.fatal");
    assert!(!err.retryable);
    assert_eq!(provider.calls().await, vec!["credential.resolve"]);
}
