use super::super::error::HttpError;
use crate::core::config::Config;
use crate::services::client_contract::RestAskRequest as AskRequestBody;
use crate::services::events::ServiceEvent;
use crate::services::query as query_svc;
use axum::{
    Extension, Json,
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use futures_util::stream;
use serde::Serialize;
use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::mpsc;

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AskStreamEvent {
    Meta {
        phase: &'static str,
    },
    Delta {
        text: String,
    },
    Done {
        result: Box<crate::services::types::AskResult>,
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

#[utoipa::path(
    post,
    path = "/v1/ask/stream",
    request_body = AskRequestBody,
    responses(
        (status = 200, description = "RAG answer streamed as server-sent events", body = String, content_type = "text/event-stream"),
        (status = 400, description = "Invalid ask request", body = crate::web::server::error::ErrorBody),
        (status = 413, description = "Ask request exceeds limits", body = crate::web::server::error::ErrorBody)
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

    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();
    let disconnected = Arc::new(AtomicBool::new(false));
    let mut req_cfg = (*cfg).clone();
    super::ask::apply_ask_overrides(&mut req_cfg, &req);
    req_cfg.ask_stream = true;
    req_cfg.json_output = false;

    tokio::spawn(async move {
        if tx
            .send(Ok(sse_json(
                "meta",
                &AskStreamEvent::Meta {
                    phase: "retrieving",
                },
            )))
            .is_err()
        {
            return;
        }

        let (event_tx, mut event_rx) = mpsc::channel::<ServiceEvent>(256);
        let delta_tx = tx.clone();
        let delta_disconnected = Arc::clone(&disconnected);
        let delta_task = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if delta_disconnected.load(Ordering::Relaxed) {
                    return;
                }
                if let ServiceEvent::SynthesisDelta { text } = event
                    && delta_tx
                        .send(Ok(sse_json("delta", &AskStreamEvent::Delta { text })))
                        .is_err()
                {
                    delta_disconnected.store(true, Ordering::Relaxed);
                    return;
                }
            }
        });
        let result = query_svc::ask(&req_cfg, &req.query, Some(event_tx))
            .await
            .map_err(|err| err.to_string());
        let _ = delta_task.await;

        if disconnected.load(Ordering::Relaxed) {
            return;
        }

        match result {
            Ok(result) => {
                if tx
                    .send(Ok(sse_json(
                        "done",
                        &AskStreamEvent::Done {
                            result: Box::new(result),
                        },
                    )))
                    .is_err()
                {
                    disconnected.store(true, Ordering::Relaxed);
                }
            }
            Err(message) => {
                if tx
                    .send(Ok(sse_json("error", &AskStreamEvent::Error { message })))
                    .is_err()
                {
                    disconnected.store(true, Ordering::Relaxed);
                }
            }
        }
    });

    let event_stream = stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|event| (event, rx))
    });
    Sse::new(event_stream).into_response()
}

#[cfg(test)]
pub(super) async fn v1_ask_stream_test_response(body: serde_json::Value) -> Response {
    let req = serde_json::from_value::<AskRequestBody>(body).expect("valid ask request");
    v1_ask_stream(Extension(Arc::new(Config::default())), Json(req)).await
}

#[cfg(test)]
#[path = "ask_stream_tests.rs"]
mod tests;
