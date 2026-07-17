use super::*;

fn lock_path(dir: &tempfile::TempDir) -> PathBuf {
    // Simulate a jobs DB inside the temp dir; the lock is its sibling.
    drain_lock_path(&dir.path().join("jobs.db"))
}

#[test]
fn lock_path_is_sibling_of_jobs_db() {
    let got = drain_lock_path(Path::new("/data/axon/jobs.db"));
    assert_eq!(got, PathBuf::from("/data/axon/jobs.db.drain-lock"));
}

#[tokio::test]
async fn holder_excludes_second_holder_until_dropped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = lock_path(&dir);

    let first = WorkerDrainLock::try_hold(&path)
        .await
        .expect("first try_hold")
        .expect("first holder acquires");

    let second = WorkerDrainLock::try_hold(&path)
        .await
        .expect("second try_hold");
    assert!(second.is_none(), "lock must be exclusive while held");
    assert!(WorkerDrainLock::is_held(&path).await.expect("is_held"));

    drop(first);

    let third = WorkerDrainLock::try_hold(&path).await.expect("third");
    assert!(third.is_some(), "lock must be reacquirable after release");
}

#[tokio::test]
async fn is_held_probe_does_not_keep_the_lock() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = lock_path(&dir);

    assert!(!WorkerDrainLock::is_held(&path).await.expect("probe"));
    // The probe above must have released — acquiring now must succeed.
    let holder = WorkerDrainLock::try_hold(&path).await.expect("try_hold");
    assert!(holder.is_some());
}

#[tokio::test]
async fn lock_file_is_created_on_first_use() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = lock_path(&dir);
    assert!(!path.exists());
    let _holder = WorkerDrainLock::try_hold(&path).await.expect("try_hold");
    assert!(path.exists(), "lock database file should be created");
}

/// Regression for `axon_rust-x4gxr.6`: a probe must never evict a live holder.
/// The holder's `BEGIN EXCLUSIVE` uses a busy-timeout, so even repeated probes
/// interleaved with the hold cannot knock it out — `is_held` keeps reporting
/// held, and the holder is still valid afterward.
#[tokio::test]
async fn probe_does_not_evict_a_live_holder() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = lock_path(&dir);

    let holder = WorkerDrainLock::try_hold(&path)
        .await
        .expect("try_hold")
        .expect("acquire");

    for _ in 0..5 {
        assert!(
            WorkerDrainLock::is_held(&path).await.expect("probe"),
            "probe must report held while a holder is alive"
        );
    }

    // Holder still owns the lock: a fresh hold attempt must fail.
    assert!(
        WorkerDrainLock::try_hold(&path)
            .await
            .expect("try_hold")
            .is_none(),
        "second holder must not acquire while first is alive"
    );
    drop(holder);
}
