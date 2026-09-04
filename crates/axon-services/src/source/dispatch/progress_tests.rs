use std::sync::Arc;

use axon_adapters::local::LocalSourceAdapter;
use axon_api::source::{
    AuthSnapshot, LifecycleStatus, PipelinePhase, ProviderId, SourceProgressEvent, SourceRequest,
    StageCounts,
};
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use axon_ledger::store::FakeLedgerStore;
use axon_vectors::store::FakeVectorStore;

use super::*;
use crate::context::TargetLocalSourceRuntime;
use crate::source::execution::SourceExecutionContext;

const FIXTURE_FILES: usize = 130;

fn progress_runtime(
    jobs: Arc<FakeJobWatchStore>,
    vectors: Arc<FakeVectorStore>,
    ledger: Arc<FakeLedgerStore>,
) -> TargetLocalSourceRuntime {
    TargetLocalSourceRuntime::new(
        jobs,
        ledger,
        Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8)),
        vectors,
        ProviderId::new("fake-embedding"),
        "fake-embedding",
        8,
    )
}

fn write_large_local_fixture(root: &std::path::Path) {
    let body = (0..700)
        .map(|index| {
            format!(
                "Paragraph {index}: deterministic source progress content with enough words to force bounded chunk windows.\n\n"
            )
        })
        .collect::<String>();
    for index in 0..FIXTURE_FILES {
        std::fs::write(root.join(format!("doc-{index:03}.md")), &body).unwrap();
    }
}

fn assert_monotonic(counts: &[StageCounts], phase: PipelinePhase) {
    for pair in counts.windows(2) {
        assert!(
            pair[0].items_done <= pair[1].items_done,
            "{phase:?} item progress regressed"
        );
        assert!(
            pair[0].documents_done <= pair[1].documents_done,
            "{phase:?} document progress regressed"
        );
        assert!(
            pair[0].chunks_done <= pair[1].chunks_done,
            "{phase:?} chunk progress regressed"
        );
    }
}

#[tokio::test]
async fn local_source_exposes_durable_progress_across_multiple_acquisition_and_chunk_batches() {
    let source = crate::test_support::visible_tempdir().unwrap();
    write_large_local_fixture(source.path());
    let jobs = Arc::new(FakeJobWatchStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ledger = Arc::new(FakeLedgerStore::new());
    let runtime = progress_runtime(jobs.clone(), vectors, ledger);
    let request = SourceRequest::local_path(source.path().to_string_lossy(), true);
    let routed = crate::source::routing::resolve_source_route(&request).unwrap();
    let execution = SourceExecutionContext::inline(request.clone(), None);
    let auth = AuthSnapshot::trusted_cli("progress-test");

    let result = dispatch_local(
        Arc::new(LocalSourceAdapter::new()),
        &axon_core::config::Config::default(),
        &runtime,
        &request.source,
        "progress-test",
        "progress-owner",
        Some(&auth),
        true,
        &routed.route,
        &execution,
    )
    .await
    .expect("multi-batch local source should complete");

    let summary = jobs
        .get(result.job_id)
        .await
        .unwrap()
        .expect("durable job summary");
    assert_eq!(summary.status, LifecycleStatus::Running);
    let updates = jobs.recorded_status_updates(result.job_id).await;
    let phase_counts = |phase| {
        updates
            .iter()
            .filter(|update| update.phase == phase)
            .filter_map(|update| update.counts.clone())
            .collect::<Vec<_>>()
    };

    let discovering = phase_counts(PipelinePhase::Discovering);
    assert_eq!(
        discovering.last().and_then(|counts| counts.items_total),
        Some(130)
    );
    assert_eq!(
        discovering.last().map(|counts| counts.items_done),
        Some(130)
    );
    let diffing = phase_counts(PipelinePhase::Diffing);
    assert_eq!(diffing.first().map(|counts| counts.items_done), Some(0));
    assert_eq!(diffing.last().map(|counts| counts.items_done), Some(130));
    assert_monotonic(&diffing, PipelinePhase::Diffing);

    let fetching = phase_counts(PipelinePhase::Fetching);
    assert_eq!(
        fetching.first().and_then(|counts| counts.items_total),
        Some(130)
    );
    assert_eq!(fetching.first().map(|counts| counts.items_done), Some(0));
    assert!(fetching.iter().any(|counts| counts.items_done == 64));
    assert!(fetching.iter().any(|counts| counts.items_done == 128));
    assert_eq!(fetching.last().map(|counts| counts.items_done), Some(130));
    assert_eq!(
        fetching.last().map(|counts| counts.documents_done),
        Some(130)
    );
    assert_monotonic(&fetching, PipelinePhase::Fetching);

    for phase in [
        PipelinePhase::Discovering,
        PipelinePhase::Diffing,
        PipelinePhase::Enriching,
        PipelinePhase::Normalizing,
        PipelinePhase::Preparing,
        PipelinePhase::Batching,
        PipelinePhase::Embedding,
        PipelinePhase::Vectorizing,
        PipelinePhase::Upserting,
        PipelinePhase::Publishing,
    ] {
        assert!(
            updates.iter().any(|update| update.phase == phase),
            "missing durable {phase:?} progress"
        );
    }

    let embedding = phase_counts(PipelinePhase::Embedding);
    assert!(
        embedding.len() > 2,
        "expected multiple embedding checkpoints"
    );
    assert_monotonic(&embedding, PipelinePhase::Embedding);
    let embedded = embedding.last().unwrap();
    assert!(embedded.chunks_total.unwrap_or(0) > 512);
    assert_eq!(embedded.chunks_done, embedded.chunks_total.unwrap());

    let upserting = phase_counts(PipelinePhase::Upserting);
    assert_monotonic(&upserting, PipelinePhase::Upserting);
    let upserted = upserting.last().unwrap();
    assert_eq!(upserted.chunks_done, upserted.chunks_total.unwrap());

    let heartbeats = jobs.recorded_heartbeats(result.job_id).await;
    for phase in [PipelinePhase::Embedding, PipelinePhase::Upserting] {
        assert!(
            heartbeats
                .iter()
                .any(|heartbeat| heartbeat.phase == phase && heartbeat.counts.is_some()),
            "{phase:?} reservation heartbeat must carry progress counts"
        );
    }

    let progress_events = jobs
        .recorded_events(result.job_id)
        .await
        .into_iter()
        .filter_map(|event| event.details.get("source_progress_event").cloned())
        .map(serde_json::from_value::<SourceProgressEvent>)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for phase in [PipelinePhase::Embedding, PipelinePhase::Upserting] {
        let event = progress_events
            .iter()
            .find(|event| event.phase == phase && event.status == LifecycleStatus::Running)
            .expect("running source progress event");
        assert!(updates.iter().any(|update| {
            update.phase == phase && update.counts.as_ref() == Some(&event.counts)
        }));
    }
}

#[tokio::test]
async fn embed_false_skips_vector_phases_without_stale_fetching_counts() {
    let source = crate::test_support::visible_tempdir().unwrap();
    std::fs::write(
        source.path().join("doc.md"),
        "# Progress\n\nNo embeddings requested.\n",
    )
    .unwrap();
    let jobs = Arc::new(FakeJobWatchStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let ledger = Arc::new(FakeLedgerStore::new());
    let runtime = progress_runtime(jobs.clone(), vectors.clone(), ledger);
    let request = SourceRequest::local_path(source.path().to_string_lossy(), false);
    let routed = crate::source::routing::resolve_source_route(&request).unwrap();
    let execution = SourceExecutionContext::inline(request.clone(), None);
    let auth = AuthSnapshot::trusted_cli("progress-test");

    let result = dispatch_local(
        Arc::new(LocalSourceAdapter::new()),
        &axon_core::config::Config::default(),
        &runtime,
        &request.source,
        "progress-no-embed",
        "progress-owner",
        Some(&auth),
        false,
        &routed.route,
        &execution,
    )
    .await
    .expect("embed=false local source should complete");

    let updates = jobs.recorded_status_updates(result.job_id).await;
    let phases = updates
        .iter()
        .map(|update| update.phase)
        .collect::<Vec<_>>();
    assert!(phases.contains(&PipelinePhase::Preparing));
    assert!(phases.contains(&PipelinePhase::Publishing));
    assert!(
        !phases.contains(&PipelinePhase::Complete),
        "adapter pipeline must defer terminal status until graph and cleanup finish"
    );
    for skipped in [
        PipelinePhase::Batching,
        PipelinePhase::Embedding,
        PipelinePhase::Vectorizing,
        PipelinePhase::Upserting,
    ] {
        assert!(
            !phases.contains(&skipped),
            "embed=false emitted {skipped:?}"
        );
    }
    let preparing = updates
        .iter()
        .filter(|update| update.phase == PipelinePhase::Preparing)
        .filter_map(|update| update.counts.as_ref())
        .collect::<Vec<_>>();
    let preparing_start = preparing.first().expect("preparing start counts");
    assert_eq!(preparing_start.documents_total, Some(1));
    assert_eq!(preparing_start.documents_done, 0);
    let preparing_done = preparing.last().expect("preparing completion counts");
    assert_eq!(preparing_done.documents_total, Some(1));
    assert_eq!(preparing_done.documents_done, 1);
    assert!(preparing_done.chunks_done > 0);
    assert!(vectors.points("progress-no-embed").await.is_empty());
}
