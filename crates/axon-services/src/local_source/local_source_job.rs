use anyhow::Context;
use async_trait::async_trait;
use axon_api::source::*;
use axon_embedding::provider::EmbeddingProvider;
use axon_jobs::boundary::JobStore;
use axon_ledger::store::LedgerStore;
use axon_vectors::store::VectorStore;
use std::path::Path;

use super::local_source_adapter::{local_source_id, timestamp};
use super::local_source_progress::{LocalSourceProgress, source_error_from_api_error};
use super::{LocalSourceIndexInput, LocalSourceIndexOutput, index_local_source_with_progress};

/// Create a source job row, index the local source under it, and record
/// terminal job status. Used by callers that always own the job outright —
/// no worker has already claimed one for this run (e.g. code-search
/// auto-refresh, `query/code_search_refresh.rs`).
pub async fn index_local_source_with_job(
    mut input: LocalSourceIndexInput,
    jobs: &dyn JobStore,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
) -> anyhow::Result<LocalSourceIndexOutput> {
    reject_symlinked_source_root(&input.root).await?;
    let root = tokio::fs::canonicalize(&input.root)
        .await
        .with_context(|| {
            format!(
                "invalid local source root {}",
                public_path_hint(&input.root)
            )
        })?;
    let source_id = local_source_id(&root);
    let descriptor = jobs
        .create(job_create_request(&input, JobPriority::Background, None, 1))
        .await?;
    input.job_id = descriptor.job_id;
    run_indexing(
        input,
        descriptor.job_id,
        true,
        source_id,
        jobs,
        ledger,
        embedding_provider,
        vector_store,
    )
    .await
}

async fn reject_symlinked_source_root(root: &Path) -> anyhow::Result<()> {
    let metadata = tokio::fs::symlink_metadata(root)
        .await
        .with_context(|| format!("invalid local source root {}", public_path_hint(root)))?;
    if metadata.file_type().is_symlink() {
        anyhow::bail!(
            "unsafe local source root {}: symlinks are not allowed",
            public_path_hint(root)
        );
    }
    Ok(())
}

/// Shared indexing + terminal-status bookkeeping, used by
/// `index_local_source_with_job` above (the sole remaining caller —
/// `code_search_refresh.rs`'s code-search auto-refresh, which always owns
/// its job outright). The unified `source::dispatch::dispatch_local` path no
/// longer has a counterpart entry point here: it routes through the shared
/// `non_web` runner instead (finding C1;
/// `source/dispatch/local.rs`), which already has its own
/// `owns_status`-equivalent handling of a worker-claimed parent job id via
/// `SourceExecutionContext`.
///
/// `owns_status` gates only the *terminal* `Complete` status write —
/// intermediate phase updates inside `index_local_source_with_progress`
/// always flow through `progress` (`JobProgressSink`) regardless of
/// ownership.
async fn run_indexing(
    input: LocalSourceIndexInput,
    job_id: JobId,
    owns_status: bool,
    source_id: SourceId,
    jobs: &dyn JobStore,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
) -> anyhow::Result<LocalSourceIndexOutput> {
    let progress = JobProgressSink::new(jobs, job_id, source_id);
    match index_local_source_with_progress(
        input.clone(),
        ledger,
        embedding_provider,
        vector_store,
        Some(&progress),
    )
    .await
    {
        Ok(output) => {
            if owns_status {
                progress
                    .record_phase(
                        PipelinePhase::Complete,
                        LifecycleStatus::Completed,
                        Some(counts_for_output(&output)),
                        None,
                        Vec::new(),
                    )
                    .await?;
            }
            Ok(output)
        }
        Err(err) => {
            if owns_status {
                let source_error = terminal_source_error(&err, &input.root);
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
                        "also failed to record terminal local source job failure: {progress_err}"
                    )));
                }
            }
            Err(err)
        }
    }
}

fn terminal_source_error(err: &anyhow::Error, root: &Path) -> SourceError {
    if let Some(api_error) = err.downcast_ref::<ApiError>() {
        return source_error_from_api_error(api_error);
    }
    let message = redact_local_root(&err.to_string(), root);
    // `{err:#}` (anyhow's alternate Display) prints the whole `.context()`
    // chain; `message` above only ever holds the outermost frame. Only
    // populate `cause` when the chain actually adds something beyond
    // `message`, so a single-frame error doesn't get a pointless duplicate —
    // and it gets the same local-root redaction `message` already does.
    let full_chain = redact_local_root(&format!("{err:#}"), root);
    let cause = (full_chain != message).then_some(full_chain);
    SourceError {
        code: "source.local.index_failed".to_string(),
        severity: Severity::Failed,
        message,
        source_item_key: None,
        retryable: false,
        provider_id: None,
        cause,
    }
}

#[cfg(test)]
#[path = "local_source_job_tests.rs"]
mod tests;

fn job_create_request(
    input: &LocalSourceIndexInput,
    priority: JobPriority,
    idempotency_key: Option<String>,
    attempt: u32,
) -> JobCreateRequest {
    JobCreateRequest {
        request_id: None,
        job_kind: JobKind::Source,
        job_intent: JobIntent::Run,
        // The job is created BEFORE the source row exists in `sources` (the
        // ledger upserts it during the run), so we cannot set source_id here —
        // `jobs.source_id` FKs to `sources(source_id)` and would fail at INSERT.
        // The column is nullable by contract; linking the job to its source
        // after `upsert_source` is a follow-up (see bd unified-sqlite bug).
        source_id: None,
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt,
        priority,
        idempotency_key,
        stage_plan: Vec::new(),
        // Wrap as `{"source_request": <..>}` — the shape the source worker
        // (`source_runner.rs::run`) requires. The previous
        // `{"source_kind": "local", "root_hint": ...}` shape had no
        // `source_request` key, so a locally-created job could never be
        // recovered or retried: the runner's claim path fails immediately
        // with "source job request is missing `source_request`" (finding
        // C2). Match every other family's `job_create_request`
        // (`source/non_web.rs`, `web_source/web_source_job.rs`).
        request: Some(serde_json::json!({ "source_request": local_source_request(input) })),
        auth_snapshot: input
            .auth_snapshot
            .clone()
            .unwrap_or_else(|| AuthSnapshot::trusted_system("runtime")),
        config_snapshot_id: Some(crate::config_snapshot_hash::config_snapshot_id(
            &crate::config_snapshot_hash::JobConfigSnapshot {
                source_kind: "local",
                source_ref: &public_path_hint(&input.root),
                collection: &input.collection,
                embedding_provider_id: &input.embedding_provider_id.0,
                vector_provider_id: &input.vector_provider_id.0,
                embedding_model: &input.embedding_model,
                embedding_dimensions: input.embedding_dimensions,
                embed: input.embed,
                max_items: None,
            },
        )),
        requirements: MetadataMap::new(),
        result_schema: Some("source_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
        deadline_at: None,
    }
}

/// Reconstruct a retryable `SourceRequest` for the job payload. Mirrors
/// `source/dispatch.rs::family_source_plan`'s ad-hoc reconstruction for the
/// generic non-web families: not necessarily byte-identical to whatever
/// caller-facing request originally routed here, but sufficient for
/// `source_runner.rs` to re-run this exact local index on retry/recovery.
/// When `input.route` carries the real routed plan (every `dispatch_local`
/// call sets it — see `LocalSourceIndexInput::route`'s doc comment), its
/// `scope`/`adapter`/`validated_options` are carried over; callers that
/// bypass routing (tests, `query/code_search_refresh.rs`) fall back to a
/// plain `SourceRequest::local_path` built from the root path.
fn local_source_request(input: &LocalSourceIndexInput) -> SourceRequest {
    let is_dir = input.root.is_dir();
    let mut request = SourceRequest::local_path(input.root.to_string_lossy().to_string(), is_dir);
    request.embed = input.embed;
    request.collection = Some(input.collection.clone());
    if let Some(route) = &input.route {
        request.scope = Some(route.scope);
        request.adapter = Some(route.adapter.name.clone());
        request.options = route.validated_options.clone();
    }
    request
}

fn public_path_hint(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| "local-source".to_string())
}

fn redact_local_root(message: &str, root: &Path) -> String {
    let mut redacted = message.to_string();
    let root_display = root.display().to_string();
    if !root_display.is_empty() {
        redacted = redacted.replace(&root_display, "<local-source-root>");
    }
    if let Ok(canonical) = std::fs::canonicalize(root) {
        let canonical_display = canonical.display().to_string();
        if !canonical_display.is_empty() {
            redacted = redacted.replace(&canonical_display, "<local-source-root>");
        }
    }
    redacted
}

struct JobProgressSink<'a> {
    jobs: &'a dyn JobStore,
    job_id: JobId,
    source_id: SourceId,
}

impl<'a> JobProgressSink<'a> {
    fn new(jobs: &'a dyn JobStore, job_id: JobId, source_id: SourceId) -> Self {
        Self {
            jobs,
            job_id,
            source_id,
        }
    }
}

#[async_trait]
impl LocalSourceProgress for JobProgressSink<'_> {
    async fn record_phase(
        &self,
        phase: PipelinePhase,
        status: LifecycleStatus,
        counts: Option<StageCounts>,
        error: Option<SourceError>,
        provider_reservations: Vec<ProviderReservationSnapshot>,
    ) -> anyhow::Result<()> {
        // Query the job's current latest event sequence instead of tracking
        // one locally. A sink-local counter (previously `Mutex<u64>` starting
        // at 0) is only correct when this sink is the very first writer for
        // `job_id` — true when local always created its own fresh job, but
        // no longer true once local dispatch can reuse a worker-claimed
        // parent job (finding C2 fix, `dispatch_local`): that job may already
        // carry events from the routing/authorizing stages emitted before
        // local dispatch even started, so a counter starting at 1 collides
        // with the store's real next-sequence expectation
        // (`job_event.sequence_invalid`). Mirrors
        // `source/events.rs::emit_source_event`, the pattern every other
        // family's event emission already uses.
        let sequence = self
            .jobs
            .latest_event_sequence(self.job_id)
            .await?
            .unwrap_or(0)
            + 1;
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
                message: Some(format!("local source {phase:?}").to_ascii_lowercase()),
                error: error.clone(),
            })
            .await?;
        let reservation_id = provider_reservations
            .first()
            .map(|reservation| reservation.reservation_id.clone());
        self.jobs
            .append_event(SourceProgressEvent {
                event_id: format!("evt_local_{}_{}", self.job_id.0, sequence),
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
                message: format!("local source {phase:?}").to_ascii_lowercase(),
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
                worker_id: Some("local-source".to_string()),
                phase,
                status,
                stage_id: None,
                heartbeat_at: timestamp(),
                sequence: 0,
                last_progress_at: None,
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

fn counts_for_output(output: &LocalSourceIndexOutput) -> StageCounts {
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
