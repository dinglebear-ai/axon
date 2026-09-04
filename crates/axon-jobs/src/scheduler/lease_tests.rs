use super::*;
use crate::store::open_sqlite_pool;
use axon_api::source::ProviderKind;

#[tokio::test]
async fn waiting_drop_records_runtime_absence() {
    let _test_guard = DROP_CLEANUP_TEST_LOCK.lock().await;
    let before = DROP_CLEANUP_FAILURES.load(Ordering::Relaxed);
    std::thread::spawn(|| {
        spawn_drop_cleanup(
            async { Ok(()) },
            "queued",
            "reservation-runtime".to_string(),
            fence_fingerprint("secret-runtime-fence"),
        );
    })
    .join()
    .expect("drop thread");
    assert!(DROP_CLEANUP_FAILURES.load(Ordering::Relaxed) > before);
    let failure = LAST_DROP_CLEANUP_FAILURE
        .lock()
        .expect("failure capture lock")
        .clone()
        .expect("runtime failure details");
    assert_eq!(failure.phase, "queued");
    assert_eq!(failure.reason, "runtime_unavailable");
    assert_eq!(failure.reservation_id, "reservation-runtime");
    assert_eq!(
        failure.fence_fingerprint,
        fence_fingerprint("secret-runtime-fence")
    );
    assert!(!failure.fence_fingerprint.contains("secret-runtime-fence"));
    assert!(failure.error.contains("runtime unavailable"));
}

#[tokio::test]
async fn waiting_drop_records_async_cleanup_failure() {
    let _test_guard = DROP_CLEANUP_TEST_LOCK.lock().await;
    let before = DROP_CLEANUP_FAILURES.load(Ordering::Relaxed);
    spawn_drop_cleanup(
        async { Err(SchedulerError::QueueFull) },
        "active",
        "reservation-async".to_string(),
        fence_fingerprint("secret-async-fence"),
    );
    for _ in 0..20 {
        if DROP_CLEANUP_FAILURES.load(Ordering::Relaxed) > before {
            let failure = LAST_DROP_CLEANUP_FAILURE
                .lock()
                .expect("failure capture lock")
                .clone()
                .expect("async failure details");
            assert_eq!(failure.phase, "active");
            assert_eq!(failure.reason, "async_cleanup_failed");
            assert_eq!(failure.reservation_id, "reservation-async");
            assert_eq!(failure.error, SchedulerError::QueueFull.to_string());
            assert!(!failure.fence_fingerprint.contains("secret-async-fence"));
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("async cleanup failure was not recorded");
}

#[tokio::test]
async fn completion_stale_fence_warning_carries_secret_free_identity() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    let scheduler = ProviderScheduler::new(
        pool,
        super::super::ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "completion-warning".to_string(),
            authority_id: "completion-warning-authority".to_string(),
        },
        super::super::SchedulerConfig::new(1, 0, 8, 8).expect("scheduler config"),
    )
    .expect("scheduler");
    let lease = ActiveReservationLease::<()> {
        scheduler,
        reservation_id: "reservation-completion".to_string(),
        fence: "secret-completion-fence".to_string(),
        _kind: std::marker::PhantomData,
    };

    record_completion_stale_fence(&lease);

    let warning = LAST_COMPLETION_FENCE_WARNING
        .lock()
        .expect("completion warning capture lock")
        .clone()
        .expect("completion warning details");
    assert_eq!(warning.reservation_id, "reservation-completion");
    assert_eq!(warning.provider_kind, "Embedding");
    assert_eq!(warning.provider_id, "completion-warning");
    assert_eq!(warning.capacity_domain, "embedding");
    assert!(!format!("{warning:?}").contains("secret-completion-fence"));
}
