use super::*;

#[tokio::test]
async fn shutdown_waits_for_cancelled_scan_disk_owner_before_reopen() {
    use std::time::Duration;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(".pause-scan-test"), b"").unwrap();
    let outbox = Arc::new(ArtifactCandidateOutbox::new(root.path()));
    assert!(outbox.begin_drain());
    let scanning = outbox.clone();
    outbox.register_drain_task(tokio::spawn(async move {
        let _ = scanning.scan().await;
    }));
    tokio::time::timeout(Duration::from_secs(5), async {
        while !root.path().join(".scan-started-test").exists() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("blocking scan acquired the real disk fence");
    assert!(lock_disk_admission(root.path()).is_err());

    let stopping = outbox.clone();
    let mut shutdown = tokio::spawn(async move { stopping.shutdown_drain().await });
    let completed_early = tokio::time::timeout(Duration::from_millis(30), &mut shutdown).await;
    // Always unblock the worker, including on the old implementation's failure.
    std::fs::write(root.path().join(".release-scan-test"), b"").unwrap();
    let waited = completed_early.is_err();
    match completed_early {
        Ok(result) => result.unwrap(),
        Err(_) => tokio::time::timeout(Duration::from_secs(5), shutdown)
            .await
            .expect("shutdown settles after disk worker release")
            .unwrap(),
    }
    if !waited {
        // On the broken implementation only, settle the released fixture
        // worker before failing; the passing path has no lock retry.
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if lock_disk_admission(root.path()).is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("release failed-test worker before removing its root");
    }
    assert!(
        waited,
        "shutdown returned while its cancelled scan still held the disk fence"
    );
    let reopened = ArtifactCandidateOutbox::new(root.path());
    assert!(
        reopened
            .pending()
            .await
            .expect("immediate reopen after shutdown")
            .is_empty()
    );
    assert!(lock_disk_admission(root.path()).is_ok());
}
