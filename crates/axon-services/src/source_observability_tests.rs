use axon_api::source::{
    JobEventListRequest, PipelinePhase, SourceRequest, SourceScope, Visibility,
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
    ) -> anyhow::Result<Vec<PipelinePhase>> {
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
        Ok(page.events.into_iter().map(|event| event.phase).collect())
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

    assert_eq!(
        phases,
        vec![
            PipelinePhase::Resolving,
            PipelinePhase::Routing,
            PipelinePhase::Authorizing,
            PipelinePhase::Discovering,
            PipelinePhase::Diffing,
            PipelinePhase::Fetching,
            PipelinePhase::Normalizing,
            PipelinePhase::Preparing,
            PipelinePhase::Embedding,
            PipelinePhase::Upserting,
            PipelinePhase::Publishing,
            PipelinePhase::Cleaning,
            PipelinePhase::Complete,
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
