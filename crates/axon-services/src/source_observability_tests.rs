use axon_api::source::{
    JobEventListRequest, LifecycleStatus, PipelinePhase, SourceRequest, SourceScope, Visibility,
};

struct SourceObservabilityHarness {
    harness: crate::test_support::SourceWebJobIdentityHarness,
}

impl SourceObservabilityHarness {
    async fn with_fake_web() -> Self {
        Self {
            harness: crate::test_support::source_context_with_fake_web()
                .await
                .expect("source context with fake web"),
        }
    }

    async fn run_source(
        &self,
        request: SourceRequest,
    ) -> anyhow::Result<axon_jobs::workers::unified::UnifiedClaimedJob> {
        let claimed = self.harness.enqueue_and_claim_source(request).await?;
        self.harness.run_source_claim_once(&claimed).await?;
        Ok(claimed)
    }

    async fn event_phases(
        &self,
        job_id: axon_api::source::JobId,
    ) -> anyhow::Result<Vec<(PipelinePhase, LifecycleStatus)>> {
        let page = self
            .harness
            .ctx()
            .job_store()
            .expect("job store")
            .events(JobEventListRequest {
                job_id,
                after_sequence: None,
                limit: Some(100),
                severity: None,
                visibility: Some(Visibility::Public),
                phase: None,
                since_sequence: None,
                cursor: None,
            })
            .await?;
        Ok(page
            .events
            .into_iter()
            .map(|event| (event.phase, event.status))
            .collect())
    }

    async fn service_events(
        &self,
        job_id: axon_api::source::JobId,
    ) -> anyhow::Result<axon_api::source::JobEventPage> {
        crate::jobs::unified_job_events(
            self.harness.ctx(),
            JobEventListRequest {
                job_id,
                after_sequence: None,
                limit: Some(100),
                severity: None,
                visibility: Some(Visibility::Public),
                phase: None,
                since_sequence: None,
                cursor: None,
            },
        )
        .await
        .map_err(anyhow::Error::msg)
    }
}

#[tokio::test]
async fn page_source_emits_ordered_phase_events() {
    let harness = SourceObservabilityHarness::with_fake_web().await;
    let mut request = SourceRequest::new("https://docs.example.test/intro");
    request.scope = Some(SourceScope::Page);

    let claimed = harness.run_source(request).await.expect("source run");
    let phases = harness
        .event_phases(claimed.job_id)
        .await
        .expect("event phases");

    // The spine assertion filters out `CompletedDegraded` warning events so it
    // doesn't couple to whether preparation warns for this fixture: warnings
    // depend on the acquisition path's content kind (an HTML-fetched page has
    // no registered parser and degrades; a rendered-markdown page parses
    // cleanly). Any warning events that do occur must sit at the Publishing
    // phase.
    for (phase, status) in phases
        .iter()
        .filter(|(_, status)| *status == LifecycleStatus::CompletedDegraded)
    {
        assert_eq!(
            *phase,
            PipelinePhase::Publishing,
            "unexpected degraded event at {phase:?} ({status:?})"
        );
    }
    let spine = phases
        .into_iter()
        .filter(|(_, status)| *status != LifecycleStatus::CompletedDegraded)
        .collect::<Vec<_>>();
    assert_eq!(
        spine,
        vec![
            (PipelinePhase::Resolving, LifecycleStatus::Running),
            (PipelinePhase::Routing, LifecycleStatus::Running),
            (PipelinePhase::Authorizing, LifecycleStatus::Running),
            (PipelinePhase::Discovering, LifecycleStatus::Running),
            (PipelinePhase::Discovering, LifecycleStatus::Completed),
            (PipelinePhase::Diffing, LifecycleStatus::Running),
            (PipelinePhase::Diffing, LifecycleStatus::Completed),
            (PipelinePhase::Fetching, LifecycleStatus::Running),
            (PipelinePhase::Normalizing, LifecycleStatus::Running),
            (PipelinePhase::Preparing, LifecycleStatus::Running),
            (PipelinePhase::Embedding, LifecycleStatus::Running),
            (PipelinePhase::Upserting, LifecycleStatus::Running),
            (PipelinePhase::Publishing, LifecycleStatus::Running),
            (PipelinePhase::Publishing, LifecycleStatus::Completed),
            (PipelinePhase::Cleaning, LifecycleStatus::Running),
            (PipelinePhase::Complete, LifecycleStatus::Completed),
        ]
    );
}

#[tokio::test]
async fn metrics_reject_high_cardinality_labels() {
    let err = axon_observe::source_metrics::record_source_phase_with_labels(
        "fetching",
        &[("url", "https://secret.example.test/token")],
    )
    .expect_err("url label rejected");
    assert!(err.to_string().contains("unsupported source metric label"));
}

#[tokio::test]
async fn unified_job_events_service_reads_same_public_event_page() {
    let harness = SourceObservabilityHarness::with_fake_web().await;
    let mut request = SourceRequest::new("https://docs.example.test/intro");
    request.scope = Some(SourceScope::Page);

    let claimed = harness.run_source(request).await.expect("source run");
    let direct = harness
        .harness
        .ctx()
        .job_store()
        .expect("job store")
        .events(JobEventListRequest {
            job_id: claimed.job_id,
            after_sequence: None,
            limit: Some(100),
            severity: None,
            visibility: Some(Visibility::Public),
            phase: None,
            since_sequence: None,
            cursor: None,
        })
        .await
        .expect("direct events");
    let service = harness
        .service_events(claimed.job_id)
        .await
        .expect("service events");

    assert_eq!(service, direct);
}
