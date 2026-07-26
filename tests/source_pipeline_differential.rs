//! Production-composition differential smoke test for the unified source path.

use axon_api::source::{
    AuthSnapshot, JobEventListRequest, LifecycleStatus, PipelinePhase, SourceRequest, Visibility,
};
use axon_services::source::index_source_with_auth;
use std::error::Error;

async fn observe(
    request: SourceRequest,
    harness: &axon_services::test_support::SourceWebJobIdentityHarness,
) -> Result<(LifecycleStatus, Vec<PipelinePhase>, usize, usize), Box<dyn Error + Send + Sync>> {
    let result = index_source_with_auth(
        request,
        harness.ctx(),
        Some(AuthSnapshot::trusted_system("integration-differential")),
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
    let stages = store.stages(result.job_id).await?;
    let mut phases = Vec::new();
    for phase in events
        .iter()
        .filter(|event| event.status != LifecycleStatus::CompletedDegraded)
        .map(|event| event.phase)
        .filter(|phase| *phase != PipelinePhase::Leasing)
    {
        if phases.last() != Some(&phase) {
            phases.push(phase);
        }
    }
    Ok((
        result.status,
        phases,
        stages.len(),
        harness
            .vectors()
            .points(&harness.ctx().cfg().collection)
            .await
            .len(),
    ))
}

#[tokio::test]
async fn web_and_local_use_the_same_production_composition() {
    let web = axon_services::test_support::source_context_with_fake_web()
        .await
        .unwrap();
    let web_observation = observe(
        SourceRequest::new("https://docs.example.test/integration-differential"),
        &web,
    )
    .await
    .unwrap();

    let local_dir = tempfile::tempdir().unwrap();
    std::fs::write(local_dir.path().join("fixture.md"), "# shared\n\nbody\n").unwrap();
    let local = axon_services::test_support::source_context_with_local_sqlite_ledger()
        .await
        .unwrap();
    let local_observation = observe(
        SourceRequest::local_path(local_dir.path().to_string_lossy(), true),
        &local,
    )
    .await
    .unwrap();

    assert_eq!(web_observation.0, LifecycleStatus::Completed);
    assert_eq!(local_observation.0, LifecycleStatus::Completed);
    let expected = vec![
        PipelinePhase::Discovering,
        PipelinePhase::Diffing,
        PipelinePhase::Fetching,
        PipelinePhase::Normalizing,
        PipelinePhase::Preparing,
        PipelinePhase::Embedding,
        PipelinePhase::Upserting,
        PipelinePhase::Publishing,
    ];
    assert_eq!(web_observation.1, expected);
    assert_eq!(local_observation.1, expected);
    assert_eq!(web_observation.2, local_observation.2);
    assert_eq!(web_observation.3, local_observation.3);
}
