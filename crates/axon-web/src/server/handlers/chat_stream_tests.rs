use axon_services::service_traits::ask_service::ChatResult;
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
    let response = super::v1_chat_stream_test_response_with_service(
        serde_json::json!({
            "message": "hello"
        }),
        Box::new(|request, mut on_delta| {
            Box::pin(async move {
                on_delta("hello").map_err(|error| error.to_string())?;
                Ok(ChatResult {
                    session_id: "session-1".to_string(),
                    reply: "hello".to_string(),
                    model: Some(format!("model-for-{}", request.message)),
                    loadout: None,
                    agent: None,
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

    let progress = body.find("event: progress").expect("progress event");
    let delta = body.find("event: delta").expect("delta event");
    let done = body.find("event: done").expect("done event");
    assert!(progress < delta, "{body}");
    assert!(delta < done, "{body}");
}

#[tokio::test]
async fn chat_stream_preserves_raw_message_and_final_payload() {
    let observed_message = Arc::new(std::sync::Mutex::new(None));
    let service_message = Arc::clone(&observed_message);
    let response = super::v1_chat_stream_test_response_with_service(
        serde_json::json!({
            "message": "  hello  "
        }),
        Box::new(move |request, _on_delta| {
            *service_message.lock().unwrap() = Some(request.message);
            Box::pin(async move {
                Ok(ChatResult {
                    session_id: "session-raw".to_string(),
                    reply: "answer".to_string(),
                    model: Some("chat-model".to_string()),
                    loadout: None,
                    agent: None,
                })
            })
        }),
    )
    .await;

    let body = to_bytes(response.into_body(), 16 * 1024)
        .await
        .expect("SSE body");
    let body = std::str::from_utf8(&body).expect("SSE body is utf8");

    assert_eq!(
        observed_message.lock().unwrap().as_deref(),
        Some("  hello  ")
    );
    assert!(body.contains("\"message\":\"  hello  \""), "{body}");
    assert!(body.contains("\"answer\":\"answer\""), "{body}");
    assert!(body.contains("\"model\":\"chat-model\""), "{body}");
}

#[tokio::test]
async fn dropping_response_cancels_in_flight_service_future() {
    struct CancellationFlag(Arc<AtomicBool>);
    impl Drop for CancellationFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let started = Arc::new(AtomicBool::new(false));
    let canceled = Arc::new(AtomicBool::new(false));
    let service_started = Arc::clone(&started);
    let service_canceled = Arc::clone(&canceled);
    let response = super::v1_chat_stream_test_response_with_service(
        serde_json::json!({
            "message": "hello"
        }),
        Box::new(move |_request, _on_delta| {
            Box::pin(async move {
                let _cancellation_flag = CancellationFlag(service_canceled);
                service_started.store(true, Ordering::SeqCst);
                std::future::pending::<Result<ChatResult, String>>().await
            })
        }),
    )
    .await;

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while !started.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("service future should start");

    drop(response);

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while !canceled.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("dropping transport response should cancel service future");
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
