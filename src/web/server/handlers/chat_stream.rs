use crate::core::config::Config;
use crate::services::{client_contract::RestChatRequest, llm_backend};
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
enum ChatStreamEvent {
    Meta { phase: &'static str },
    Delta { text: String },
    Done { answer: String },
    Error { message: String },
}

fn sse_json(event_name: &'static str, value: &ChatStreamEvent) -> Event {
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
    path = "/v1/chat/stream",
    request_body = RestChatRequest,
    responses(
        (status = 200, description = "Direct LLM chat answer streamed as server-sent events", body = String, content_type = "text/event-stream"),
        (status = 400, description = "Invalid chat request", body = crate::web::server::error::ErrorBody),
        (status = 413, description = "Chat request exceeds limits", body = crate::web::server::error::ErrorBody)
    ),
    tag = "rag"
)]
pub async fn v1_chat_stream(
    Extension(cfg): Extension<Arc<Config>>,
    Json(req): Json<RestChatRequest>,
) -> Response {
    if let Err(err) = super::chat::validate_chat_message(&req.message) {
        return err.into_response();
    }

    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();
    let disconnected = Arc::new(AtomicBool::new(false));
    let req_cfg = (*cfg).clone();

    tokio::spawn(async move {
        if tx
            .send(Ok(sse_json(
                "meta",
                &ChatStreamEvent::Meta { phase: "chatting" },
            )))
            .is_err()
        {
            return;
        }

        let delta_disconnected = Arc::clone(&disconnected);
        let delta_tx = tx.clone();
        let request = super::chat::completion_request(&req_cfg, &req.message, true);
        let result = llm_backend::complete_streaming(request, move |text| {
            if delta_disconnected.load(Ordering::Relaxed) {
                return Ok(());
            }
            if delta_tx
                .send(Ok(sse_json(
                    "delta",
                    &ChatStreamEvent::Delta {
                        text: text.to_string(),
                    },
                )))
                .is_err()
            {
                delta_disconnected.store(true, Ordering::Relaxed);
            }
            Ok(())
        })
        .await
        .map_err(|err| err.to_string());

        if disconnected.load(Ordering::Relaxed) {
            return;
        }

        match result {
            Ok(completion) => {
                if tx
                    .send(Ok(sse_json(
                        "done",
                        &ChatStreamEvent::Done {
                            answer: completion.text,
                        },
                    )))
                    .is_err()
                {
                    disconnected.store(true, Ordering::Relaxed);
                }
            }
            Err(message) => {
                if tx
                    .send(Ok(sse_json("error", &ChatStreamEvent::Error { message })))
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
pub(super) async fn v1_chat_stream_test_response(body: serde_json::Value) -> Response {
    let req = serde_json::from_value::<RestChatRequest>(body).expect("valid chat request");
    v1_chat_stream(Extension(Arc::new(Config::default())), Json(req)).await
}

#[cfg(test)]
#[path = "chat_stream_tests.rs"]
mod tests;
