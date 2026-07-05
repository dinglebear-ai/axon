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

fn sse_json(event_name: &'static str, value: &StreamEvent) -> Event {
    Event::default()
        .event(event_name)
        .json_data(value)
        .unwrap_or_else(|_| {
            Event::default()
                .event("error")
                .data("{\"kind\":\"error\",\"message\":\"encode failed\"}")
        })
}

fn source_progress_from_job_event(event: JobEvent) -> SourceProgressEvent {
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
                        if tx
                            .send(Ok(sse_json(
                                "progress",
                                &StreamEvent::Progress {
                                    event: source_progress_from_job_event(event),
                                },
                            )))
                            .await
                            .is_err()
                        {
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
                    let event = StreamEvent::Error {
                        error: axon_api::source::ApiError::new(
                            "jobs.stream_error",
                            axon_api::source::ErrorStage::Routing,
                            error.to_string(),
                        ),
                    };
                    let _ = tx.send(Ok(sse_json("error", &event))).await;
                    break;
                }
            }
            tokio::time::sleep(JOB_STREAM_POLL).await;
        }
    });
    Sse::new(AbortOnDropJobStream { rx, handle }).into_response()
}
