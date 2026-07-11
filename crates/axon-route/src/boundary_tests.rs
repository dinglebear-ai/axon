use std::sync::Arc;

use axon_api::source::*;
use axon_error::{ApiError, ErrorStage};

use super::*;
use crate::capability::AdapterRegistry;

fn sample_request() -> SourceRequest {
    SourceRequest::new("example.com")
}

fn sample_resolved_source() -> ResolvedSource {
    let resolver = crate::resolver::SourceResolver::new(
        crate::authority::InMemoryAuthorityRegistry::from_records(Vec::new()),
        AdapterRegistry::target_defaults(),
    );
    resolver
        .resolve(&sample_request())
        .expect("sample source resolves")
}

fn sample_route_plan() -> RoutePlan {
    let router = crate::router::SourceRouter::new(AdapterRegistry::target_defaults());
    router
        .route(&sample_request(), sample_resolved_source())
        .expect("sample source routes")
}

fn sample_error() -> ApiError {
    ApiError::new(
        "test.failure",
        ErrorStage::Resolving,
        "boundary test failure",
    )
}

// --- Fake coverage -------------------------------------------------------

#[tokio::test]
async fn fake_source_resolver_success_records_calls_and_returns_source() {
    let source = sample_resolved_source();
    let fake = FakeSourceResolver::new(FakeSourceRouteMode::Success(source.clone()));

    let request = sample_request();
    let resolved = SourceResolver::resolve(&fake, &request).await.unwrap();

    assert_eq!(resolved.canonical_uri, source.canonical_uri);
    assert_eq!(fake.calls().await, vec![request]);

    let capability = SourceResolver::capabilities(&fake).await.unwrap();
    assert_eq!(capability.0.health, HealthStatus::Healthy);
}

#[tokio::test]
async fn fake_source_resolver_failure_mode_returns_error() {
    let fake = FakeSourceResolver::new(FakeSourceRouteMode::Failure(sample_error()));

    let err = SourceResolver::resolve(&fake, &sample_request())
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "test.failure");

    let capability = SourceResolver::capabilities(&fake).await.unwrap();
    assert_eq!(capability.0.health, HealthStatus::Unavailable);
}

#[tokio::test]
async fn fake_source_resolver_degraded_mode_and_capability_override() {
    let source = sample_resolved_source();
    let fake = FakeSourceResolver::new(FakeSourceRouteMode::Degraded(source.clone()))
        .with_capability_override(SourceResolverCapability::from(CapabilityBase {
            name: "overridden".to_string(),
            version: "0.0.0".to_string(),
            owner_crate: "axon-route".to_string(),
            health: HealthStatus::Cooling,
            features: Vec::new(),
            limits: MetadataMap::new(),
        }));

    let resolved = SourceResolver::resolve(&fake, &sample_request())
        .await
        .unwrap();
    assert_eq!(resolved.canonical_uri, source.canonical_uri);

    let capability = SourceResolver::capabilities(&fake).await.unwrap();
    assert_eq!(capability.0.name, "overridden");
    assert_eq!(capability.0.health, HealthStatus::Cooling);
}

#[tokio::test]
async fn fake_source_router_success_records_calls_and_returns_plan() {
    let plan = sample_route_plan();
    let fake = FakeSourceRouter::new(FakeSourceRouteMode::Success(plan.clone()));

    let source = sample_resolved_source();
    let request = sample_request();
    let routed = SourceRouter::route(&fake, source.clone(), &request)
        .await
        .unwrap();
    assert_eq!(routed.adapter.name, plan.adapter.name);
    assert_eq!(fake.calls().await, vec![(source, request)]);

    let validated = SourceRouter::validate_options(&fake, &plan).await.unwrap();
    assert_eq!(validated.values, plan.validated_options.values);

    let capability = SourceRouter::capabilities(&fake).await.unwrap();
    assert_eq!(capability.0.health, HealthStatus::Healthy);
}

#[tokio::test]
async fn fake_source_router_failure_mode_returns_error_for_route_and_validate() {
    let fake = FakeSourceRouter::new(FakeSourceRouteMode::Failure(sample_error()));

    let err = SourceRouter::route(&fake, sample_resolved_source(), &sample_request())
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "test.failure");

    let err = SourceRouter::validate_options(&fake, &sample_route_plan())
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "test.failure");

    let capability = SourceRouter::capabilities(&fake).await.unwrap();
    assert_eq!(capability.0.health, HealthStatus::Unavailable);
}

// --- Concrete-struct trait satisfaction -----------------------------------

#[tokio::test]
async fn concrete_source_resolver_satisfies_boundary_trait_via_dyn() {
    let resolver: Arc<dyn SourceResolver> = Arc::new(crate::resolver::SourceResolver::new(
        crate::authority::InMemoryAuthorityRegistry::from_records(Vec::new()),
        AdapterRegistry::target_defaults(),
    ));

    let resolved = resolver.resolve(&sample_request()).await.unwrap();
    assert_eq!(resolved.source_kind, SourceKind::Web);

    let capability = resolver.capabilities().await.unwrap();
    assert_eq!(capability.0.owner_crate, "axon-route");
    assert_eq!(capability.0.health, HealthStatus::Healthy);
}

#[tokio::test]
async fn concrete_source_router_satisfies_boundary_trait_via_dyn() {
    let router: Arc<dyn SourceRouter> = Arc::new(crate::router::SourceRouter::new(
        AdapterRegistry::target_defaults(),
    ));

    let source = sample_resolved_source();
    let request = sample_request();
    let plan = router.route(source.clone(), &request).await.unwrap();
    assert_eq!(plan.adapter.name, source.adapter.name);

    let validated = router.validate_options(&plan).await.unwrap();
    assert_eq!(validated.values, plan.validated_options.values);

    let capability = router.capabilities().await.unwrap();
    assert_eq!(capability.0.owner_crate, "axon-route");
}
