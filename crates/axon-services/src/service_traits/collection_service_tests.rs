use std::sync::Arc;

use super::*;

#[tokio::test]
async fn fake_collection_service_ensure_then_get() {
    let fake = FakeCollectionService::new();
    let spec = fake_spec("test-collection");
    fake.ensure(spec.clone())
        .await
        .expect("ensure should succeed");

    let got = fake
        .get("test-collection".to_string())
        .await
        .expect("get should find the collection");
    assert_eq!(got.collection, "test-collection");
}

#[tokio::test]
async fn fake_collection_service_list_through_trait_object() {
    let fake: Arc<dyn CollectionService> = Arc::new(FakeCollectionService::new());
    fake.ensure(fake_spec("alpha")).await.expect("ensure alpha");
    fake.ensure(fake_spec("beta")).await.expect("ensure beta");

    let mut names: Vec<String> = fake
        .list()
        .await
        .expect("list should succeed")
        .into_iter()
        .map(|s| s.collection)
        .collect();
    names.sort();
    assert_eq!(names, vec!["alpha".to_string(), "beta".to_string()]);
}

/// Compile-level check: `CollectionServiceImpl` satisfies the trait so it can
/// be constructed as a trait object; production behavior against a live
/// `ServiceContext`/Qdrant is out of scope for this fake-backed sidecar.
#[allow(dead_code)]
fn _collection_service_impl_is_object_safe(ctx: Arc<crate::context::ServiceContext>) {
    let _svc: Arc<dyn CollectionService> = Arc::new(CollectionServiceImpl::new(ctx));
}

#[tokio::test]
async fn fake_collection_service_delete() {
    let fake = FakeCollectionService::new();
    fake.ensure(fake_spec("test-collection"))
        .await
        .expect("ensure should succeed");

    let result = fake
        .delete("test-collection".to_string())
        .await
        .expect("delete should succeed");
    assert!(result.deleted);
    assert!(fake.get("test-collection".to_string()).await.is_err());
}
