use std::future::Future;
use std::panic::AssertUnwindSafe;

use futures::FutureExt;
use uuid::Uuid;

use super::runners::JobResult;
use crate::backend::JobKind;

/// Run a job runner future, converting a panic into a job failure instead of
/// letting it unwind and kill the worker task.
///
/// Worker loops are detached `tokio::spawn` tasks that await the runner future
/// inline. Before this guard, a panic anywhere in a runner (e.g. deep in the
/// crawl engine) unwound the lane's task permanently while the process stayed
/// alive — a silent, unrecoverable worker death that required a full restart.
/// Catching the unwind here keeps the lane alive: the offending job is marked
/// `failed` and the loop continues claiming.
pub(super) async fn run_catching<Fut>(fut: Fut, kind: JobKind, job_id: Uuid) -> JobResult
where
    Fut: Future<Output = JobResult>,
{
    match AssertUnwindSafe(fut).catch_unwind().await {
        Ok(result) => result,
        Err(panic) => {
            let msg = panic_message(panic.as_ref());
            tracing::error!(
                table = kind.table_name(),
                job_id = %job_id,
                panic = %msg,
                "job worker: runner PANICKED — lane survived via catch_unwind; marking job failed"
            );
            Err(format!("job panicked: {msg}").into())
        }
    }
}

/// Best-effort extraction of a human-readable message from a caught panic payload.
fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

#[cfg(test)]
#[path = "panic_guard_tests.rs"]
mod tests;
