use crate::server::test_support::{spawn_ask_test_server, stop};
use axon_authz::http::AuthPolicy;
use axum::http::StatusCode;
use axum::response::sse::Event;
use serial_test::serial;
use std::convert::Infallible;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::mpsc;

/// POST `/v1/ask/stream` against a loopback-dev test server (no auth) and
/// return the HTTP status. Exercises the same validation the handler runs
/// before it reaches retrieval/synthesis.
async fn ask_stream_status(body: serde_json::Value) -> StatusCode {
    let (base, shutdown, handle) = spawn_ask_test_server(AuthPolicy::LoopbackDev).await;
    let status = reqwest::Client::new()
        .post(format!("{base}/v1/ask/stream"))
        .json(&body)
        .send()
        .await
        .expect("ask stream request")
        .status();
    stop(shutdown, handle).await;
    StatusCode::from_u16(status.as_u16()).expect("status code")
}

#[tokio::test]
#[serial]
async fn ask_stream_rejects_empty_query() {
    let status = ask_stream_status(serde_json::json!({ "query": "" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[serial]
async fn ask_stream_rejects_explain_mode() {
    let status = ask_stream_status(serde_json::json!({
        "query": "why?",
        "explain": true
    }))
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[serial]
async fn ask_stream_rejects_invalid_collection_before_sse() {
    let status = ask_stream_status(serde_json::json!({
        "query": "why?",
        "collection": "../secret"
    }))
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
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
