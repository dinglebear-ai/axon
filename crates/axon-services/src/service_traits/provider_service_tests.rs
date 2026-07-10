use std::sync::Arc;

use super::*;

#[tokio::test]
async fn fake_provider_service_providers_and_provider() {
    let fake = FakeProviderService::new();
    let providers = fake.providers().await.expect("providers should succeed");
    assert_eq!(providers.len(), 1);

    let capability = fake
        .provider(ProviderId::new("fake-provider"))
        .await
        .expect("provider should succeed");
    assert_eq!(capability.provider_id.0, "fake-provider");
}

#[tokio::test]
async fn fake_provider_service_doctor_reports_ok() {
    let fake = FakeProviderService::new();
    let result = fake.doctor().await.expect("doctor should succeed");
    assert_eq!(result.payload["status"], "ok");
}

#[tokio::test]
async fn fake_provider_service_capabilities_and_health_through_trait_object() {
    let fake: Arc<dyn ProviderService> = Arc::new(FakeProviderService::new());

    let capabilities = fake
        .capabilities()
        .await
        .expect("capabilities should succeed");
    assert_eq!(capabilities.server.name, "axon");

    let health = fake.health().await.expect("health should succeed");
    assert_eq!(health.providers.len(), 1);
}

#[tokio::test]
async fn fake_provider_service_unknown_provider_errors() {
    let fake = FakeProviderService::new();
    let result = fake.provider(ProviderId::new("does-not-exist")).await;
    assert!(result.is_err());
}

/// Compile-level check: `ProviderServiceImpl` satisfies the trait so it can
/// be constructed as a trait object; `doctor` is the only method with real
/// production orchestration (wraps `crate::system::doctor`), verified here
/// only at the type level — a live `ServiceContext` doctor run is out of
/// scope for this fake-backed sidecar.
#[allow(dead_code)]
fn _provider_service_impl_is_object_safe(ctx: Arc<crate::context::ServiceContext>) {
    let _svc: Arc<dyn ProviderService> = Arc::new(ProviderServiceImpl::new(ctx));
}
