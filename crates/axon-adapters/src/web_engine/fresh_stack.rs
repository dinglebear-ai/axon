//! Poll-depth isolation for spider crawl futures.

use std::future::Future;

/// Run `future` on its own spawned task and await the result, so its poll
/// frames start from the runtime's base stack instead of stacking on top of
/// the caller's chain. Spider's `crawl*` futures carry very large debug-build
/// poll frames; polled inline underneath the full source pipeline
/// (dispatch → executor → adapter → render provider → scrape) they overflow
/// the default test-thread stack.
///
/// Inline semantics are preserved: dropping the returned future aborts the
/// spawned task (cooperative cancellation at its next await point, exactly
/// like dropping the inline future), and a panic inside the task is resumed
/// on the caller's thread.
pub(crate) async fn crawl_on_fresh_stack<F>(future: F) -> F::Output
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    struct AbortOnDrop<T>(tokio::task::JoinHandle<T>);
    impl<T> Drop for AbortOnDrop<T> {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let mut task = AbortOnDrop(tokio::spawn(future));
    match (&mut task.0).await {
        Ok(value) => value,
        Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
        // Only this function's drop guard aborts the task, and the guard
        // cannot drop while it is being awaited; a cancelled join therefore
        // means the runtime itself is shutting down.
        Err(error) => panic!("crawl task cancelled unexpectedly: {error}"),
    }
}
