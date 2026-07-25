use axon_api::source::{AuthSnapshot, ExecutionMode, JobKind, SourceRequest, SourceScope};

/// Build a `SourceRequest` for an existing local directory. The harness
/// (`crate::test_support::source_context_with_fake_web`) wires a full
/// `TargetLocalSourceRuntime` (fake jobs/ledger/embedding/vector) that local
/// dispatch uses identically to web dispatch — local acquisition never
/// touches the harness's fake `fetch_provider`/`render_provider`, so the same
/// "web" harness exercises the local family too without any test_support.rs
/// changes.
fn local_source_request(root: &std::path::Path) -> SourceRequest {
    SourceRequest::local_path(root.to_string_lossy().to_string(), true)
}

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

    /// Local-source variant: a real `SqliteLedgerStore` shares the `jobs`
    /// pool instead of the in-memory `FakeLedgerStore` — required for local
    /// dispatch specifically, see
    /// `test_support::source_context_with_local_sqlite_ledger`'s doc comment.
    async fn with_sqlite_ledger_and_fakes() -> Self {
        Self {
            harness: crate::test_support::source_context_with_local_sqlite_ledger()
                .await
                .expect("source context with sqlite ledger"),
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

/// Local counterpart of `detached_web_source_uses_claimed_source_job_id`:
/// proves local sources honor a worker-claimed parent `Source` job id instead
/// of unconditionally creating a second `jobs` row (finding C2 — local was
/// the one family whose dispatcher discarded
/// `SourceExecutionContext::existing_job_id`).
#[tokio::test]
async fn detached_local_source_uses_claimed_source_job_id() {
    let harness = SourceRuntimeHarness::with_sqlite_ledger_and_fakes().await;
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("lib.rs"), "pub fn f() {}\n").expect("write fixture file");
    let mut request = local_source_request(dir.path());
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
        "local source path must not create a nested Source job"
    );
    assert_eq!(jobs[0].job_id, claimed.job_id);
}

/// Local counterpart of `inline_web_source_creates_one_source_job`: an inline
/// (no pre-existing job) local index must still create exactly one `Source`
/// job row, and that job's id must be the one reported back on the result.
#[tokio::test]
async fn inline_local_source_creates_one_source_job() {
    let harness = SourceRuntimeHarness::with_sqlite_ledger_and_fakes().await;
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("lib.rs"), "pub fn f() {}\n").expect("write fixture file");
    let request = local_source_request(dir.path());

    let result = harness
        .index_source_inline(request, Some(AuthSnapshot::trusted_system("test")))
        .await
        .expect("inline source");

    let jobs = harness.jobs_by_kind(JobKind::Source).await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].job_id, result.job_id);
}
