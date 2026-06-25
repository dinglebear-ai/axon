use axum::http::StatusCode;
use axum::response::sse::Event;
use std::convert::Infallible;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::mpsc;

#[tokio::test]
async fn ask_stream_rejects_empty_query() {
    let response = super::v1_ask_stream_test_response(serde_json::json!({
        "query": ""
    }))
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn ask_stream_rejects_explain_mode() {
    let response = super::v1_ask_stream_test_response(serde_json::json!({
        "query": "why?",
        "explain": true
    }))
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn ask_stream_rejects_invalid_collection_before_sse() {
    let response = super::v1_ask_stream_test_response(serde_json::json!({
        "query": "why?",
        "collection": "../secret"
    }))
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn ask_stream_output_channel_is_bounded() {
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
async fn ask_stream_drop_aborts_worker_task() {
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
