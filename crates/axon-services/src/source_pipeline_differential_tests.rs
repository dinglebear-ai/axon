//! Cross-family characterization for the production source composition.
//!
//! This is deliberately test-only: observations are derived from the existing
//! public result, durable job stages, and progress events. It does not add a
//! production phase registry or a second pipeline model.

#![allow(unsafe_code)]

use axon_api::source::{
    AuthSnapshot, JobEvent, JobEventListRequest, JobStageSnapshot, LifecycleStatus, PipelinePhase,
    SourceRequest, SourceResult, Visibility,
};

#[derive(Debug)]
struct PipelineObservation {
    request: SourceRequest,
    progress: Vec<JobEvent>,
    durable_stages: Vec<JobStageSnapshot>,
    provider_calls: FakeProviderCalls,
    result: SourceResult,
}

#[derive(Debug, PartialEq, Eq)]
struct FakeProviderCalls {
    embedding_batches: usize,
    vector_operations: usize,
    vector_points: usize,
}

async fn observe(
    request: SourceRequest,
    harness: &crate::test_support::SourceWebJobIdentityHarness,
) -> anyhow::Result<PipelineObservation> {
    let result = crate::source::index_source_with_auth(
        request.clone(),
        harness.ctx(),
        Some(AuthSnapshot::trusted_system("differential-test")),
    )
    .await?;
    let store = harness.ctx().job_store().expect("job store");
    let events = store
        .events(JobEventListRequest {
            job_id: result.job_id,
            after_sequence: None,
            limit: Some(256),
            severity: None,
            visibility: Some(Visibility::Public),
            phase: None,
            since_sequence: None,
            cursor: None,
        })
        .await?
        .events;
    let durable_stages = store.stages(result.job_id).await?;
    let provider_calls = FakeProviderCalls {
        embedding_batches: harness.embedder().calls().await.len(),
        vector_operations: harness.vectors().calls().await.len(),
        vector_points: harness
            .vectors()
            .points(&harness.ctx().cfg().collection)
            .await
            .len(),
    };
    Ok(PipelineObservation {
        request,
        progress: events,
        durable_stages,
        provider_calls,
        result,
    })
}

fn phase_spine(observation: &PipelineObservation) -> Vec<PipelinePhase> {
    let mut phases = Vec::new();
    for phase in observation
        .progress
        .iter()
        .filter(|event| event.status != LifecycleStatus::CompletedDegraded)
        .map(|event| event.phase)
        // Leasing is currently implemented only by the generic non-web
        // runner. Keep it out of this pre-stage-identity characterization so
        // the harness compares the shared acquisition/indexing spine rather
        // than a family-specific lease event.
        .filter(|phase| *phase != PipelinePhase::Leasing)
    {
        if phases.last() != Some(&phase) {
            phases.push(phase);
        }
    }
    phases
}

fn public_observation(observation: &PipelineObservation) -> (Vec<PipelinePhase>, usize, usize) {
    (
        phase_spine(observation),
        observation.durable_stages.len(),
        observation.result.counts.documents_total as usize,
    )
}

fn expected_shared_phase_spine() -> Vec<PipelinePhase> {
    vec![
        PipelinePhase::Discovering,
        PipelinePhase::Diffing,
        PipelinePhase::Fetching,
        PipelinePhase::Enriching,
        PipelinePhase::Normalizing,
        PipelinePhase::Preparing,
        PipelinePhase::Batching,
        PipelinePhase::Embedding,
        PipelinePhase::Vectorizing,
        PipelinePhase::Upserting,
        PipelinePhase::Publishing,
    ]
}

fn assert_one_job(observation: &PipelineObservation) {
    assert_eq!(observation.result.status, LifecycleStatus::Completed);
    assert!(!observation.result.canonical_uri.is_empty());
    assert!(!observation.request.source.is_empty());
}

fn write_session_fixture(home: &std::path::Path) -> String {
    let root = home.join(".claude/projects/-home-differential");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("fixture.jsonl"),
        concat!(
            r#"{"type":"user","cwd":"/home/differential","timestamp":"2026-01-01T00:00:00Z","message":{"content":"session fixture"}}"#,
            "\n",
            r#"{"type":"assistant","timestamp":"2026-01-01T00:00:01Z","message":{"model":"fake","content":[{"type":"text","text":"response"}]}}"#,
            "\n",
        ),
    )
    .unwrap();
    format!("session:claude:{}", root.display())
}

#[tokio::test]
async fn web_and_local_share_the_observable_source_contract() {
    let web = crate::test_support::source_context_with_fake_web()
        .await
        .unwrap();
    let web_request = SourceRequest::new("https://docs.example.test/differential");
    let web_observation = observe(web_request.clone(), &web).await.unwrap();

    let local_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        local_dir.path().join("fixture.md"),
        "# differential fixture\n\nshared source body\n",
    )
    .unwrap();
    let local = crate::test_support::source_context_with_local_sqlite_ledger()
        .await
        .unwrap();
    let local_request = SourceRequest::local_path(local_dir.path().to_string_lossy(), true);
    let local_observation = observe(local_request.clone(), &local).await.unwrap();

    assert_one_job(&web_observation);
    assert_one_job(&local_observation);
    assert_eq!(phase_spine(&web_observation), expected_shared_phase_spine());
    assert_eq!(
        phase_spine(&local_observation),
        expected_shared_phase_spine()
    );
    assert_eq!(
        public_observation(&web_observation).0,
        public_observation(&local_observation).0,
        "web/local phase drift: web={:?} local={:?}",
        phase_spine(&web_observation),
        phase_spine(&local_observation)
    );
    assert_eq!(
        web_observation.result.counts.documents_total,
        local_observation.result.counts.documents_total
    );
    assert_eq!(
        web_observation.provider_calls,
        local_observation.provider_calls
    );
    assert!(
        web_observation
            .progress
            .iter()
            .all(|event| !event.message.contains("shared source body")),
        "progress must not contain document content"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn session_source_joins_the_same_observable_source_contract() {
    let home = tempfile::tempdir().unwrap();
    let previous_home = std::env::var_os("HOME");
    unsafe { std::env::set_var("HOME", home.path()) };
    let request = SourceRequest::new(write_session_fixture(home.path()));
    let harness = crate::test_support::source_context_with_local_sqlite_ledger()
        .await
        .unwrap();
    let observation = observe(request, &harness).await;
    match previous_home {
        Some(value) => unsafe { std::env::set_var("HOME", value) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    let observation = observation.unwrap();
    assert_one_job(&observation);
    assert_eq!(phase_spine(&observation), expected_shared_phase_spine());
    assert_eq!(observation.result.counts.documents_total, 1);
    assert!(observation.progress.iter().all(|event| {
        !event.message.contains("session fixture") && !event.message.contains("response")
    }));
}

#[tokio::test]
async fn route_failures_map_to_the_same_failed_source_result() {
    for input in ["", "ftp://unsupported.example.test/source"] {
        let harness = crate::test_support::source_context_with_fake_web()
            .await
            .unwrap();
        let result = crate::source::index_source_with_auth(
            SourceRequest::new(input),
            harness.ctx(),
            Some(AuthSnapshot::trusted_system("differential-test")),
        )
        .await
        .unwrap();
        assert_eq!(result.status, LifecycleStatus::Failed, "input={input:?}");
        assert!(
            !result.warnings.is_empty(),
            "route failure must be surfaced as a warning: input={input:?}"
        );
        assert_eq!(
            result.counts.documents_total, 0,
            "route failure must not prepare documents: input={input:?}"
        );
    }
}
