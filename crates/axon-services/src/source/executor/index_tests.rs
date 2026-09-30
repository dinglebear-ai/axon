use super::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::point::VectorPointBatchBuilder;
use axon_vectors::store::FakeVectorStore;
use axon_vectors::store::VectorStore;
use axon_vectors::testing::{
    test_collection_spec_hybrid, test_embedding_result_for, test_prepared_document,
    test_vector_build_context,
};
use std::sync::Arc;

#[test]
fn informational_notice_does_not_degrade_completion() {
    let notice = SourceWarning {
        code: "document.content.pre_chunk_redacted".to_string(),
        severity: Severity::Info,
        message: "content was scrubbed".to_string(),
        source_item_key: None,
        retryable: false,
    };
    assert_eq!(successful_status(&[notice]), LifecycleStatus::Completed);
}

fn runtime() -> TargetLocalSourceRuntime {
    TargetLocalSourceRuntime::new(
        Arc::new(FakeJobWatchStore::new()),
        Arc::new(FakeLedgerStore::new()),
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
        Arc::new(FakeVectorStore::new("fake-vector")),
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        8,
    )
}

#[tokio::test]
async fn imported_vectors_advance_the_ledger_generation_before_writing() {
    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let document = test_prepared_document();
    let embeddings = test_embedding_result_for(&document, "text-embedding-test", 8);
    let mut batch = VectorPointBatchBuilder::new(
        test_collection_spec_hybrid(8),
        document,
        embeddings,
        test_vector_build_context(),
    )
    .build()
    .unwrap();
    for point in &mut batch.points {
        point
            .payload
            .insert("source_generation".into(), serde_json::json!(1));
        point
            .payload
            .insert("committed_generation".into(), serde_json::json!(1));
    }
    vectors
        .ensure_collection(test_collection_spec_hybrid(8))
        .await
        .unwrap();
    vectors.upsert(batch).await.unwrap();
    let runtime = TargetLocalSourceRuntime::new(
        Arc::new(FakeJobWatchStore::new()),
        ledger.clone(),
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
        vectors.clone(),
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        8,
    );
    let route = crate::source::routing::resolve_source_route(&SourceRequest::new(
        "https://example.com/docs",
    ))
    .unwrap()
    .route;
    let plan = crate::source::dispatch::family_source_plan(
        &route.source.canonical_uri,
        &route,
        true,
        None,
        None,
    );
    let execution = SourceExecutionContext::inline(plan.request.clone(), None);
    let adapter = axon_adapters::FakeSourceAdapter::new(route.adapter.clone());
    let input = SourcePipelineInput {
        adapter: &adapter,
        plan,
        collection: "axon-test",
        owner_id: "test",
        auth_snapshot: None,
        execution: &execution,
    };
    let source_id = SourceId::new("src-web");
    let mut summary = metadata::source_summary(
        &input,
        LifecycleStatus::Running,
        empty_source_counts(),
        None,
    );
    summary.source_id = source_id.clone();
    ledger.upsert_source(summary).await.unwrap();
    let (generation, occupied) = reserve_unoccupied_generation(&runtime, &input, &source_id)
        .await
        .unwrap();
    assert_eq!(generation.generation.0, "gen_2");
    assert_eq!(occupied, vec![SourceGenerationId::new("gen_1")]);
    assert_eq!(ledger.generation_count().await, 2);
    assert_eq!(vectors.points("axon-test").await.len(), 2);
}

fn counts() -> IndexCounts {
    IndexCounts {
        documents_skipped: 0,
        job_id: JobId::new(uuid::Uuid::new_v4()),
        source_id: SourceId::new("source-release-debt"),
        generation: SourceGenerationId::new("generation-release-debt"),
        items_discovered: 2,
        documents_prepared: 2,
        chunks_prepared: 3,
        vector_points_written: 3,
        removed: 0,
        published_manifest: None,
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        artifacts: Vec::new(),
        inline: None,
    }
}

#[tokio::test]
async fn persisted_adapter_release_debt_degrades_success_without_losing_counts() {
    let expected = counts();
    let warning = deferred_warning(
        "source.adapter.release_deferred",
        "adapter release was persisted as cleanup debt".to_string(),
    );

    let actual = merge_pipeline_results(
        &runtime(),
        Ok(expected.clone()),
        Ok(()),
        Ok(AdapterReleaseOutcome::Deferred(warning)),
    )
    .await
    .expect("persisted cleanup debt must remain a successful source result");

    assert_eq!(actual.documents_prepared, expected.documents_prepared);
    assert_eq!(actual.chunks_prepared, expected.chunks_prepared);
    assert_eq!(actual.vector_points_written, expected.vector_points_written);
    assert_eq!(
        successful_status(&actual.warnings),
        LifecycleStatus::CompletedDegraded
    );
    assert_eq!(actual.warnings.len(), 1);
    assert_eq!(actual.warnings[0].code, "source.adapter.release_deferred");
}

#[tokio::test]
async fn adapter_release_and_debt_persistence_failure_remains_an_error() {
    let error = anyhow::anyhow!("adapter release failed and debt was not persisted");

    let result = merge_pipeline_results(&runtime(), Ok(counts()), Ok(()), Err(error)).await;

    assert!(
        result.is_err(),
        "untracked cleanup work must fail the pipeline"
    );
}

#[tokio::test]
async fn status_and_release_failure_preserves_typed_status_error() {
    let status = ApiError::new(
        "job.status_write_failed",
        ErrorStage::Publishing,
        "status unavailable",
    )
    .with_source_item_key("src/file.rs");
    let error = merge_pipeline_results(
        &runtime(),
        Ok(counts()),
        Err(anyhow::Error::new(status.clone())),
        Err(anyhow::anyhow!("adapter release failure")),
    )
    .await
    .unwrap_err();
    assert_eq!(error.downcast_ref::<ApiError>(), Some(&status));
    assert!(format!("{error:#}").contains("adapter release failure"));
}
