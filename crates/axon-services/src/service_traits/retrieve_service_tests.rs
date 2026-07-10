use super::*;

#[tokio::test]
async fn fake_retrieve_service_returns_seeded_content() {
    let fake = FakeRetrieveService::new();
    fake.seed("https://example.com/a", "hello world");

    let result = fake
        .retrieve(RetrieveRequest {
            url: "https://example.com/a".to_string(),
        })
        .await
        .expect("retrieve should succeed");
    assert_eq!(result.content, "hello world");
    assert_eq!(result.chunk_count, 1);
}

#[tokio::test]
async fn fake_retrieve_service_missing_url_returns_empty_with_warning() {
    let fake = FakeRetrieveService::new();
    let result = fake
        .retrieve(RetrieveRequest {
            url: "https://example.com/missing".to_string(),
        })
        .await
        .expect("retrieve should succeed even for missing url");
    assert_eq!(result.chunk_count, 0);
    assert!(!result.warnings.is_empty());
}

#[tokio::test]
async fn fake_retrieve_service_works_through_trait_object() {
    let concrete = FakeRetrieveService::new();
    concrete.seed("https://example.com/a", "hello world");
    let fake: Arc<dyn RetrieveService> = Arc::new(concrete);

    let result = fake
        .retrieve(RetrieveRequest {
            url: "https://example.com/a".to_string(),
        })
        .await
        .expect("retrieve should succeed");
    assert_eq!(result.content, "hello world");
}

/// Compile-only check: `RetrieveServiceImpl` satisfies `RetrieveService`.
/// Not executed — constructing a real `ServiceContext` needs live services.
fn _assert_retrieve_service_impl<T: RetrieveService>() {}
#[allow(dead_code)]
fn _retrieve_service_impl_satisfies_trait() {
    _assert_retrieve_service_impl::<RetrieveServiceImpl>();
}
