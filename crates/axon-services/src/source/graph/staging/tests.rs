use super::*;
use futures_util::{StreamExt, stream};

#[tokio::test]
async fn buffered_graph_staging_releases_writer_without_consuming_its_result() {
    let gate = axon_core::sqlite::SqliteWriteGate::default();
    let held = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let pending = stream::iter(0..2)
        .map(|index| {
            let gate = gate.clone();
            let held = held.clone();
            let release = release.clone();
            run_staging_independently(async move {
                if index == 0 {
                    held.notified().await;
                } else {
                    // Model scheduler completion owning the live writer while
                    // the earlier buffered result becomes ready for publication.
                    let _writer = gate.lock().await;
                    held.notify_one();
                    release.notified().await;
                }
                Ok(index)
            })
        })
        .buffered(2);
    tokio::pin!(pending);
    assert_eq!(pending.next().await.unwrap().unwrap(), 0);
    release.notify_one();
    let writer = tokio::time::timeout(std::time::Duration::from_secs(1), gate.lock())
        .await
        .expect("publication must not require polling the later buffered graph result");
    drop(writer);
    assert_eq!(pending.next().await.unwrap().unwrap(), 1);
}

#[tokio::test]
async fn canceling_staging_caller_releases_its_owned_writer() {
    let gate = axon_core::sqlite::SqliteWriteGate::default();
    let held = Arc::new(tokio::sync::Notify::new());
    let caller = tokio::spawn(run_staging_independently({
        let gate = gate.clone();
        let held = held.clone();
        async move {
            let _writer = gate.lock().await;
            held.notify_one();
            std::future::pending::<()>().await;
            Ok(())
        }
    }));
    held.notified().await;
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    let writer = tokio::time::timeout(std::time::Duration::from_secs(1), gate.lock())
        .await
        .expect("canceling the caller must abort its owned staging work");
    drop(writer);
}
