use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use axon_ledger::store::FakeLedgerStore;

struct Dropped(Arc<AtomicBool>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn queue_summary_shutdown_joins_the_cancelled_task() {
    let dropped = Arc::new(AtomicBool::new(false));
    let task_dropped = Arc::clone(&dropped);
    let (stop, stopped) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let _guard = Dropped(task_dropped);
        let _ = stopped.recv();
    });
    let supervisor = QueueSummaryTask::new(stop, thread);

    supervisor.shutdown().await;

    assert!(dropped.load(Ordering::SeqCst));
}

#[tokio::test(flavor = "current_thread")]
async fn queue_summary_drop_never_joins_the_thread_on_the_executor() {
    let dropped = Arc::new(AtomicBool::new(false));
    let task_dropped = Arc::clone(&dropped);
    let (stop, _stopped) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let _guard = Dropped(task_dropped);
        let _ = released.recv();
    });
    drop(QueueSummaryTask::new(stop, thread));
    tokio::time::timeout(std::time::Duration::from_millis(100), async {
        tokio::task::yield_now().await;
    })
    .await
    .expect("dropping a worker supervisor must not block the executor");
    assert!(!dropped.load(Ordering::SeqCst));
    release.send(()).expect("release worker");
}

#[test]
fn adapter_cleanup_propagates_runtime_construction_failure() {
    let result = spawn_adapter_cleanup_worker_with_runtime(
        Arc::new(FakeLedgerStore::new()),
        SourceAdapterRegistry::default(),
        Err(std::io::Error::other("injected runtime failure")),
        "axon-adapter-cleanup",
    );
    let error = match result {
        Ok(_) => panic!("runtime failure must fail worker startup"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("injected runtime failure"));
}

#[test]
fn adapter_cleanup_propagates_thread_spawn_failure() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let result = spawn_adapter_cleanup_worker_with_runtime(
        Arc::new(FakeLedgerStore::new()),
        SourceAdapterRegistry::default(),
        runtime,
        "invalid\0thread-name",
    );
    let error = match result {
        Ok(_) => panic!("thread spawn failure must fail worker startup"),
        Err(error) => error,
    };

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

#[tokio::test]
async fn queue_summary_propagates_runtime_construction_failure() {
    let runtime = crate::test_support::sqlite_test_runtime().await.unwrap();
    let result = spawn_queue_summary_logger_with_runtime(
        runtime.runtime,
        1,
        Err(std::io::Error::other("injected queue runtime failure")),
        "axon-queue-summary-test",
    );
    let error = match result {
        Ok(_) => panic!("runtime failure must be visible"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("injected queue runtime failure"));
}

#[tokio::test]
async fn queue_summary_propagates_thread_spawn_failure() {
    let jobs = crate::test_support::sqlite_test_runtime().await.unwrap();
    let result = spawn_queue_summary_logger_with_runtime(
        jobs.runtime,
        1,
        Err(std::io::Error::other("unused runtime")),
        "invalid\0name",
    );
    let error = match result {
        Ok(_) => panic!("thread spawn failure must be visible"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}
