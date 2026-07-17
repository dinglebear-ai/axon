use axon_api::source::{
    JobEvent, JobEventListRequest, LifecycleStatus, SourceProgressEvent, StageCounts, StreamEvent,
};
use axon_services as services;
use axon_services::context::ServiceContext;
use axum::{
    Extension,
    extract::Path,
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use futures_util::Stream;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::{sync::mpsc, task::JoinHandle};
use uuid::Uuid;

use super::jobs::UnifiedJobsState;

const JOB_STREAM_BUFFER: usize = 32;
const JOB_STREAM_POLL: Duration = Duration::from_secs(2);
const JOB_STREAM_PAGE_LIMIT: u32 = 100;

struct AbortOnDropJobStream {
    rx: mpsc::Receiver<Result<Event, Infallible>>,
    handle: JoinHandle<()>,
}

impl Stream for AbortOnDropJobStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx).poll_recv(cx)
    }
}

impl Drop for AbortOnDropJobStream {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

fn event_name(event: &StreamEvent) -> &'static str {
    match event.kind {
        axon_api::source::StreamKind::Progress => "progress",
        axon_api::source::StreamKind::Token => "delta",
        axon_api::source::StreamKind::Citation => "citation",
        axon_api::source::StreamKind::Artifact => "artifact",
        axon_api::source::StreamKind::Warning => "warning",
        axon_api::source::StreamKind::Error => "error",
        axon_api::source::StreamKind::Final => "done",
    }
}

fn sse_json(event: &StreamEvent) -> Event {
    Event::default()
        .event(event_name(event))
        .json_data(event)
        .unwrap_or_else(|_| {
            Event::default()
                .event("error")
                .data("{\"kind\":\"error\",\"data\":{},\"message\":\"encode failed\"}")
        })
}

fn source_progress_from_job_event(event: JobEvent) -> SourceProgressEvent {
    if let Some(mut progress) = event
        .details
        .get("source_progress_event")
        .cloned()
        .and_then(|value| serde_json::from_value::<SourceProgressEvent>(value).ok())
    {
        progress.event_id = event.event_id;
        progress.sequence = event.sequence;
        progress.job_id = event.job_id;
        progress.attempt = event.attempt;
        progress.stage_id = event.stage_id.or(progress.stage_id);
        progress.phase = event.phase;
        progress.status = event.status;
        progress.severity = event.severity;
        progress.visibility = event.visibility;
        progress.message = event.message;
        progress.timestamp = event.timestamp;
        return progress;
    }

    SourceProgressEvent {
        event_id: event.event_id,
        sequence: event.sequence,
        job_id: event.job_id,
        attempt: event.attempt,
        stage_id: event.stage_id,
        batch_id: None,
        reservation_id: None,
        checkpoint_id: None,
        dedupe_key: None,
        phase: event.phase,
        status: event.status,
        severity: event.severity,
        visibility: event.visibility,
        message: event.message,
        timestamp: event.timestamp,
        source_id: None,
        canonical_uri: None,
        adapter: None,
        scope: None,
        generation: None,
        counts: StageCounts {
            items_total: None,
            items_done: 0,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        timing: None,
        current: None,
        throughput: None,
        retry: None,
        warning: None,
        error: None,
    }
}

async fn terminal_job_status(
    service_context: &ServiceContext,
    job_id: axon_api::source::JobId,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let Some(summary) = services::jobs::unified_job_status(service_context, job_id).await? else {
        return Ok(true);
    };
    Ok(matches!(
        summary.status,
        LifecycleStatus::Completed
            | LifecycleStatus::CompletedDegraded
            | LifecycleStatus::Failed
            | LifecycleStatus::Canceled
            | LifecycleStatus::Expired
            | LifecycleStatus::Skipped
    ))
}

#[utoipa::path(
    get,
    path = "/v1/jobs/{id}/stream",
    params(("id" = uuid::Uuid, Path, description = "Unified job ID")),
    responses((status = 200, description = "Unified job events streamed as server-sent events", body = StreamEvent, content_type = "text/event-stream")),
    tag = "jobs"
)]
pub(crate) async fn unified_job_stream(
    Extension(state): Extension<UnifiedJobsState>,
    Path(id): Path<Uuid>,
) -> Response {
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(JOB_STREAM_BUFFER);
    let service_context = Arc::clone(&state.service_context);
    let job_id = axon_api::source::JobId::new(id);
    let handle = tokio::spawn(async move {
        let mut after_sequence = None;
        loop {
            let page = services::jobs::unified_job_events(
                &service_context,
                JobEventListRequest {
                    job_id,
                    after_sequence,
                    limit: Some(JOB_STREAM_PAGE_LIMIT),
                    severity: None,
                    visibility: None,
                    phase: None,
                    since_sequence: None,
                    cursor: None,
                },
            )
            .await;

            match page {
                Ok(page) => {
                    for event in page.events {
                        after_sequence = Some(event.sequence);
                        let sequence = event.sequence;
                        let progress = source_progress_from_job_event(event);
                        let stream_event =
                            StreamEvent::progress(sequence, &progress).with_job_id(job_id);
                        if tx.send(Ok(sse_json(&stream_event))).await.is_err() {
                            return;
                        }
                    }
                    if terminal_job_status(&service_context, job_id)
                        .await
                        .unwrap_or(false)
                    {
                        break;
                    }
                }
                Err(error) => {
                    let sequence = after_sequence.map(|s| s + 1).unwrap_or(0);
                    let api_error = axon_api::source::ApiError::new(
                        "jobs.stream_error",
                        axon_api::source::ErrorStage::Routing,
                        error.to_string(),
                    );
                    let event = StreamEvent::error_event(sequence, api_error).with_job_id(job_id);
                    let _ = tx.send(Ok(sse_json(&event))).await;
                    break;
                }
            }
            tokio::time::sleep(JOB_STREAM_POLL).await;
        }
    });
    Sse::new(AbortOnDropJobStream { rx, handle }).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axon_api::source::{
        ApiError, ErrorStage, JobId, MetadataMap, PipelinePhase, ProgressCurrent, Severity,
        SourceItemKey, Timestamp, Visibility,
    };

    #[test]
    fn source_progress_from_job_event_preserves_stored_counts_current_and_error() {
        let job_id = JobId::new(Uuid::from_u128(42));
        let mut stored = SourceProgressEvent::minimal(
            job_id,
            7,
            PipelinePhase::Embedding,
            LifecycleStatus::Running,
            Severity::Info,
            "stored message",
        );
        stored.counts = StageCounts {
            items_total: Some(5),
            items_done: 3,
            documents_total: Some(4),
            documents_done: 2,
            chunks_total: Some(9),
            chunks_done: 8,
            bytes_total: Some(100),
            bytes_done: 64,
        };
        stored.current = Some(ProgressCurrent {
            source_item_key: Some(SourceItemKey::from("item-3")),
            document_id: None,
            chunk_id: None,
            adapter: Some("web".to_string()),
            provider: None,
            message: Some("current item".to_string()),
        });
        stored.error = Some(ApiError::new(
            "provider.cooling",
            ErrorStage::Embedding,
            "cooling down",
        ));

        let mut details = MetadataMap::new();
        details.insert(
            "source_progress_event".to_string(),
            serde_json::to_value(stored).expect("progress json"),
        );

        let event = JobEvent {
            event_id: "row-event".to_string(),
            sequence: 11,
            job_id,
            attempt: 2,
            stage_id: None,
            phase: PipelinePhase::Embedding,
            status: LifecycleStatus::Running,
            severity: Severity::Info,
            visibility: Visibility::Public,
            message: "row message".to_string(),
            timestamp: Timestamp("2026-07-15T00:00:00Z".to_string()),
            details,
        };

        let progress = source_progress_from_job_event(event);
        assert_eq!(progress.sequence, 11);
        assert_eq!(progress.counts.items_total, Some(5));
        assert_eq!(progress.counts.chunks_done, 8);
        assert_eq!(
            progress
                .current
                .as_ref()
                .and_then(|current| current.message.as_deref()),
            Some("current item")
        );
        assert_eq!(
            progress.error.as_ref().map(|error| error.code.0.as_str()),
            Some("provider.cooling")
        );
    }
}
