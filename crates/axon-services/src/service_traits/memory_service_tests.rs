use std::sync::Arc;

use super::*;

/// Compile-level assertion: `MemoryServiceImpl` satisfies the trait and can
/// be built into a trait object. Not exercised live (no real service deps in
/// this test), but if the signatures drift this fails to compile.
#[allow(dead_code)]
fn assert_impl_is_memory_service(ctx: Arc<ServiceContext>) -> Arc<dyn MemoryService> {
    Arc::new(MemoryServiceImpl::new(ctx))
}

fn sample_request(body: &str) -> MemoryRequest {
    MemoryRequest {
        memory_type: axon_api::source::MemoryType::Fact,
        body: body.to_string(),
        confidence: 0.9,
        salience: 0.5,
        scope: axon_api::source::MemoryScope {
            kind: "project".to_string(),
            value: "axon".to_string(),
        },
        title: None,
        tags: Vec::new(),
        links: Vec::new(),
        decay: None,
        embed: false,
        visibility: None,
    }
}

#[tokio::test]
async fn fake_memory_service_remember_then_get() {
    let fake = FakeMemoryService::new();
    let result = fake
        .remember(sample_request("axon uses qdrant"))
        .await
        .expect("remember should succeed");
    let record = fake
        .get(result.memory_id.clone())
        .await
        .expect("get should find the memory");
    assert_eq!(record.body, "axon uses qdrant");
}

#[tokio::test]
async fn fake_memory_service_search_matches_body() {
    let fake = FakeMemoryService::new();
    fake.remember(sample_request("axon uses qdrant"))
        .await
        .expect("remember should succeed");

    let request = axon_api::source::MemorySearchRequest {
        query: "qdrant".to_string(),
        limit: 10,
        filters: axon_api::source::MetadataMap::new(),
        include_graph: false,
        include_archived: false,
        reinforce: false,
        include_statuses: Vec::new(),
    };
    let result = fake.search(request).await.expect("search should succeed");
    assert_eq!(result.results.len(), 1);
}

#[tokio::test]
async fn fake_memory_service_context_joins_bodies() {
    let fake: Arc<dyn MemoryService> = Arc::new(FakeMemoryService::new());
    fake.remember(sample_request("axon uses qdrant"))
        .await
        .expect("remember should succeed");

    let request = MemoryContextRequest {
        token_budget: 1000,
        query: None,
        source_id: None,
        graph_node_id: None,
        filters: axon_api::source::MetadataMap::new(),
        depth: None,
        include_working: false,
    };
    let result = fake.context(request).await.expect("context should succeed");
    assert!(result.context.contains("axon uses qdrant"));
    assert_eq!(result.memories.len(), 1);
}

#[tokio::test]
async fn fake_memory_service_link_appends_link() {
    let fake: Arc<dyn MemoryService> = Arc::new(FakeMemoryService::new());
    let remembered = fake
        .remember(sample_request("axon uses qdrant"))
        .await
        .expect("remember should succeed");

    let request = MemoryLinkRequest {
        memory_id: remembered.memory_id.clone(),
        link: axon_api::source::MemoryLink {
            link_type: "relates_to".to_string(),
            target: "memory-other".to_string(),
            confidence: 0.8,
            evidence: Vec::new(),
        },
    };
    let result = fake.link(request).await.expect("link should succeed");
    assert_eq!(result.memory_id, remembered.memory_id);

    let record = fake
        .get(remembered.memory_id)
        .await
        .expect("get should find the memory");
    assert_eq!(record.links.len(), 1);
}

#[tokio::test]
async fn fake_memory_service_forget_marks_forgotten() {
    let fake: Arc<dyn MemoryService> = Arc::new(FakeMemoryService::new());
    let remembered = fake
        .remember(sample_request("axon uses qdrant"))
        .await
        .expect("remember should succeed");

    let result = fake
        .forget(remembered.memory_id.clone())
        .await
        .expect("forget should succeed");
    assert_eq!(result.status, axon_api::source::MemoryStatus::Forgotten);

    let record = fake
        .get(remembered.memory_id)
        .await
        .expect("get should find the memory");
    assert_eq!(record.status, axon_api::source::MemoryStatus::Forgotten);
}

#[tokio::test]
async fn fake_memory_service_get_missing_errors() {
    let fake: Arc<dyn MemoryService> = Arc::new(FakeMemoryService::new());
    let result = fake.get(MemoryId::new("memory-missing")).await;
    assert!(result.is_err());
}
