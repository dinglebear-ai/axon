use axon_api::source::{AuthSnapshot, ExecutionMode, JobKind, SourceRequest, SourceScope};

struct SourceRuntimeHarness {
    harness: crate::test_support::SourceWebJobIdentityHarness,
}

impl SourceRuntimeHarness {
    async fn with_sqlite_and_fakes() -> Self {
        Self {
            harness: crate::test_support::source_context_with_fake_web()
                .await
                .expect("source context with fake web"),
        }
    }

    async fn enqueue_source_job(
        &self,
        request: SourceRequest,
    ) -> axon_jobs::workers::unified::UnifiedClaimedJob {
        self.harness
            .enqueue_and_claim_source(request)
            .await
            .expect("enqueue source")
    }

    async fn run_source_job_once(
        &self,
        claimed: &axon_jobs::workers::unified::UnifiedClaimedJob,
    ) -> Result<(), axon_api::source::ApiError> {
        self.harness.run_source_claim_once(claimed).await
    }

    async fn index_source_inline(
        &self,
        request: SourceRequest,
        auth: Option<AuthSnapshot>,
    ) -> anyhow::Result<axon_api::source::SourceResult> {
        crate::source::index_source_with_auth(request, self.harness.ctx(), auth).await
    }

    async fn jobs_by_kind(&self, kind: JobKind) -> Vec<axon_api::source::JobSummary> {
        self.harness.jobs_by_kind(kind).await.expect("list jobs")
    }

    async fn source_summary_for(&self, source: &str) -> axon_api::source::SourceSummary {
        self.harness
            .source_summary_for(source)
            .await
            .expect("source summary")
    }
}

#[tokio::test]
async fn detached_web_source_uses_claimed_source_job_id() {
    let harness = SourceRuntimeHarness::with_sqlite_and_fakes().await;
    let mut request = SourceRequest::new("https://docs.example.test/");
    request.scope = Some(SourceScope::Page);
    request.execution.mode = ExecutionMode::Background;

    let claimed = harness.enqueue_source_job(request.clone()).await;
    harness
        .run_source_job_once(&claimed)
        .await
        .expect("source run");

    let jobs = harness.jobs_by_kind(JobKind::Source).await;
    assert_eq!(
        jobs.len(),
        1,
        "web source path must not create a nested Source job"
    );
    assert_eq!(jobs[0].job_id, claimed.job_id);
    assert!(
        harness.jobs_by_kind(JobKind::Crawl).await.is_empty(),
        "web Source execution must not create legacy Crawl jobs"
    );
    assert!(
        harness.jobs_by_kind(JobKind::Embed).await.is_empty(),
        "web Source execution must embed inline without child Embed jobs"
    );

    let ledger = harness
        .source_summary_for("https://docs.example.test/")
        .await;
    assert_eq!(ledger.last_job_id.as_ref(), Some(&claimed.job_id));
}

#[tokio::test]
async fn inline_web_source_creates_one_source_job() {
    let harness = SourceRuntimeHarness::with_sqlite_and_fakes().await;
    let mut request = SourceRequest::new("https://one.example.test/");
    request.scope = Some(SourceScope::Page);

    let result = harness
        .index_source_inline(request, Some(AuthSnapshot::trusted_system("test")))
        .await
        .expect("inline source");

    let jobs = harness.jobs_by_kind(JobKind::Source).await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].job_id, result.job_id);
}
