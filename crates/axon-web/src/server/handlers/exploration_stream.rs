use axon_api::source::{
    ApiError, ErrorStage, JobId, LifecycleStatus, PipelinePhase, Severity, SourceProgressEvent,
    StreamEvent,
};
use axon_services as services;
use axon_services::client_contract::{
    RestResearchRequest as ResearchRequest, RestSummarizeRequest as SummarizeRequest,
};
use axon_services::events::ServiceEvent;
use axum::response::{
    IntoResponse, Response,
    sse::{Event, Sse},
};
use axum::{Json, extract::State};
use futures_util::Stream;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::{sync::mpsc, task::JoinHandle};

use super::{WebState, search_options, summarize_config, summarize_request_urls};
use crate::server::handlers::rag::required_text;

const SSE_EVENT_BUFFER: usize = 32;
const STREAM_TIMEOUT: Duration = Duration::from_secs(35);

struct AbortOnDropStream {
    rx: mpsc::Receiver<Result<Event, Infallible>>,
    handle: JoinHandle<()>,
}

impl Stream for AbortOnDropStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx).poll_recv(cx)
    }
}

impl Drop for AbortOnDropStream {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Per-stream monotonic sequence counter, per the `StreamEvent.sequence`
/// contract ("event sequence is monotonic per job").
#[derive(Default)]
struct SequenceCounter(AtomicU64);

impl SequenceCounter {
    fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}

fn exploration_progress_event(
    job_id: JobId,
    sequence: u64,
    message: impl Into<String>,
) -> SourceProgressEvent {
    SourceProgressEvent::minimal(
        job_id,
        sequence,
        PipelinePhase::Synthesizing,
        LifecycleStatus::Running,
        Severity::Info,
        message,
    )
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

#[utoipa::path(
    post,
    path = "/v1/summarize/stream",
    request_body = SummarizeRequest,
    responses(
        (status = 200, description = "Brief LLM summary streamed as server-sent events", body = String, content_type = "text/event-stream"),
        (status = 400, description = "Invalid summarize request", body = crate::server::error::ErrorBody)
    ),
    tag = "exploration"
)]
pub(crate) async fn summarize_stream(
    State((_state, cfg)): State<WebState>,
    Json(req): Json<SummarizeRequest>,
) -> Response {
    let urls = match summarize_request_urls(&req) {
        Ok(urls) => urls,
        Err(err) => return err.into_response(),
    };
    let cfg = match summarize_config(&cfg, &req) {
        Ok(cfg) => cfg,
        Err(err) => return err.into_response(),
    };
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(SSE_EVENT_BUFFER);
    let disconnected = Arc::new(AtomicBool::new(false));
    let job_id = JobId::new(uuid::Uuid::new_v4());
    let sequence = Arc::new(SequenceCounter::default());

    let handle = tokio::spawn(async move {
        let meta = exploration_progress_event(job_id, sequence.next(), "summarizing");
        if tx
            .send(Ok(sse_json(
                &StreamEvent::progress(meta.sequence, &meta).with_job_id(job_id),
            )))
            .await
            .is_err()
        {
            disconnected.store(true, Ordering::Relaxed);
            return;
        }
        let (event_tx, mut event_rx) = mpsc::channel::<ServiceEvent>(256);
        let delta_tx = tx.clone();
        let delta_disconnected = Arc::clone(&disconnected);
        let delta_sequence = Arc::clone(&sequence);
        let delta_task = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if delta_disconnected.load(Ordering::Relaxed) {
                    return;
                }
                if let ServiceEvent::SynthesisDelta { text } = event {
                    let event = StreamEvent::token(delta_sequence.next(), text).with_job_id(job_id);
                    if delta_tx.send(Ok(sse_json(&event))).await.is_err() {
                        delta_disconnected.store(true, Ordering::Relaxed);
                        return;
                    }
                }
            }
        });
        let result = services::summarize::summarize(&cfg, &urls, Some(event_tx))
            .await
            .map_err(|err| err.to_string());
        let _ = delta_task.await;
        if disconnected.load(Ordering::Relaxed) {
            return;
        }
        match result {
            Ok(result) => {
                let event = StreamEvent::final_event(sequence.next(), &result).with_job_id(job_id);
                let _ = tx.send(Ok(sse_json(&event))).await;
            }
            Err(message) => {
                let error =
                    ApiError::new("summarize.stream_failed", ErrorStage::Synthesizing, message);
                let event = StreamEvent::error_event(sequence.next(), error).with_job_id(job_id);
                let _ = tx.send(Ok(sse_json(&event))).await;
            }
        }
    });

    let event_stream = AbortOnDropStream { rx, handle };
    Sse::new(event_stream).into_response()
}

#[cfg(test)]
pub(super) fn sse_event_buffer_for_tests() -> usize {
    SSE_EVENT_BUFFER
}

#[cfg(test)]
pub(super) fn summarize_stream_timeout_for_tests() -> Option<Duration> {
    None
}

#[cfg(test)]
pub(super) fn research_stream_timeout_for_tests() -> Duration {
    STREAM_TIMEOUT
}

#[cfg(test)]
pub(super) fn bounded_stream_for_tests(
    rx: mpsc::Receiver<Result<Event, Infallible>>,
    handle: JoinHandle<()>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    AbortOnDropStream { rx, handle }
}

#[utoipa::path(
    post,
    path = "/v1/research/stream",
    request_body = ResearchRequest,
    responses(
        (status = 200, description = "Research synthesis streamed as server-sent events", body = String, content_type = "text/event-stream"),
        (status = 400, description = "Invalid research request", body = crate::server::error::ErrorBody)
    ),
    tag = "exploration"
)]
pub(crate) async fn research_stream(
    State((state, cfg)): State<WebState>,
    Json(req): Json<ResearchRequest>,
) -> Response {
    let query = match required_text(&req.query, "query") {
        Ok(query) => query.to_string(),
        Err(err) => return err.into_response(),
    };
    let opts = match search_options(&cfg, req.limit, req.offset, req.time_range.as_deref()) {
        Ok(opts) => opts,
        Err(err) => return err.into_response(),
    };
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(SSE_EVENT_BUFFER);
    let disconnected = Arc::new(AtomicBool::new(false));
    let service_context = Arc::clone(&state.service_context);
    let job_id = JobId::new(uuid::Uuid::new_v4());
    let sequence = Arc::new(SequenceCounter::default());

    let handle = tokio::spawn(async move {
        let meta = exploration_progress_event(job_id, sequence.next(), "researching");
        if tx
            .send(Ok(sse_json(
                &StreamEvent::progress(meta.sequence, &meta).with_job_id(job_id),
            )))
            .await
            .is_err()
        {
            disconnected.store(true, Ordering::Relaxed);
            return;
        }
        let (event_tx, mut event_rx) = mpsc::channel::<ServiceEvent>(256);
        let delta_tx = tx.clone();
        let delta_disconnected = Arc::clone(&disconnected);
        let delta_sequence = Arc::clone(&sequence);
        let delta_task = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if delta_disconnected.load(Ordering::Relaxed) {
                    return;
                }
                if let ServiceEvent::SynthesisDelta { text } = event {
                    let event = StreamEvent::token(delta_sequence.next(), text).with_job_id(job_id);
                    if delta_tx.send(Ok(sse_json(&event))).await.is_err() {
                        delta_disconnected.store(true, Ordering::Relaxed);
                        return;
                    }
                }
            }
        });
        let result = tokio::time::timeout(
            STREAM_TIMEOUT,
            services::search::research_with_context(
                &cfg,
                &service_context,
                &query,
                opts,
                Some(event_tx),
            ),
        )
        .await
        .map_err(|_| "stream timed out".to_string())
        .and_then(|result| {
            result
                .map(|result| result.payload)
                .map_err(|err| err.to_string())
        });
        let _ = delta_task.await;
        if disconnected.load(Ordering::Relaxed) {
            return;
        }
        match result {
            Ok(result) => {
                let event = StreamEvent::final_event(sequence.next(), &result).with_job_id(job_id);
                let _ = tx.send(Ok(sse_json(&event))).await;
            }
            Err(message) => {
                let error =
                    ApiError::new("research.stream_failed", ErrorStage::Synthesizing, message);
                let event = StreamEvent::error_event(sequence.next(), error).with_job_id(job_id);
                let _ = tx.send(Ok(sse_json(&event))).await;
            }
        }
    });

    let event_stream = AbortOnDropStream { rx, handle };
    Sse::new(event_stream).into_response()
}

#[cfg(test)]
#[path = "exploration_stream_tests.rs"]
mod tests;
