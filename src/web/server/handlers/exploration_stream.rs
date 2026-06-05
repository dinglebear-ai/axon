use crate::services;
use crate::services::client_contract::{
    RestResearchRequest as ResearchRequest, RestSummarizeRequest as SummarizeRequest,
};
use crate::services::events::ServiceEvent;
use axum::response::{
    IntoResponse, Response,
    sse::{Event, Sse},
};
use axum::{Json, extract::State};
use futures_util::stream;
use serde::Serialize;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

use super::{WebState, search_options, summarize_config, summarize_request_urls};
use crate::web::server::handlers::rag::required_text;

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum LlmStreamEvent<T: Serialize> {
    Meta { phase: &'static str },
    Delta { text: String },
    Done { result: T },
    Error { message: String },
}

fn sse_json<T: Serialize>(event_name: &'static str, value: &LlmStreamEvent<T>) -> Event {
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
    path = "/v1/summarize/stream",
    request_body = SummarizeRequest,
    responses(
        (status = 200, description = "Brief LLM summary streamed as server-sent events", body = String, content_type = "text/event-stream"),
        (status = 400, description = "Invalid summarize request", body = crate::web::server::error::ErrorBody)
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
    let cfg = summarize_config(&cfg, &req);
    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();
    let disconnected = Arc::new(AtomicBool::new(false));

    tokio::spawn(async move {
        if tx
            .send(Ok(sse_json(
                "meta",
                &LlmStreamEvent::<services::types::SummarizeResult>::Meta {
                    phase: "summarizing",
                },
            )))
            .is_err()
        {
            disconnected.store(true, Ordering::Relaxed);
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
                        .send(Ok(sse_json(
                            "delta",
                            &LlmStreamEvent::<services::types::SummarizeResult>::Delta { text },
                        )))
                        .is_err()
                {
                    delta_disconnected.store(true, Ordering::Relaxed);
                    return;
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
                let _ = tx.send(Ok(sse_json("done", &LlmStreamEvent::Done { result })));
            }
            Err(message) => {
                let _ = tx.send(Ok(sse_json(
                    "error",
                    &LlmStreamEvent::<services::types::SummarizeResult>::Error { message },
                )));
            }
        }
    });

    let event_stream = stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|event| (event, rx))
    });
    Sse::new(event_stream).into_response()
}

#[utoipa::path(
    post,
    path = "/v1/research/stream",
    request_body = ResearchRequest,
    responses(
        (status = 200, description = "Research synthesis streamed as server-sent events", body = String, content_type = "text/event-stream"),
        (status = 400, description = "Invalid research request", body = crate::web::server::error::ErrorBody)
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
    let opts = match search_options(req.limit, req.offset, req.time_range.as_deref()) {
        Ok(opts) => opts,
        Err(err) => return err.into_response(),
    };
    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();
    let disconnected = Arc::new(AtomicBool::new(false));
    let service_context = Arc::clone(&state.service_context);

    tokio::spawn(async move {
        let _ = tx.send(Ok(sse_json(
            "meta",
            &LlmStreamEvent::<services::types::ResearchPayload>::Meta {
                phase: "researching",
            },
        )));
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
                        .send(Ok(sse_json(
                            "delta",
                            &LlmStreamEvent::<services::types::ResearchPayload>::Delta { text },
                        )))
                        .is_err()
                {
                    delta_disconnected.store(true, Ordering::Relaxed);
                    return;
                }
            }
        });
        let result = services::search::research_with_context(
            &cfg,
            &service_context,
            &query,
            opts,
            Some(event_tx),
        )
        .await
        .map(|result| result.payload)
        .map_err(|err| err.to_string());
        let _ = delta_task.await;
        if disconnected.load(Ordering::Relaxed) {
            return;
        }
        match result {
            Ok(result) => {
                let _ = tx.send(Ok(sse_json("done", &LlmStreamEvent::Done { result })));
            }
            Err(message) => {
                let _ = tx.send(Ok(sse_json(
                    "error",
                    &LlmStreamEvent::<services::types::ResearchPayload>::Error { message },
                )));
            }
        }
    });

    let event_stream = stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|event| (event, rx))
    });
    Sse::new(event_stream).into_response()
}
