use super::*;

#[tokio::test]
async fn shutdown_joins_cooperative_workers_before_returning() {
    let shutdown = CancellationToken::new();
    let finished = Arc::new(Notify::new());
    let task_finished = Arc::clone(&finished);
    let token = shutdown.clone();
    let handle = tokio::spawn(async move {
        token.cancelled().await;
        tokio::task::yield_now().await;
        task_finished.notify_one();
    });
    let handles = WorkerHandles {
        unified: Arc::new(Notify::new()),
        activity: Arc::new(WorkerActivity::default()),
        shutdown,
        worker_handles: vec![handle],
    };

    handles.shutdown_and_join(Duration::from_secs(1)).await;
    tokio::time::timeout(Duration::from_millis(50), finished.notified())
        .await
        .expect("worker must finish before shutdown returns");
}

#[tokio::test]
async fn shutdown_aborts_non_cooperative_workers_at_deadline() {
    let shutdown = CancellationToken::new();
    let handle = tokio::spawn(std::future::pending::<()>());
    let handles = WorkerHandles {
        unified: Arc::new(Notify::new()),
        activity: Arc::new(WorkerActivity::default()),
        shutdown,
        worker_handles: vec![handle],
    };

    tokio::time::timeout(
        Duration::from_millis(100),
        handles.shutdown_and_join(Duration::from_millis(10)),
    )
    .await
    .expect("shutdown deadline must bound non-cooperative workers");
}
