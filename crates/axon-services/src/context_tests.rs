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

#[test]
fn queue_summary_drop_synchronously_joins_the_thread() {
    let dropped = Arc::new(AtomicBool::new(false));
    let task_dropped = Arc::clone(&dropped);
    let (stop, stopped) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let _guard = Dropped(task_dropped);
        let _ = stopped.recv();
    });
    drop(QueueSummaryTask::new(stop, thread));
    assert!(dropped.load(Ordering::SeqCst));
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
