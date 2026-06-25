use super::super::error::HttpError;
use axon_core::config::Config;
use axon_services::client_contract::RestAskRequest as AskRequestBody;
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
use serde::Serialize;
use std::{
    convert::Infallible,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
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

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AskStreamEvent {
    Meta {
        phase: &'static str,
    },
    Activity {
        kind: String,
        label: String,
        detail: Option<String>,
    },
    Delta {
        text: String,
    },
    Done {
        result: Box<axon_services::types::AskResult>,
    },
    Error {
        message: String,
    },
}

fn sse_json(event_name: &'static str, value: &AskStreamEvent) -> Event {
    Event::default()
        .event(event_name)
        .json_data(value)
        .unwrap_or_else(|_| {
            Event::default()
                .event("error")
                .data("{\"type\":\"error\",\"message\":\"encode failed\"}")
        })
}

async fn send_stream_event(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    disconnected: &AtomicBool,
    event_name: &'static str,
    event: &AskStreamEvent,
) -> bool {
    if tx.send(Ok(sse_json(event_name, event))).await.is_ok() {
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
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if disconnected.load(Ordering::Relaxed) {
                return;
            }
            if !forward_service_event(&delta_tx, &disconnected, event).await {
                return;
            }
        }
    })
}

async fn forward_service_event(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    disconnected: &AtomicBool,
    event: ServiceEvent,
) -> bool {
    match event {
        ServiceEvent::SynthesisDelta { text } => {
            send_stream_event(tx, disconnected, "delta", &AskStreamEvent::Delta { text }).await
        }
        ServiceEvent::Activity {
            kind,
            label,
            detail,
        } => {
            send_stream_event(
                tx,
                disconnected,
                "activity",
                &AskStreamEvent::Activity {
                    kind,
                    label,
                    detail,
                },
            )
            .await
        }
        ServiceEvent::Log {
            level: LogLevel::Info,
            message,
        } => {
            send_stream_event(
                tx,
                disconnected,
                "activity",
                &AskStreamEvent::Activity {
                    kind: "thinking".to_string(),
                    label: message,
                    detail: None,
                },
            )
            .await
        }
        ServiceEvent::Log { .. } | ServiceEvent::EditorWrite { .. } => true,
    }
}

async fn emit_ask_stream_result(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    disconnected: &AtomicBool,
    result: Result<axon_services::types::AskResult, String>,
) {
    match result {
        Ok(result) => {
            send_stream_event(
                tx,
                disconnected,
                "done",
                &AskStreamEvent::Done {
                    result: Box::new(result),
                },
            )
            .await;
        }
        Err(message) => {
            let message = axon_core::redact::redact_secrets(&message);
            axon_core::logging::log_warn(&format!("ask stream failed: {message}"));
            send_stream_event(
                tx,
                disconnected,
                "error",
                &AskStreamEvent::Error { message },
            )
            .await;
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

    let handle = tokio::spawn(async move {
        if !send_stream_event(
            &tx,
            &disconnected,
            "meta",
            &AskStreamEvent::Meta {
                phase: "retrieving",
            },
        )
        .await
        {
            return;
        }

        let (event_tx, event_rx) = mpsc::channel::<ServiceEvent>(256);
        let delta_task =
            spawn_service_event_forwarder(event_rx, tx.clone(), Arc::clone(&disconnected));
        let result = query_svc::ask(&req_cfg, &req.query, Some(event_tx))
            .await
            .map_err(|err| err.to_string());
        let _ = delta_task.await;

        if disconnected.load(Ordering::Relaxed) {
            return;
        }

        emit_ask_stream_result(&tx, &disconnected, result).await;
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
pub(super) async fn v1_ask_stream_test_response(body: serde_json::Value) -> Response {
    let req = serde_json::from_value::<AskRequestBody>(body).expect("valid ask request");
    v1_ask_stream(Extension(Arc::new(Config::default())), Json(req)).await
}

#[cfg(test)]
#[path = "ask_stream_tests.rs"]
mod tests;
