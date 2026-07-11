use super::*;
use axon_api::source::{DocumentId, JobId, ProgressThroughput};

fn counts(done: u64) -> StageCounts {
    let mut counts = crate::event::zero_counts();
    counts.items_done = done;
    counts
}

#[test]
fn into_event_produces_running_status_with_sentinel_sequence() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let event = ProgressUpdate::new("embedding batch")
        .with_counts(counts(5))
        .into_event(job_id, PipelinePhase::Embedding);

    assert_eq!(event.job_id, job_id);
    assert_eq!(event.phase, PipelinePhase::Embedding);
    assert_eq!(event.status, LifecycleStatus::Running);
    assert_eq!(event.severity, Severity::Info);
    assert_eq!(event.sequence, 0, "sentinel; sink stamps the real sequence");
    assert_eq!(event.counts.items_done, 5);
    assert_eq!(event.message, "embedding batch");
}

#[test]
fn with_current_and_throughput_populate_the_event() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let current = ProgressCurrent {
        source_item_key: None,
        document_id: Some(DocumentId::from("doc_1")),
        chunk_id: None,
        adapter: None,
        provider: None,
        message: None,
    };
    let throughput = ProgressThroughput {
        items_per_second: Some(2.0),
        bytes_per_second: None,
        chunks_per_second: None,
    };

    let event = ProgressUpdate::new("tick")
        .with_current(current.clone())
        .with_throughput(throughput.clone())
        .into_event(job_id, PipelinePhase::Fetching);

    assert_eq!(event.current, Some(current));
    assert_eq!(event.throughput, Some(throughput));
}

#[test]
fn with_stage_sets_stage_id() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let stage_id = StageId(uuid::Uuid::new_v4());
    let event = ProgressUpdate::new("tick")
        .with_stage(stage_id)
        .into_event(job_id, PipelinePhase::Fetching);

    assert_eq!(event.stage_id, Some(stage_id));
}

#[test]
fn counts_update_matches_the_builder_equivalent() {
    let job_id = JobId(uuid::Uuid::new_v4());
    let event = counts_update(
        job_id,
        PipelinePhase::Parsing,
        None,
        counts(3),
        "parsing files",
    );

    assert_eq!(event.counts.items_done, 3);
    assert_eq!(event.phase, PipelinePhase::Parsing);
    assert_eq!(event.message, "parsing files");
}
