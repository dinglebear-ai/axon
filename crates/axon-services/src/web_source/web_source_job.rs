//! Job-row wrapper for the web source bridge.
//!
//! Unlike git/local, [`index_web_source`](super::index_web_source) takes a
//! caller-supplied `job_id` and does no job bookkeeping of its own. This thin
//! wrapper creates a `JobKind::Source` row (mirroring the git bridge's
//! `index_git_source_with_job`), threads its id into the input, runs the
//! indexing pass, and marks the job terminal so `axon status`/progress readers
//! see web-source indexing exactly like git/local indexing.

use axon_api::source::*;
use axon_embedding::provider::EmbeddingProvider;
use axon_jobs::boundary::JobStore;
use axon_ledger::store::LedgerStore;
use axon_vectors::store::VectorStore;

use super::{WebSourceIndexInput, WebSourceIndexOutput, index_web_source};

/// Create a source job row, index the web source under it, and record terminal
/// job status. The crawl that produced `input.manifest_path`/`markdown_root`
/// must have already run to completion.
pub async fn index_web_source_with_job(
    mut input: WebSourceIndexInput,
    jobs: &dyn JobStore,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
) -> anyhow::Result<WebSourceIndexOutput> {
    let descriptor = jobs.create(job_create_request(&input)).await?;
    input.job_id = descriptor.job_id;

    // Transition Queued -> Running before indexing; the state machine rejects a
    // direct Queued -> Completed. The other families run through a JobProgressSink
    // that performs this transition; the web bridge sets status directly, so do
    // it explicitly here.
    jobs.update_status(JobStatusUpdate {
        job_id: input.job_id,
        source_id: None,
        status: LifecycleStatus::Running,
        phase: PipelinePhase::Preparing,
        stage_id: None,
        counts: None,
        current: None,
        message: Some("web source indexing".to_string()),
        error: None,
    })
    .await?;

    match index_web_source(input.clone(), ledger, embedding_provider, vector_store).await {
        Ok(output) => {
            record_terminal_status(
                jobs,
                input.job_id,
                LifecycleStatus::Completed,
                Some(counts_for_output(&output)),
                None,
            )
            .await?;
            Ok(output)
        }
        Err(err) => {
            let source_error = terminal_source_error(&err);
            if let Err(status_err) = record_terminal_status(
                jobs,
                input.job_id,
                LifecycleStatus::Failed,
                None,
                Some(source_error),
            )
            .await
            {
                return Err(err.context(format!(
                    "also failed to record terminal web source job failure: {status_err}"
                )));
            }
            Err(err)
        }
    }
}

fn job_create_request(input: &WebSourceIndexInput) -> JobCreateRequest {
    JobCreateRequest {
        request_id: None,
        job_kind: JobKind::Source,
        job_intent: JobIntent::Run,
        source_id: None,
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        priority: JobPriority::Background,
        idempotency_key: None,
        stage_plan: Vec::new(),
        request: Some(serde_json::json!({
            "source_kind": "web",
            "source": input.source,
            "scope": format!("{:?}", input.scope).to_ascii_lowercase(),
        })),
        auth_snapshot: MetadataMap::new(),
        config_snapshot_id: Some(ConfigSnapshotId::new("cfg_web_source")),
        requirements: MetadataMap::new(),
        result_schema: Some("source_result".to_string()),
        metadata: MetadataMap::new(),
    }
}

async fn record_terminal_status(
    jobs: &dyn JobStore,
    job_id: JobId,
    status: LifecycleStatus,
    counts: Option<StageCounts>,
    error: Option<SourceError>,
) -> anyhow::Result<()> {
    jobs.update_status(JobStatusUpdate {
        job_id,
        source_id: None,
        status,
        phase: PipelinePhase::Complete,
        stage_id: None,
        counts,
        current: None,
        message: Some("web source complete".to_string()),
        error,
    })
    .await?;
    Ok(())
}

fn terminal_source_error(err: &anyhow::Error) -> SourceError {
    let message = err.to_string();
    SourceError {
        code: "source.web.index_failed".to_string(),
        severity: Severity::Failed,
        message: message.clone(),
        source_item_key: None,
        retryable: false,
        provider_id: None,
        cause: Some(message),
    }
}

fn counts_for_output(output: &WebSourceIndexOutput) -> StageCounts {
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
