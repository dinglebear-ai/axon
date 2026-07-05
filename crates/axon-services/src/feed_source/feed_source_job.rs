use anyhow::Context;
use async_trait::async_trait;
use axon_api::source::*;
use axon_embedding::provider::EmbeddingProvider;
use axon_jobs::boundary::JobStore;
use axon_ledger::store::LedgerStore;
use axon_vectors::store::VectorStore;
use std::path::Path;
use tokio::sync::Mutex;

use super::feed_source_adapter::{feed_source_id, timestamp};
use super::feed_source_progress::{FeedSourceProgress, source_error_from_api_error};
use super::{FeedSourceIndexInput, FeedSourceIndexOutput, index_feed_source_with_progress};

pub async fn index_feed_source_with_job(
    mut input: FeedSourceIndexInput,
    jobs: &dyn JobStore,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
) -> anyhow::Result<FeedSourceIndexOutput> {
    let feed_path = tokio::fs::canonicalize(&input.feed_path)
        .await
        .with_context(|| {
            format!(
                "invalid feed source path {}",
                public_path_hint(&input.feed_path)
            )
        })?;
    let source_id = feed_source_id(&feed_path);
    let descriptor = jobs
        .create(job_create_request(&input, source_id.clone()))
        .await?;
    input.job_id = descriptor.job_id;
    let progress = JobProgressSink::new(jobs, input.job_id, source_id.clone());
    match index_feed_source_with_progress(
        input.clone(),
        ledger,
        embedding_provider,
        vector_store,
        Some(&progress),
    )
    .await
    {
        Ok(output) => {
            progress
                .record_phase(
                    PipelinePhase::Complete,
                    LifecycleStatus::Completed,
                    Some(counts_for_output(&output)),
                    None,
                    Vec::new(),
                )
                .await?;
            Ok(output)
        }
        Err(err) => {
            let source_error = terminal_source_error(&err, &input.feed_path);
            if let Err(progress_err) = progress
                .record_phase(
                    PipelinePhase::Complete,
                    LifecycleStatus::Failed,
                    None,
                    Some(source_error),
                    Vec::new(),
                )
                .await
            {
                return Err(err.context(format!(
                    "also failed to record terminal feed source job failure: {progress_err}"
                )));
            }
            Err(err)
        }
    }
}

fn terminal_source_error(err: &anyhow::Error, feed_path: &Path) -> SourceError {
    if let Some(api_error) = err.downcast_ref::<ApiError>() {
        return source_error_from_api_error(api_error);
    }
    let safe_error = redact_feed_path(&err.to_string(), feed_path);
    SourceError {
        code: "source.feed.index_failed".to_string(),
        severity: Severity::Failed,
        message: safe_error.clone(),
        source_item_key: None,
        retryable: false,
        provider_id: None,
        cause: Some(safe_error),
    }
}

fn job_create_request(input: &FeedSourceIndexInput, _source_id: SourceId) -> JobCreateRequest {
    JobCreateRequest {
        request_id: None,
        job_kind: JobKind::Source,
        job_intent: JobIntent::Run,
        // Source row is upserted by the ledger DURING the run, AFTER this job is created;
        // jobs.source_id FKs to sources(source_id) and would fail at INSERT. The column is
        // nullable by contract; linking post-upsert is a follow-up.
        source_id: None,
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        priority: JobPriority::Background,
        idempotency_key: None,
        stage_plan: Vec::new(),
        request: Some(serde_json::json!({
            "source_kind": "feed",
            "feed_path_hint": public_path_hint(&input.feed_path),
        })),
        auth_snapshot: MetadataMap::new(),
        config_snapshot_id: Some(ConfigSnapshotId::new("cfg_feed_source")),
        requirements: MetadataMap::new(),
        result_schema: Some("source_result".to_string()),
        metadata: MetadataMap::new(),
    }
}

fn public_path_hint(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| "feed-source".to_string())
}

fn redact_feed_path(message: &str, feed_path: &Path) -> String {
    let mut redacted = message.to_string();
    let path_display = feed_path.display().to_string();
    if !path_display.is_empty() {
        redacted = redacted.replace(&path_display, "<feed-source-path>");
    }
    if let Ok(canonical) = std::fs::canonicalize(feed_path) {
        let canonical_display = canonical.display().to_string();
        if !canonical_display.is_empty() {
            redacted = redacted.replace(&canonical_display, "<feed-source-path>");
        }
    }
    redacted
}

struct JobProgressSink<'a> {
    jobs: &'a dyn JobStore,
    job_id: JobId,
    source_id: SourceId,
    sequence: Mutex<u64>,
}

impl<'a> JobProgressSink<'a> {
    fn new(jobs: &'a dyn JobStore, job_id: JobId, source_id: SourceId) -> Self {
        Self {
            jobs,
            job_id,
            source_id,
            sequence: Mutex::new(0),
        }
    }
}

#[async_trait]
impl FeedSourceProgress for JobProgressSink<'_> {
    async fn record_phase(
        &self,
        phase: PipelinePhase,
        status: LifecycleStatus,
        counts: Option<StageCounts>,
        error: Option<SourceError>,
        provider_reservations: Vec<ProviderReservationSnapshot>,
    ) -> anyhow::Result<()> {
        let mut sequence = self.sequence.lock().await;
        *sequence += 1;
        let sequence = *sequence;
        let event_error = error
            .as_ref()
            .map(|error| source_error_to_api_error(error, phase, self.job_id, &self.source_id));
        self.jobs
            .update_status(JobStatusUpdate {
                job_id: self.job_id,
                source_id: Some(self.source_id.clone()),
                status,
                phase,
                stage_id: None,
                counts: counts.clone(),
                current: None,
                message: Some(format!("feed source {phase:?}").to_ascii_lowercase()),
                error: error.clone(),
            })
            .await?;
        let reservation_id = provider_reservations
            .first()
            .map(|reservation| reservation.reservation_id.clone());
        self.jobs
            .append_event(SourceProgressEvent {
                event_id: format!("evt_feed_{}_{}", self.job_id.0, sequence),
                sequence,
                job_id: self.job_id,
                attempt: 1,
                stage_id: None,
                batch_id: None,
                reservation_id,
                checkpoint_id: None,
                dedupe_key: None,
                phase,
                status,
                severity: if status == LifecycleStatus::Failed {
                    Severity::Failed
                } else {
                    Severity::Info
                },
                visibility: Visibility::Public,
                message: format!("feed source {phase:?}").to_ascii_lowercase(),
                timestamp: timestamp(),
                source_id: Some(self.source_id.clone()),
                canonical_uri: None,
                adapter: None,
                scope: None,
                generation: None,
                counts: counts.clone().unwrap_or_else(empty_counts),
                timing: None,
                current: None,
                throughput: None,
                retry: None,
                warning: None,
                error: event_error,
            })
            .await?;
        self.jobs
            .heartbeat(JobHeartbeat {
                job_id: self.job_id,
                attempt: 1,
                worker_id: Some("feed-source".to_string()),
                phase,
                status,
                stage_id: None,
                heartbeat_at: timestamp(),
                last_event_sequence: Some(sequence),
                counts,
                provider_reservations,
            })
            .await?;
        Ok(())
    }
}

fn source_error_to_api_error(
    error: &SourceError,
    phase: PipelinePhase,
    job_id: JobId,
    source_id: &SourceId,
) -> ApiError {
    let mut api_error = ApiError::new(
        error.code.clone(),
        error_stage_for_phase(phase),
        error.message.clone(),
    )
    .with_job_id(job_id.0.to_string())
    .with_source_id(source_id.0.clone())
    .with_severity(error_severity(error.severity));
    api_error.retryable = error.retryable;
    if let Some(provider_id) = &error.provider_id {
        api_error = api_error.with_provider_id(provider_id.0.clone());
    }
    if let Some(source_item_key) = &error.source_item_key {
        api_error.source_item_key = Some(source_item_key.0.clone());
    }
    if let Some(cause) = &error.cause {
        api_error = api_error.with_context("cause", cause.clone());
    }
    api_error
}

fn error_stage_for_phase(phase: PipelinePhase) -> ErrorStage {
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
        PipelinePhase::Normalizing => ErrorStage::Normalizing,
        PipelinePhase::Parsing => ErrorStage::ParsingContent,
        PipelinePhase::Graphing => ErrorStage::Graphing,
        PipelinePhase::Preparing | PipelinePhase::Batching => ErrorStage::Preparing,
        PipelinePhase::Embedding => ErrorStage::Embedding,
        PipelinePhase::Vectorizing | PipelinePhase::Upserting => ErrorStage::Upserting,
        PipelinePhase::Retrieving => ErrorStage::Retrieving,
        PipelinePhase::Synthesizing => ErrorStage::Synthesizing,
        PipelinePhase::Publishing => ErrorStage::Publishing,
        PipelinePhase::Cleaning => ErrorStage::Cleaning,
        PipelinePhase::Queued
        | PipelinePhase::Requested
        | PipelinePhase::Enriching
        | PipelinePhase::Evaluating
        | PipelinePhase::Complete
        | PipelinePhase::Canceled => ErrorStage::Observing,
    }
}

fn error_severity(severity: Severity) -> ErrorSeverity {
    match severity {
        Severity::Debug => ErrorSeverity::Info,
        Severity::Info => ErrorSeverity::Info,
        Severity::Warning => ErrorSeverity::Warning,
        Severity::Degraded => ErrorSeverity::Degraded,
        Severity::Failed => ErrorSeverity::Failed,
        Severity::Fatal => ErrorSeverity::Fatal,
    }
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

fn counts_for_output(output: &FeedSourceIndexOutput) -> StageCounts {
    StageCounts {
        items_total: None,
        items_done: 0,
        documents_total: Some(output.documents_prepared),
        documents_done: output.documents_prepared,
        chunks_total: Some(output.chunks_prepared),
        chunks_done: output.chunks_prepared,
        bytes_total: None,
        bytes_done: 0,
    }
}
