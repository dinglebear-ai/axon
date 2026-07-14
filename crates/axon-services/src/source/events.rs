use std::sync::Arc;

use axon_api::source::{
    AdapterRef, JobId, LifecycleStatus, PipelinePhase, Severity, SourceKind, SourceProgressEvent,
    SourceScope, StageCounts, Timestamp, Visibility,
};
use axon_jobs::boundary::JobStore;

#[derive(Clone)]
pub(crate) struct SourceEventEmitter {
    jobs: Option<Arc<dyn JobStore>>,
    job_id: Option<JobId>,
    source_kind: Option<SourceKind>,
    scope: Option<SourceScope>,
    adapter: Option<AdapterRef>,
}

impl SourceEventEmitter {
    pub(crate) fn new(jobs: Option<Arc<dyn JobStore>>, job_id: Option<JobId>) -> Self {
        Self {
            jobs,
            job_id,
            source_kind: None,
            scope: None,
            adapter: None,
        }
    }

    pub(crate) fn for_web(
        jobs: Option<Arc<dyn JobStore>>,
        job_id: JobId,
        scope: SourceScope,
    ) -> Self {
        Self::new(jobs, Some(job_id)).with_route(
            SourceKind::Web,
            scope,
            AdapterRef {
                name: "web".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        )
    }

    pub(crate) fn with_route(
        mut self,
        source_kind: SourceKind,
        scope: SourceScope,
        adapter: AdapterRef,
    ) -> Self {
        self.source_kind = Some(source_kind);
        self.scope = Some(scope);
        self.adapter = Some(adapter);
        self
    }

    pub(crate) async fn running(&self, phase: PipelinePhase, message: impl Into<String>) {
        self.emit(phase, LifecycleStatus::Running, Severity::Info, message)
            .await;
    }

    pub(crate) async fn completed(&self, phase: PipelinePhase, message: impl Into<String>) {
        self.emit(phase, LifecycleStatus::Completed, Severity::Info, message)
            .await;
    }

    pub(crate) async fn failed(&self, phase: PipelinePhase, message: impl Into<String>) {
        self.emit(phase, LifecycleStatus::Failed, Severity::Failed, message)
            .await;
    }

    async fn emit(
        &self,
        phase: PipelinePhase,
        status: LifecycleStatus,
        severity: Severity,
        message: impl Into<String>,
    ) {
        let phase_label = axon_observe::phase::label(phase);
        if let Err(err) = record_metric(phase, status, self.source_kind, self.scope, &self.adapter)
        {
            tracing::warn!(
                phase = %phase_label,
                error = %err,
                "failed to record source progress metric"
            );
        }
        let (Some(jobs), Some(job_id)) = (self.jobs.as_ref(), self.job_id) else {
            return;
        };
        if let Err(err) = emit_source_event(
            jobs.as_ref(),
            job_id,
            phase,
            status,
            severity,
            self.source_kind,
            self.scope,
            self.adapter.clone(),
            message,
        )
        .await
        {
            tracing::warn!(
                job_id = %job_id.0,
                phase = %phase_label,
                error = %err,
                "failed to emit source progress event"
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn emit_source_event(
    jobs: &dyn JobStore,
    job_id: JobId,
    phase: PipelinePhase,
    status: LifecycleStatus,
    severity: Severity,
    source_kind: Option<SourceKind>,
    scope: Option<SourceScope>,
    adapter: Option<AdapterRef>,
    message: impl Into<String>,
) -> anyhow::Result<()> {
    let sequence = jobs.latest_event_sequence(job_id).await?.unwrap_or(0) + 1;
    let mut event =
        SourceProgressEvent::minimal(job_id, sequence, phase, status, severity, message);
    event.attempt = 1;
    event.visibility = Visibility::Public;
    event.source_id = None;
    event.adapter = adapter;
    event.scope = scope;
    event.counts = empty_counts();
    event.timestamp = Timestamp::from(chrono::Utc::now());
    jobs.append_event(event).await?;
    let _ = source_kind;
    Ok(())
}

fn record_metric(
    phase: PipelinePhase,
    status: LifecycleStatus,
    source_kind: Option<SourceKind>,
    scope: Option<SourceScope>,
    adapter: &Option<AdapterRef>,
) -> anyhow::Result<()> {
    let source_kind = source_kind.map(enum_label);
    let scope = scope.map(enum_label);
    let status = enum_label(status);
    let adapter_name = adapter.as_ref().map(|adapter| adapter.name.as_str());

    let mut labels = vec![("status", status.as_str())];
    if let Some(value) = source_kind.as_deref() {
        labels.push(("source_kind", value));
    }
    if let Some(value) = scope.as_deref() {
        labels.push(("scope", value));
    }
    if let Some(value) = adapter_name {
        labels.push(("adapter", value));
    }

    axon_observe::source_metrics::record_source_phase_with_labels(
        &axon_observe::phase::label(phase),
        &labels,
    )
}

fn enum_label<T>(value: T) -> String
where
    T: serde::Serialize + std::fmt::Debug,
{
    serde_json::to_value(&value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("{value:?}").to_ascii_lowercase())
}

fn empty_counts() -> StageCounts {
    StageCounts {
        items_total: None,
        items_done: 0,
        documents_total: None,
        documents_done: 0,
        chunks_total: None,
        chunks_done: 0,
        bytes_total: None,
        bytes_done: 0,
    }
}
