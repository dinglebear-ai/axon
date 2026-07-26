use axon_api::source::{
    ApiError, ErrorStage, JobId, LifecycleStatus, PipelinePhase, Severity, SourceProgressEvent,
    StreamEvent,
};
use axon_services::client_contract::{RestChatRequest, RestChatResponse};
use axon_services::context::ServiceContext;
use axon_services::service_traits::{
    AskService, AskServiceImpl,
    ask_service::{ChatDeltaHandler, ChatRequest, ChatResult},
};
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
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll},
};
use tokio::{sync::mpsc, task::JoinHandle};

const SSE_EVENT_BUFFER: usize = 32;

/// Per-stream monotonic sequence counter, per the `StreamEvent.sequence`
/// contract ("event sequence is monotonic per job").
#[derive(Default)]
struct SequenceCounter(AtomicU64);

impl SequenceCounter {
    fn next(&self) -> u64 {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}

fn chat_progress_event(
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

type ChatStreamingFuture = Pin<Box<dyn Future<Output = Result<ChatResult, String>> + Send>>;
type ChatStreamingFn = Box<dyn FnOnce(ChatRequest, ChatDeltaHandler) -> ChatStreamingFuture + Send>;

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
    path = "/v1/chat/stream",
    request_body = RestChatRequest,
    responses(
        (status = 200, description = "Direct LLM chat answer streamed as server-sent events", body = String, content_type = "text/event-stream"),
        (status = 400, description = "Invalid chat request", body = crate::server::error::ErrorBody),
        (status = 413, description = "Chat request exceeds limits", body = crate::server::error::ErrorBody)
    ),
    tag = "rag"
)]
pub async fn v1_chat_stream(
    Extension(context): Extension<Arc<ServiceContext>>,
    Json(req): Json<RestChatRequest>,
) -> Response {
    if let Err(err) = super::chat::validate_chat_message(&req.message) {
        return err.into_response();
    }

    let chat_streaming: ChatStreamingFn = Box::new(move |request, on_delta| {
        Box::pin(async move {
            AskServiceImpl::new(context)
                .chat_stream(request, on_delta)
                .await
                .map_err(|error| error.to_string())
        })
    });
    v1_chat_stream_response(req, chat_streaming)
}

fn v1_chat_stream_response(req: RestChatRequest, chat_streaming: ChatStreamingFn) -> Response {
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(SSE_EVENT_BUFFER);
    let disconnected = Arc::new(AtomicBool::new(false));
    let job_id = JobId::new(uuid::Uuid::new_v4());
    let sequence = Arc::new(SequenceCounter::default());

    let handle = tokio::spawn(async move {
        let meta = chat_progress_event(job_id, sequence.next(), "chatting");
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

        let delta_disconnected = Arc::clone(&disconnected);
        let delta_tx = tx.clone();
        let delta_sequence = Arc::clone(&sequence);
        let message = req.message.clone();
        let on_delta: ChatDeltaHandler = Box::new(move |text| {
            if delta_disconnected.load(Ordering::Relaxed) {
                return Ok(());
            }
            let event = StreamEvent::token(delta_sequence.next(), text).with_job_id(job_id);
            match delta_tx.try_send(Ok(sse_json(&event))) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    return Err("stream backpressure exceeded".into());
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    delta_disconnected.store(true, Ordering::Relaxed);
                }
            }
            Ok(())
        });
        let result = chat_streaming(
            ChatRequest {
                session_id: None,
                message: req.message,
            },
            on_delta,
        )
        .await;

        if disconnected.load(Ordering::Relaxed) {
            return;
        }

        match result {
            Ok(completion) => {
                let response = RestChatResponse {
                    message,
                    answer: completion.reply,
                    model: completion.model,
                };
                let event =
                    StreamEvent::final_event(sequence.next(), &response).with_job_id(job_id);
                if tx.send(Ok(sse_json(&event))).await.is_err() {
                    disconnected.store(true, Ordering::Relaxed);
                }
            }
            Err(message) => {
                let error = ApiError::new("chat.stream_failed", ErrorStage::Synthesizing, message);
                let event = StreamEvent::error_event(sequence.next(), error).with_job_id(job_id);
                if tx.send(Ok(sse_json(&event))).await.is_err() {
                    disconnected.store(true, Ordering::Relaxed);
                }
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
pub(super) fn bounded_stream_for_tests(
    rx: mpsc::Receiver<Result<Event, Infallible>>,
    handle: JoinHandle<()>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    AbortOnDropStream { rx, handle }
}

#[cfg(test)]
pub(super) async fn v1_chat_stream_test_response(body: serde_json::Value) -> Response {
    let req = serde_json::from_value::<RestChatRequest>(body).expect("valid chat request");
    if let Err(err) = super::chat::validate_chat_message(&req.message) {
        return err.into_response();
    }
    v1_chat_stream_response(
        req,
        Box::new(|request, _on_delta| {
            Box::pin(async move {
                Ok(ChatResult {
                    session_id: "test-session".to_string(),
                    reply: request.message,
                    model: Some("test-model".to_string()),
                })
            })
        }),
    )
}

#[cfg(test)]
pub(super) async fn v1_chat_stream_test_response_with_service(
    body: serde_json::Value,
    chat_streaming: ChatStreamingFn,
) -> Response {
    let req = serde_json::from_value::<RestChatRequest>(body).expect("valid chat request");
    v1_chat_stream_response(req, chat_streaming)
}

#[cfg(test)]
#[path = "chat_stream_tests.rs"]
mod tests;
