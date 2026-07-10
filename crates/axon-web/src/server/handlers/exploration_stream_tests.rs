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
