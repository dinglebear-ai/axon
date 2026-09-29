//! Real dispatch failure persistence, including the public event projection.
use super::*;
use axon_api::source::*;

struct FailingAcquisition {
    inner: FakeSourceAdapter,
    job: Mutex<Option<JobId>>,
    retryable: bool,
}

#[async_trait::async_trait]
impl SourceAdapter for FailingAcquisition {
    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn version(&self) -> &'static str {
        self.inner.version()
    }
    async fn capabilities(&self) -> Result<SourceAdapterCapability, ApiError> {
        self.inner.capabilities().await
    }
    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest, ApiError> {
        self.inner.discover(plan).await
    }
    async fn acquire(
        &self,
        plan: &SourcePlan,
        _: &SourceManifestDiff,
    ) -> Result<SourceAcquisition, ApiError> {
        *self.job.lock().unwrap() = Some(plan.job_id);
        let mut error = ApiError::new("adapter.test.read_failed", ErrorStage::Fetching,
            "cannot read https://reader:synthetic-password@example.test/file?token=synthetic-token\r\nnext")
            .with_context("source_item_key", "src/file.rs")
            .with_context("body", "synthetic-body-secret");
        error.retryable = self.retryable;
        Err(error)
    }
    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquired: SourceAcquisition,
    ) -> Result<StageExecutionResult<Vec<SourceDocument>>, ApiError> {
        self.inner.normalize(plan, acquired).await
    }
}

#[tokio::test]
async fn dispatch_persists_typed_sanitized_acquisition_failure() {
    for (scheduled, retryable) in [(false, false), (false, true), (true, false), (true, true)] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().to_string_lossy().to_string();
        let route = route_for(&source);
        let jobs = Arc::new(FakeJobWatchStore::new());
        let mut runtime = test_runtime_with_jobs(
            Arc::new(FakeVectorStore::new("fake-vector")),
            Arc::new(FakeLedgerStore::new()),
            jobs.clone(),
        );
        runtime.embed_scheduler_enabled = scheduled;
        let adapter = FailingAcquisition {
            inner: FakeSourceAdapter::new(route.adapter.clone()).with_item(
                "src/file.rs",
                ContentKind::PlainText,
                "text",
            ),
            job: Mutex::new(None),
            retryable,
        };
        dispatch_materialized(
            &runtime,
            &adapter,
            family_source_plan(&source, &route, false, None, None),
            "test",
            "test-owner",
            None,
            &test_execution(&source),
            |plan| async move { Ok(MaterializedSource::virtual_source(plan)) },
        )
        .await
        .expect_err("typed acquisition failure");
        let job_id = adapter.job.lock().unwrap().unwrap();
        let job = jobs.get(job_id).await.unwrap().unwrap();
        assert_eq!(job.status, LifecycleStatus::Failed);
        assert_diagnostic(
            job.last_error.as_ref().expect("persisted failure"),
            retryable,
        );
        let events = jobs.recorded_events(job_id).await;
        let errors = events
            .iter()
            .filter_map(|event| event.details.get("source_progress_event"))
            .filter_map(|value| serde_json::from_value::<SourceProgressEvent>(value.clone()).ok())
            .filter_map(|event| event.error)
            .collect::<Vec<_>>();
        assert!(!errors.is_empty(), "failure event must be persisted");
        for error in errors {
            assert_diagnostic(&error, retryable);
        }
    }
}

fn assert_diagnostic(error: &impl serde::Serialize, retryable: bool) {
    let value = serde_json::to_value(error).unwrap();
    assert_eq!(value["code"], "adapter.test.read_failed", "{value}");
    assert_eq!(value["retryable"], retryable);
    assert_eq!(value["source_item_key"], "src/file.rs");
    let serialized = value.to_string();
    for secret in [
        "synthetic-password",
        "synthetic-token",
        "synthetic-body-secret",
    ] {
        assert!(
            !serialized.contains(secret),
            "diagnostic included a hidden value"
        );
    }
    assert!(!value["message"].as_str().unwrap().contains(['\r', '\n']));
}
