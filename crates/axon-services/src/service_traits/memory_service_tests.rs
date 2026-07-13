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

fn ts(value: &str) -> axon_api::source::Timestamp {
    axon_api::source::Timestamp(value.to_string())
}

#[tokio::test]
async fn fake_memory_service_update_edits_fields() {
    let fake: Arc<dyn MemoryService> = Arc::new(FakeMemoryService::new());
    let remembered = fake
        .remember(sample_request("axon uses qdrant"))
        .await
        .expect("remember should succeed");

    fake.update(MemoryUpdateRequest {
        memory_id: remembered.memory_id.clone(),
        body: Some("axon uses qdrant and tei".to_string()),
        title: Some("updated title".to_string()),
        memory_type: None,
        confidence: Some(0.42),
        salience: None,
        scope: None,
        reason: Some("correction".to_string()),
        timestamp: ts("2026-01-01T00:00:00Z"),
    })
    .await
    .expect("update should succeed");

    let record = fake
        .get(remembered.memory_id)
        .await
        .expect("get should find the memory");
    assert_eq!(record.body, "axon uses qdrant and tei");
    assert_eq!(record.title.as_deref(), Some("updated title"));
    assert_eq!(record.confidence, 0.42);
}

#[tokio::test]
async fn fake_memory_service_reinforce_raises_salience_and_count() {
    let fake: Arc<dyn MemoryService> = Arc::new(FakeMemoryService::new());
    let remembered = fake
        .remember(sample_request("axon uses qdrant"))
        .await
        .expect("remember should succeed");

    fake.reinforce(
        remembered.memory_id.clone(),
        axon_api::source::MemoryReinforcement {
            amount: 0.1,
            reason: "used in ask".to_string(),
            timestamp: ts("2026-01-01T00:00:00Z"),
        },
    )
    .await
    .expect("reinforce should succeed");

    let record = fake
        .get(remembered.memory_id)
        .await
        .expect("get should find the memory");
    assert!((record.salience - 0.6).abs() < 1e-6);
}

#[tokio::test]
async fn fake_memory_service_supersede_marks_superseded() {
    let fake: Arc<dyn MemoryService> = Arc::new(FakeMemoryService::new());
    let old = fake
        .remember(sample_request("port is 8080"))
        .await
        .expect("remember should succeed");
    let replacement = fake
        .remember(sample_request("port is 9090"))
        .await
        .expect("remember should succeed");

    fake.supersede(MemorySupersedeRequest {
        memory_id: old.memory_id.clone(),
        replacement_id: replacement.memory_id.clone(),
        reason: None,
        timestamp: ts("2026-01-01T00:00:00Z"),
    })
    .await
    .expect("supersede should succeed");

    let record = fake
        .get(old.memory_id)
        .await
        .expect("get should find the memory");
    assert_eq!(record.status, axon_api::source::MemoryStatus::Superseded);
    assert_eq!(record.superseded_by, Some(replacement.memory_id));
}

#[tokio::test]
async fn fake_memory_service_contradict_marks_both_contradicted() {
    let fake: Arc<dyn MemoryService> = Arc::new(FakeMemoryService::new());
    let a = fake
        .remember(sample_request("port is 8080"))
        .await
        .expect("remember should succeed");
    let b = fake
        .remember(sample_request("port is 9090"))
        .await
        .expect("remember should succeed");

    fake.contradict(MemoryContradictRequest {
        memory_id: a.memory_id.clone(),
        conflicting_id: b.memory_id.clone(),
        reason: Some("port mismatch".to_string()),
        timestamp: ts("2026-01-01T00:00:00Z"),
    })
    .await
    .expect("contradict should succeed");

    let ra = fake
        .get(a.memory_id.clone())
        .await
        .expect("get should find memory a");
    let rb = fake
        .get(b.memory_id.clone())
        .await
        .expect("get should find memory b");
    assert_eq!(ra.status, axon_api::source::MemoryStatus::Contradicted);
    assert_eq!(rb.status, axon_api::source::MemoryStatus::Contradicted);
    assert_eq!(ra.contradicts, Some(b.memory_id));
    assert_eq!(rb.contradicts, Some(a.memory_id));
}

#[tokio::test]
async fn fake_memory_service_pin_sets_decay_pinned_flag() {
    let fake: Arc<dyn MemoryService> = Arc::new(FakeMemoryService::new());
    let remembered = fake
        .remember(sample_request("axon uses qdrant"))
        .await
        .expect("remember should succeed");

    fake.pin(MemoryPinRequest {
        memory_id: remembered.memory_id.clone(),
        pinned: true,
        reason: None,
        timestamp: ts("2026-01-01T00:00:00Z"),
    })
    .await
    .expect("pin should succeed");

    let record = fake
        .get(remembered.memory_id)
        .await
        .expect("get should find the memory");
    assert!(record.decay.expect("decay should be set").pinned);
}

#[tokio::test]
async fn fake_memory_service_archive_marks_archived() {
    let fake: Arc<dyn MemoryService> = Arc::new(FakeMemoryService::new());
    let remembered = fake
        .remember(sample_request("axon uses qdrant"))
        .await
        .expect("remember should succeed");

    fake.archive(MemoryArchiveRequest {
        memory_id: remembered.memory_id.clone(),
        reason: None,
        timestamp: ts("2026-01-01T00:00:00Z"),
    })
    .await
    .expect("archive should succeed");

    let record = fake
        .get(remembered.memory_id)
        .await
        .expect("get should find the memory");
    assert_eq!(record.status, axon_api::source::MemoryStatus::Archived);
}

#[tokio::test]
async fn fake_memory_service_review_returns_contradicted_memories() {
    let fake: Arc<dyn MemoryService> = Arc::new(FakeMemoryService::new());
    let a = fake
        .remember(sample_request("port is 8080"))
        .await
        .expect("remember should succeed");
    let b = fake
        .remember(sample_request("port is 9090"))
        .await
        .expect("remember should succeed");
    // A third memory stays active and must NOT show up in the review queue.
    fake.remember(sample_request("unrelated fact"))
        .await
        .expect("remember should succeed");

    fake.contradict(MemoryContradictRequest {
        memory_id: a.memory_id.clone(),
        conflicting_id: b.memory_id.clone(),
        reason: Some("port mismatch".to_string()),
        timestamp: ts("2026-01-01T00:00:00Z"),
    })
    .await
    .expect("contradict should succeed");

    let result = fake
        .review(MemoryReviewRequest::default())
        .await
        .expect("review should succeed");
    assert_eq!(result.memories.len(), 2);
    assert!(
        result
            .memories
            .iter()
            .all(|m| m.status == axon_api::source::MemoryStatus::Contradicted)
    );
}

#[tokio::test]
async fn fake_memory_service_compact_merges_sources() {
    let fake: Arc<dyn MemoryService> = Arc::new(FakeMemoryService::new());
    let a = fake
        .remember(sample_request("axon uses qdrant"))
        .await
        .expect("remember should succeed");
    let b = fake
        .remember(sample_request("axon uses tei"))
        .await
        .expect("remember should succeed");

    let result = fake
        .compact(MemoryCompactRequest {
            memory_ids: vec![a.memory_id.clone(), b.memory_id.clone()],
            strategy: "concatenate".to_string(),
            result_type: axon_api::source::MemoryType::Fact,
            title: Some("compacted".to_string()),
            scope: axon_api::source::MemoryScope {
                kind: "project".to_string(),
                value: "axon".to_string(),
            },
            archive_sources: true,
            instructions: None,
            timestamp: ts("2026-01-01T00:00:00Z"),
        })
        .await
        .expect("compact should succeed");

    let compacted = fake
        .get(result.memory_id)
        .await
        .expect("get should find the compacted memory");
    assert!(compacted.body.contains("axon uses qdrant"));
    assert!(compacted.body.contains("axon uses tei"));

    let source_a = fake
        .get(a.memory_id)
        .await
        .expect("get should find memory a");
    assert_eq!(source_a.status, axon_api::source::MemoryStatus::Archived);
}
