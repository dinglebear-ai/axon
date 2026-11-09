use super::*;
use axon_core::config::Config;
use std::sync::Arc;

#[tokio::test]
async fn tracking_wrapper_does_not_retain_the_concrete_operation_future() {
    let directory = tempfile::tempdir().expect("private jobs directory");
    let context = ServiceContext::new(Arc::new(Config {
        sqlite_path: directory.path().join("jobs.db"),
        ..Config::test_default()
    }))
    .await
    .expect("isolated service context");
    const OPERATION_BYTES: usize = 256 * 1024;
    let future = track_operation_job(
        &context,
        OperationKind::MemoryCompaction,
        serde_json::json!({}),
        || async {
            let retained = [0_u8; OPERATION_BYTES];
            tokio::task::yield_now().await;
            std::hint::black_box(&retained);
            Ok(())
        },
    );
    let wrapper_bytes = std::mem::size_of_val(&future);
    assert!(
        wrapper_bytes < OPERATION_BYTES,
        "tracking wrapper retains the operation state ({wrapper_bytes} bytes)"
    );
}
