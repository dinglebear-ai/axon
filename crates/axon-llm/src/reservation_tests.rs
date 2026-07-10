use super::*;

// This crate's test suite runs every test function in parallel by default,
// but there is exactly one `LLM_RESERVATIONS` singleton (see module docs) and
// nothing else in this crate's test suite touches the top-level `runtime::
// complete_text`/`complete_streaming` dispatch (backend-level tests call the
// per-backend `headless::gemini::complete_text` etc. directly). So a single
// sequential test function exercises the whole lifecycle without races; the
// core reservation/cooldown algorithm itself (interactive-reserve
// preservation, cooldown entry/expiry, fatal-vs-retryable) is already covered
// by `axon-observe`'s own `reservation_tests.rs` against a fresh manager —
// this test only covers axon-llm's additions: priority scoping and the thin
// success/failure/health wrappers around the shared singleton.
#[tokio::test]
async fn llm_reservation_pool_lifecycle() {
    let manager = manager_for_tests();
    manager.record_success().await; // known-healthy baseline
    assert_eq!(health().await, HealthStatus::Healthy);
    assert!(cooling_snapshot().await.is_none());

    // Default priority (no `with_priority` scope) is Background.
    let outside = reserve().await.expect("reservation should be granted");
    assert_eq!(outside.priority(), JobPriority::Background);
    drop(outside);

    // `with_priority` tags reservations acquired inside its future.
    let interactive = with_priority(JobPriority::Interactive, reserve())
        .await
        .expect("interactive reservation should be granted");
    assert_eq!(interactive.priority(), JobPriority::Interactive);
    drop(interactive);

    // The scope does not leak past its future.
    with_priority(JobPriority::Interactive, async {}).await;
    let after_scope = reserve().await.expect("reservation should be granted");
    assert_eq!(after_scope.priority(), JobPriority::Background);
    drop(after_scope);

    // Bulk (background) work claims capacity right up to the interactive
    // reserve boundary — mirrors "bulk LLM work (synthesis/research/extract)
    // does not starve interactive ask" from the provider contract.
    let mut bulk = Vec::new();
    while let Ok(reservation) = with_priority(JobPriority::Background, reserve()).await {
        bulk.push(reservation);
    }
    assert!(
        !bulk.is_empty(),
        "background work should have claimed some capacity before hitting the reserve"
    );

    // Interactive (ask) still gets in — the pool refuses more background
    // reservations once they'd eat into the reserve, but interactive bypasses
    // that check.
    let interactive = with_priority(JobPriority::Interactive, reserve())
        .await
        .expect("interactive reservation must not be starved by bulk background work");
    assert_eq!(interactive.priority(), JobPriority::Interactive);
    drop(interactive);
    drop(bulk);

    // LLM_COOLDOWN_AFTER_FAILURES = 3: the first two retryable failures
    // degrade but do not cool the pool.
    assert_eq!(
        record_failure("llm.timeout", true).await,
        ProviderReservationOutcome::Recorded
    );
    assert_eq!(
        record_failure("llm.timeout", true).await,
        ProviderReservationOutcome::Recorded
    );
    assert_eq!(health().await, HealthStatus::Degraded);

    // The third pushes it into cooldown.
    assert_eq!(
        record_failure("llm.timeout", true).await,
        ProviderReservationOutcome::Cooling
    );
    assert_eq!(health().await, HealthStatus::Cooling);
    let snapshot = cooling_snapshot()
        .await
        .expect("pool should report a cooling snapshot");
    assert_eq!(snapshot.reason, "llm.timeout");
    assert!(cooldown_until().await.is_some());

    // A subsequent success clears the cooldown, leaving the singleton clean
    // for any test added later.
    record_success().await;
    assert_eq!(health().await, HealthStatus::Healthy);
    assert!(cooling_snapshot().await.is_none());
}
