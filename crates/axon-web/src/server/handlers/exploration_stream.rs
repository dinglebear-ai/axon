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
use serde::Serialize;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

    let handle = tokio::spawn(async move {
        if tx
            .send(Ok(sse_json(
                "meta",
                &LlmStreamEvent::<services::types::SummarizeResult>::Meta {
                    phase: "summarizing",
                },
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
                        .await
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
                let _ = tx
                    .send(Ok(sse_json("done", &LlmStreamEvent::Done { result })))
                    .await;
            }
            Err(message) => {
                let _ = tx
                    .send(Ok(sse_json(
                        "error",
                        &LlmStreamEvent::<services::types::SummarizeResult>::Error { message },
                    )))
                    .await;
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

    let handle = tokio::spawn(async move {
        if tx
            .send(Ok(sse_json(
                "meta",
                &LlmStreamEvent::<services::types::ResearchPayload>::Meta {
                    phase: "researching",
                },
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
                        .await
                        .is_err()
                {
                    delta_disconnected.store(true, Ordering::Relaxed);
                    return;
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
                let _ = tx
                    .send(Ok(sse_json("done", &LlmStreamEvent::Done { result })))
                    .await;
            }
            Err(message) => {
                let _ = tx
                    .send(Ok(sse_json(
                        "error",
                        &LlmStreamEvent::<services::types::ResearchPayload>::Error { message },
                    )))
                    .await;
            }
        }
    });

    let event_stream = AbortOnDropStream { rx, handle };
    Sse::new(event_stream).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::sse::Event;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn exploration_stream_output_channel_is_bounded() {
        let (tx, _rx) = mpsc::channel::<Result<Event, Infallible>>(sse_event_buffer_for_tests());
        for _ in 0..sse_event_buffer_for_tests() {
            tx.try_send(Ok(Event::default()))
                .expect("buffer slot should be available");
        }
        assert!(
            tx.try_send(Ok(Event::default())).is_err(),
            "stream output channel should apply backpressure when full"
        );
    }

    #[test]
    fn research_stream_budget_is_finite() {
        assert_eq!(research_stream_timeout_for_tests(), Duration::from_secs(35));
    }

    #[test]
    fn summarize_stream_has_no_fixed_wall_clock_timeout() {
        assert_eq!(summarize_stream_timeout_for_tests(), None);
    }

    #[tokio::test]
    async fn exploration_stream_drop_aborts_worker_task() {
        struct AbortFlag(Arc<AtomicBool>);
        impl Drop for AbortFlag {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        let (_tx, rx) = mpsc::channel::<Result<Event, Infallible>>(1);
        let aborted = Arc::new(AtomicBool::new(false));
        let task_aborted = Arc::clone(&aborted);
        let handle = tokio::spawn(async move {
            let _flag = AbortFlag(task_aborted);
            std::future::pending::<()>().await;
        });
        tokio::task::yield_now().await;
        let stream = bounded_stream_for_tests(rx, handle);
        drop(stream);
        tokio::task::yield_now().await;

        assert!(
            aborted.load(Ordering::SeqCst),
            "dropping the SSE stream should abort the worker task"
        );
    }
}
