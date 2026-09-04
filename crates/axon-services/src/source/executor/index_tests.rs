use super::*;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::FakeJobWatchStore;
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::FakeVectorStore;
use std::sync::Arc;

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

fn counts() -> IndexCounts {
    IndexCounts {
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
