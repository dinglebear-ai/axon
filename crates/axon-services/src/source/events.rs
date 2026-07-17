use std::sync::Arc;

use axon_api::source::{
    AdapterRef, ApiError, ErrorStage, JobId, LifecycleStatus, PipelinePhase, ProgressCurrent,
    Severity, SourceError, SourceGenerationId, SourceId, SourceKind, SourceProgressEvent,
    SourceScope, SourceWarning, StageCounts, Timestamp, Visibility,
};
use axon_jobs::boundary::JobStore;

#[derive(Clone)]
pub(crate) struct SourceEventEmitter {
    jobs: Option<Arc<dyn JobStore>>,
    job_id: Option<JobId>,
    source_kind: Option<SourceKind>,
    source_id: Option<SourceId>,
    canonical_uri: Option<String>,
    scope: Option<SourceScope>,
    adapter: Option<AdapterRef>,
    attempt: u32,
}

impl SourceEventEmitter {
    pub(crate) fn new(jobs: Option<Arc<dyn JobStore>>, job_id: Option<JobId>) -> Self {
        Self {
            jobs,
            job_id,
            source_kind: None,
            source_id: None,
            canonical_uri: None,
            scope: None,
            adapter: None,
            attempt: 1,
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

    pub(crate) fn with_source(
        mut self,
        source_id: SourceId,
        canonical_uri: impl Into<String>,
    ) -> Self {
        self.source_id = Some(source_id);
        self.canonical_uri = Some(canonical_uri.into());
        self
    }

    pub(crate) fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt = attempt.max(1);
        self
    }

    pub(crate) async fn running(&self, phase: PipelinePhase, message: impl Into<String>) {
        self.emit(
            phase,
            LifecycleStatus::Running,
            Severity::Info,
            message,
            SourceEventDetails::default(),
        )
        .await;
    }

    pub(crate) async fn completed(&self, phase: PipelinePhase, message: impl Into<String>) {
        self.completed_with(phase, message, SourceEventDetails::default())
            .await;
    }

    pub(crate) async fn completed_with(
        &self,
        phase: PipelinePhase,
        message: impl Into<String>,
        details: SourceEventDetails,
    ) {
        self.emit(
            phase,
            LifecycleStatus::Completed,
            Severity::Info,
            message,
            details,
        )
        .await;
    }

    pub(crate) async fn failed(&self, phase: PipelinePhase, message: impl Into<String>) {
        self.emit(
            phase,
            LifecycleStatus::Failed,
            Severity::Failed,
            message,
            SourceEventDetails::default(),
        )
        .await;
    }

    pub(crate) async fn failed_with_error(
        &self,
        phase: PipelinePhase,
        message: impl Into<String>,
        error: ApiError,
    ) {
        self.emit(
            phase,
            LifecycleStatus::Failed,
            Severity::Failed,
            message,
            SourceEventDetails {
                error: Some(error),
                ..SourceEventDetails::default()
            },
        )
        .await;
    }

    pub(crate) async fn warning(
        &self,
        phase: PipelinePhase,
        warning: SourceWarning,
        generation: Option<SourceGenerationId>,
    ) {
        let message = warning.message.clone();
        let current = warning
            .source_item_key
            .clone()
            .map(|source_item_key| ProgressCurrent {
                source_item_key: Some(source_item_key),
                document_id: None,
                chunk_id: None,
                adapter: self.adapter.as_ref().map(|adapter| adapter.name.clone()),
                provider: None,
                message: Some(message.clone()),
            });
        self.emit(
            phase,
            LifecycleStatus::CompletedDegraded,
            Severity::Degraded,
            message,
            SourceEventDetails {
                generation,
                current,
                warning: Some(warning),
                ..SourceEventDetails::default()
            },
        )
        .await;
    }

    pub(crate) async fn item_error(
        &self,
        phase: PipelinePhase,
        error: SourceError,
        generation: Option<SourceGenerationId>,
    ) {
        let message = error.message.clone();
        let source_item_key = error.source_item_key.clone();
        let provider_id = error.provider_id.clone();
        let current = error
            .source_item_key
            .clone()
            .map(|source_item_key| ProgressCurrent {
                source_item_key: Some(source_item_key),
                document_id: None,
                chunk_id: None,
                adapter: self.adapter.as_ref().map(|adapter| adapter.name.clone()),
                provider: error.provider_id.clone(),
                message: Some(message.clone()),
            });
        let mut api_error = ApiError::new(error.code, error_stage(phase), message.clone());
        api_error.retryable = error.retryable;
        api_error.source_item_key = source_item_key.map(|key| key.0);
        api_error.provider_id = provider_id.map(|provider| provider.0);
        self.emit(
            phase,
            LifecycleStatus::CompletedDegraded,
            error.severity,
            message,
            SourceEventDetails {
                generation,
                current,
                error: Some(api_error),
                ..SourceEventDetails::default()
            },
        )
        .await;
    }

    async fn emit(
        &self,
        phase: PipelinePhase,
        status: LifecycleStatus,
        severity: Severity,
        message: impl Into<String>,
        details: SourceEventDetails,
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
            self.source_id.clone(),
            self.canonical_uri.clone(),
            self.scope,
            self.adapter.clone(),
            self.attempt,
            message,
            details,
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

#[derive(Debug, Clone, Default)]
pub(crate) struct SourceEventDetails {
    pub(crate) generation: Option<SourceGenerationId>,
    pub(crate) counts: Option<StageCounts>,
    pub(crate) current: Option<ProgressCurrent>,
    pub(crate) warning: Option<SourceWarning>,
    pub(crate) error: Option<ApiError>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn emit_source_event(
    jobs: &dyn JobStore,
    job_id: JobId,
    phase: PipelinePhase,
    status: LifecycleStatus,
    severity: Severity,
    source_id: Option<SourceId>,
    canonical_uri: Option<String>,
    scope: Option<SourceScope>,
    adapter: Option<AdapterRef>,
    attempt: u32,
    message: impl Into<String>,
    details: SourceEventDetails,
) -> anyhow::Result<()> {
    let sequence = jobs.latest_event_sequence(job_id).await?.unwrap_or(0) + 1;
    let mut event =
        SourceProgressEvent::minimal(job_id, sequence, phase, status, severity, message);
    event.attempt = attempt.max(1);
    event.visibility = Visibility::Public;
    event.source_id = source_id;
    event.canonical_uri = canonical_uri;
    event.adapter = adapter;
    event.scope = scope;
    event.generation = details.generation;
    event.counts = details.counts.unwrap_or_else(empty_counts);
    event.current = details.current;
    event.warning = details.warning;
    event.error = details.error;
    event.timestamp = Timestamp::from(chrono::Utc::now());
    jobs.append_event(event).await?;
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

fn error_stage(phase: PipelinePhase) -> ErrorStage {
    match phase {
        PipelinePhase::Resolving => ErrorStage::Resolving,
        PipelinePhase::Routing => ErrorStage::Routing,
        PipelinePhase::Authorizing => ErrorStage::Authorizing,
        PipelinePhase::Planning => ErrorStage::Planning,
        PipelinePhase::Leasing => ErrorStage::Leasing,
        PipelinePhase::Discovering => ErrorStage::Discovering,
        PipelinePhase::Diffing => ErrorStage::Diffing,
        PipelinePhase::Fetching => ErrorStage::Fetching,
        PipelinePhase::Rendering => ErrorStage::Rendering,
        PipelinePhase::Enriching => ErrorStage::Enriching,
        PipelinePhase::Normalizing => ErrorStage::Normalizing,
        PipelinePhase::Parsing => ErrorStage::ParsingContent,
        PipelinePhase::Graphing => ErrorStage::Graphing,
        PipelinePhase::Preparing => ErrorStage::Preparing,
        PipelinePhase::Batching => ErrorStage::Batching,
        PipelinePhase::Embedding => ErrorStage::Embedding,
        PipelinePhase::Vectorizing => ErrorStage::Vectorizing,
        PipelinePhase::Upserting => ErrorStage::Upserting,
        PipelinePhase::Publishing | PipelinePhase::Complete => ErrorStage::Publishing,
        PipelinePhase::Cleaning => ErrorStage::Cleaning,
        PipelinePhase::Retrieving => ErrorStage::Retrieving,
        PipelinePhase::Synthesizing => ErrorStage::Synthesizing,
        PipelinePhase::Evaluating => ErrorStage::Evaluating,
        _ => ErrorStage::Internal,
    }
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

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
