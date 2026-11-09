//! Keep admitted blocking work owned through async cancellation and shutdown.
use super::*;

pub(super) type OperationOwner = Arc<tokio::sync::OwnedMutexGuard<()>>;

pub(super) async fn blocking<T: Send + 'static>(
    owner: OperationOwner,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, tokio::task::JoinError> {
    tokio::task::spawn_blocking(move || {
        let _operation = owner;
        work()
    })
    .await
}

impl ArtifactCandidateOutbox {
    pub(super) async fn begin_operation(&self) -> OperationOwner {
        Arc::new(self.gate.clone().lock_owned().await)
    }

    pub(crate) async fn shutdown_drain(&self) {
        self.drain_cancelled.store(true, Ordering::Release);
        self.drain_requested.store(false, Ordering::Release);
        let task = self
            .drain_task
            .lock()
            .expect("outbox drain task lock")
            .take();
        if let Some(task) = task {
            task.abort();
            let _ = task.await;
        }
        // Aborting an async owner does not stop its already-started blocking
        // work. Cloned operation ownership keeps this gate held until every
        // admitted worker has released its disk fence; no retry hides a busy
        // fence held by a different process.
        let _quiescent = self.gate.lock().await;
        self.draining.store(false, Ordering::Release);
    }
}
