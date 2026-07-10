use super::super::error::HttpError;
use axon_api::source::{
    ApiError, ErrorStage, JobId, LifecycleStatus, PipelinePhase, Severity, SourceProgressEvent,
    StreamEvent,
};
use axon_core::config::Config;
use axon_services::client_contract::RestAskRequest as AskRequestBody;
use axon_services::context::ServiceContext;
use axon_services::events::{LogLevel, ServiceEvent};
use axon_services::query as query_svc;
use axum::{
    Extension, Json,
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use futures_util::Stream;
use std::{
    convert::Infallible,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll},
};
use tokio::{sync::mpsc, task::JoinHandle};

const SSE_EVENT_BUFFER: usize = 32;

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

async fn send_stream_event(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    disconnected: &AtomicBool,
    event: &StreamEvent,
) -> bool {
    if tx.send(Ok(sse_json(event))).await.is_ok() {
        true
    } else {
        disconnected.store(true, Ordering::Relaxed);
        false
    }
}

fn spawn_service_event_forwarder(
    mut event_rx: mpsc::Receiver<ServiceEvent>,
    delta_tx: mpsc::Sender<Result<Event, Infallible>>,
    disconnected: Arc<AtomicBool>,
    sequence: Arc<SequenceCounter>,
    job_id: JobId,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if disconnected.load(Ordering::Relaxed) {
                return;
            }
            if !forward_service_event(&delta_tx, &disconnected, &sequence, job_id, event).await {
                return;
            }
        }
    })
}

fn ask_progress_event(
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

async fn forward_service_event(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    disconnected: &AtomicBool,
    sequence: &SequenceCounter,
    job_id: JobId,
    event: ServiceEvent,
) -> bool {
    match event {
        ServiceEvent::SynthesisDelta { text } => {
            let event = StreamEvent::token(sequence.next(), text).with_job_id(job_id);
            send_stream_event(tx, disconnected, &event).await
        }
        ServiceEvent::Activity {
            kind,
            label,
            detail,
        } => {
            let message = match detail {
                Some(detail) => format!("{label}: {detail}"),
                None => label,
            };
            let progress =
                ask_progress_event(job_id, sequence.next(), format!("[{kind}] {message}"));
            let event = StreamEvent::progress(progress.sequence, &progress).with_job_id(job_id);
            send_stream_event(tx, disconnected, &event).await
        }
        ServiceEvent::Log {
            level: LogLevel::Info,
            message,
        } => {
            let progress = ask_progress_event(job_id, sequence.next(), message);
            let event = StreamEvent::progress(progress.sequence, &progress).with_job_id(job_id);
            send_stream_event(tx, disconnected, &event).await
        }
        ServiceEvent::Log { .. } | ServiceEvent::EditorWrite { .. } => true,
    }
}

async fn emit_ask_stream_result(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    disconnected: &AtomicBool,
    sequence: &SequenceCounter,
    job_id: JobId,
    result: Result<axon_services::types::AskResult, String>,
) {
    match result {
        Ok(result) => {
            let event = StreamEvent::final_event(sequence.next(), &result).with_job_id(job_id);
            send_stream_event(tx, disconnected, &event).await;
        }
        Err(message) => {
            let message = axon_core::redact::redact_secrets(&message);
            axon_core::logging::log_warn(&format!("ask stream failed: {message}"));
            let error = ApiError::new(
                "ask.stream_failed",
                ErrorStage::Synthesizing,
                message.clone(),
            );
            let event = StreamEvent::error_event(sequence.next(), error).with_job_id(job_id);
            send_stream_event(tx, disconnected, &event).await;
        }
    }
}

#[utoipa::path(
    post,
    path = "/v1/ask/stream",
    request_body = AskRequestBody,
    responses(
        (status = 200, description = "RAG answer streamed as server-sent events", body = String, content_type = "text/event-stream"),
        (status = 400, description = "Invalid ask request", body = crate::server::error::ErrorBody),
        (status = 413, description = "Ask request exceeds limits", body = crate::server::error::ErrorBody)
    ),
    tag = "rag"
)]
pub async fn v1_ask_stream(
    Extension(cfg): Extension<Arc<Config>>,
    Extension(ctx): Extension<Arc<ServiceContext>>,
    Json(req): Json<AskRequestBody>,
) -> Response {
    use super::super::types::ASK_QUERY_MAX_CHARS;

    if req.query.trim().is_empty() {
        return HttpError::bad_request("query is required").into_response();
    }
    if req.query.chars().count() > ASK_QUERY_MAX_CHARS {
        return HttpError::payload_too_large(format!("query exceeds {ASK_QUERY_MAX_CHARS} chars"))
            .into_response();
    }
    if req.explain == Some(true) {
        return HttpError::bad_request("explain is not supported for streaming ask")
            .into_response();
    }

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(SSE_EVENT_BUFFER);
    let disconnected = Arc::new(AtomicBool::new(false));
    let mut req_cfg = axon_services::transport::apply_ask_overrides(
        &cfg,
        super::ask::ask_transport_overrides(&req),
    );
    if let Err(reason) = axon_core::config::validate_collection_name(&req_cfg.collection) {
        return HttpError::bad_request(format!("invalid collection name: {reason}"))
            .into_response();
    }
    req_cfg.ask_stream = true;
    req_cfg.json_output = false;

    let service_context = Arc::clone(&ctx);
    let job_id = JobId::new(uuid::Uuid::new_v4());
    let sequence = Arc::new(SequenceCounter::default());
    let handle = tokio::spawn(async move {
        let meta = ask_progress_event(job_id, sequence.next(), "retrieving");
        if !send_stream_event(
            &tx,
            &disconnected,
            &StreamEvent::progress(meta.sequence, &meta).with_job_id(job_id),
        )
        .await
        {
            return;
        }

        let (event_tx, event_rx) = mpsc::channel::<ServiceEvent>(256);
        let delta_task = spawn_service_event_forwarder(
            event_rx,
            tx.clone(),
            Arc::clone(&disconnected),
            Arc::clone(&sequence),
            job_id,
        );
        let result = query_svc::ask(&service_context, &req_cfg, &req.query, Some(event_tx))
            .await
            .map_err(|err| err.to_string());
        let _ = delta_task.await;

        if disconnected.load(Ordering::Relaxed) {
            return;
        }

        emit_ask_stream_result(&tx, &disconnected, &sequence, job_id, result).await;
    });

    let event_stream = AbortOnDropStream { rx, handle };
    Sse::new(event_stream).into_response()
}

#[cfg(test)]
pub(super) fn sse_event_buffer_for_tests() -> usize {
    SSE_EVENT_BUFFER
}

#[cfg(test)]
pub(super) fn bounded_stream_for_tests(
    rx: mpsc::Receiver<Result<Event, Infallible>>,
    handle: JoinHandle<()>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    AbortOnDropStream { rx, handle }
}

#[cfg(test)]
#[path = "ask_stream_tests.rs"]
mod tests;
