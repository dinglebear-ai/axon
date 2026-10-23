use super::*;
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use serde_json::json;

fn source() -> SourceSummary {
    SourceSummary {
        source_id: SourceId::new("source"),
        canonical_uri: "file:///fixture".into(),
        display_name: "fixture".into(),
        source_kind: SourceKind::Local,
        adapter: AdapterRef {
            name: "local".into(),
            version: "test".into(),
        },
        authority: AuthorityLevel::UserPinned,
        status: LifecycleStatus::Running,
        counts: SourceCounts {
            documents_skipped: 0,
            items_total: 0,
            items_changed: 0,
            documents_total: 0,
            chunks_total: 0,
            vector_points_total: 0,
            bytes_total: 0,
        },
        created_at: Timestamp("2026-10-16T00:00:00Z".into()),
        updated_at: Timestamp("2026-10-16T00:00:00Z".into()),
        watch_id: None,
        graph_node_ids: vec![],
        last_job_id: None,
        last_refreshed_at: None,
        tags: vec![],
        user_label: None,
    }
}
async fn publish(ledger: &FakeLedgerStore) -> SourceGenerationId {
    let mut generation = ledger
        .create_generation(SourceId::new("source"))
        .await
        .unwrap();
    ledger
        .put_manifest(SourceManifest {
            source_id: generation.source_id.clone(),
            generation: generation.generation.clone(),
            adapter: source().adapter,
            scope: SourceScope::Repo,
            items: vec![],
            created_at: source().created_at,
            metadata: MetadataMap::new(),
        })
        .await
        .unwrap();
    generation.status = LifecycleStatus::Completed;
    let generation = ledger.complete_generation(generation).await.unwrap();
    ledger
        .publish_generation(PublishGenerationRequest {
            job_id: JobId::new(uuid::Uuid::new_v4()),
            attempt: 1,
            source_id: generation.source_id.clone(),
            generation: generation.generation.clone(),
            expected_previous_generation: generation.previous_generation.clone(),
            retained_statuses: vec![],
        })
        .await
        .unwrap()
        .generation
}
async fn ledger_at_two() -> FakeLedgerStore {
    let ledger = FakeLedgerStore::new();
    ledger.upsert_source(source()).await.unwrap();
    publish(&ledger).await;
    assert_eq!(
        generation_payload_i64(&publish(&ledger).await, "committed_generation").unwrap(),
        2
    );
    ledger
}
fn request() -> VectorSearchRequest {
    VectorSearchRequest {
        collection: "fixture".into(),
        query: "answer".into(),
        limit: 10,
        dense_vector: None,
        sparse_vector: None,
        filters: MetadataMap::new(),
        hybrid: None,
        generation: None,
        graph_refs: vec![],
        metadata: MetadataMap::new(),
    }
}
fn hit(id: &str, source: &str, born: i64, committed: serde_json::Value) -> VectorSearchMatch {
    let payload: MetadataMap = serde_json::from_value(json!({
        "source_id": source, "born_epoch": born, "committed_generation": committed,
        "retired_epoch": null, "document_status": "published"
    }))
    .unwrap();
    VectorSearchMatch {
        point_id: VectorPointId::new(id),
        score: 0.75,
        chunk_id: Some(ChunkId::new(id)),
        document_id: Some(DocumentId::new(id)),
        source_id: Some(SourceId::new(source)),
        source_item_key: Some(SourceItemKey::new(id)),
        text: Some(id.into()),
        payload,
    }
}
fn result(results: Vec<VectorSearchMatch>) -> VectorSearchResult {
    VectorSearchResult {
        collection: "fixture".into(),
        results,
        limit: 10,
        next_cursor: None,
        warnings: vec![],
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn excludes_deleted_updated_old_and_future_points_but_preserves_carried_current() {
    let ledger = ledger_at_two().await;
    // Provider still exposes old unretired points, exactly the state left by a
    // failed retire_generation call after epoch 2 ledger publication succeeds.
    let carried = hit("unchanged-current", "source", 1, json!(2));
    let updated = hit("updated-current", "source", 2, json!(2));
    let output = reconcile(
        &ledger,
        &request(),
        result(vec![
            hit("deleted-old", "source", 1, json!(1)),
            hit("updated-old", "source", 1, json!(1)),
            carried.clone(),
            updated.clone(),
            hit("future", "source", 3, json!(3)),
        ]),
    )
    .await
    .unwrap();
    assert_eq!(output.results, vec![carried, updated]);
    assert_eq!(
        output.metadata.get("publication_fence_rejected"),
        Some(&json!(3))
    );
}

#[tokio::test]
async fn rejects_unknown_ledger_sources_and_null_or_malformed_commit_epochs() {
    let ledger = ledger_at_two().await;
    let output = reconcile(
        &ledger,
        &request(),
        result(vec![
            hit("unknown", "not-registered", 1, json!(1)),
            hit("staged", "source", 2, json!(null)),
            hit("malformed", "source", 2, json!("2")),
        ]),
    )
    .await
    .unwrap();
    assert!(output.results.is_empty());
    assert_eq!(
        output.metadata.get("publication_fence_rejected"),
        Some(&json!(3))
    );
}

#[tokio::test]
async fn ledger_failure_returns_typed_error_instead_of_provider_results() {
    let ledger = FakeLedgerStore::new().with_committed_generation_failure();
    let error = reconcile(
        &ledger,
        &request(),
        result(vec![hit("possibly-current", "source", 1, json!(1))]),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.code.to_string(),
        "retrieval.publication_fence_unavailable"
    );
}

#[tokio::test]
async fn mismatched_payload_and_match_identity_fails_closed() {
    let ledger = ledger_at_two().await;
    let mut candidate = hit("mismatch", "source", 2, json!(2));
    candidate.source_id = Some(SourceId::new("different-source"));
    let error = reconcile(&ledger, &request(), result(vec![candidate]))
        .await
        .unwrap_err();
    assert_eq!(
        error.code.to_string(),
        "retrieval.invalid_publication_identity"
    );
}

#[tokio::test]
async fn staged_future_generation_does_not_replace_the_last_published_epoch() {
    let ledger = ledger_at_two().await;
    let pending = ledger
        .create_generation(SourceId::new("source"))
        .await
        .unwrap();
    assert_eq!(
        generation_payload_i64(&pending.generation, "committed_generation").unwrap(),
        3
    );
    let current = hit("current", "source", 2, json!(2));
    let output = reconcile(
        &ledger,
        &request(),
        result(vec![
            hit("provider-committed-before-ledger", "source", 3, json!(3)),
            current.clone(),
        ]),
    )
    .await
    .unwrap();
    assert_eq!(output.results, vec![current]);
}
