use axon_core::llm::CompletionResponse;
use axum::{body::to_bytes, http::StatusCode, response::sse::Event};
use std::convert::Infallible;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::mpsc;

#[tokio::test]
async fn v1_chat_stream_rejects_empty_message() {
    let response = super::v1_chat_stream_test_response(serde_json::json!({
        "message": ""
    }))
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn v1_chat_stream_rejects_unknown_fields() {
    let err = serde_json::from_value::<axon_services::client_contract::RestChatRequest>(
        serde_json::json!({
            "message": "hello",
            "collection": "should-not-exist"
        }),
    )
    .expect_err("chat request must reject RAG-only fields");

    assert!(err.to_string().contains("unknown field"));
}

#[tokio::test]
async fn v1_chat_stream_emits_meta_delta_done_sequence() {
    let response = super::v1_chat_stream_test_response_with_completion(
        serde_json::json!({
            "message": "hello"
        }),
        Box::new(|_request, mut on_delta| {
            Box::pin(async move {
                on_delta("hello")?;
                Ok(CompletionResponse {
                    text: "hello".to_string(),
                    usage: None,
                })
            })
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 16 * 1024)
        .await
        .expect("SSE body");
    let body = std::str::from_utf8(&body).expect("SSE body is utf8");

    let meta = body.find("event: meta").expect("meta event");
    let delta = body.find("event: delta").expect("delta event");
    let done = body.find("event: done").expect("done event");
    assert!(meta < delta, "{body}");
    assert!(delta < done, "{body}");
}

#[test]
fn chat_stream_output_channel_is_bounded() {
    let (tx, _rx) = mpsc::channel::<Result<Event, Infallible>>(super::sse_event_buffer_for_tests());
    for _ in 0..super::sse_event_buffer_for_tests() {
        tx.try_send(Ok(Event::default()))
            .expect("buffer slot should be available");
    }
    assert!(
        tx.try_send(Ok(Event::default())).is_err(),
        "stream output channel should apply backpressure when full"
    );
}

#[tokio::test]
async fn chat_stream_drop_aborts_worker_task() {
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
    let stream = super::bounded_stream_for_tests(rx, handle);
    drop(stream);
    tokio::task::yield_now().await;

    assert!(
        aborted.load(Ordering::SeqCst),
        "dropping the SSE stream should abort the worker task"
    );
}
