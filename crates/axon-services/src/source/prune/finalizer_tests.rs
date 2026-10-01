use super::*;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn finalizer_busy_then_available_reconciles_without_backoff_clock_advance() {
    let (ledger, generation) = graph_retry_fixture().await;
    let providers = BusyGraph {
        calls: AtomicUsize::new(0),
        retry_only: AtomicBool::new(false),
        vector_calls: AtomicUsize::new(0),
        vector_fails: AtomicBool::new(false),
        busy: AtomicBool::new(true),
        retry_generation: Some(generation.generation.clone()),
    };
    let passes = AtomicUsize::new(0);
    let summary = super::super::super::finalizer::drain(
        &ledger,
        &index_counts(&generation.generation),
        None,
        Duration::from_secs(1),
        Duration::from_millis(1),
        |retry_only| {
            providers.retry_only.store(retry_only, Ordering::SeqCst);
            if passes.fetch_add(1, Ordering::SeqCst) > 0 {
                providers.busy.store(false, Ordering::SeqCst);
            }
            run_retry(&ledger, &providers, &generation.generation)
        },
    )
    .await;
    assert_eq!(passes.load(Ordering::SeqCst), 2);
    assert_eq!(summary.resolved, 3);
    assert_eq!(summary.failed, 0);
    assert!(crate::source::job_tracking::prune_outcome_warning(&summary).is_none());
    assert!(
        ledger
            .list_pending_cleanup_debt(SourceId::new(SRC))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test(start_paused = true)]
async fn finalizer_permanent_busy_is_bounded_and_pending() {
    let (ledger, generation) = graph_retry_fixture().await;
    let providers = BusyGraph {
        calls: AtomicUsize::new(0),
        retry_only: AtomicBool::new(false),
        vector_calls: AtomicUsize::new(0),
        vector_fails: AtomicBool::new(false),
        busy: AtomicBool::new(true),
        retry_generation: Some(generation.generation.clone()),
    };
    let summary = super::super::super::finalizer::drain(
        &ledger,
        &index_counts(&generation.generation),
        None,
        Duration::from_secs(3),
        Duration::from_secs(1),
        |retry_only| {
            providers.retry_only.store(retry_only, Ordering::SeqCst);
            run_retry(&ledger, &providers, &generation.generation)
        },
    )
    .await;
    assert_eq!(providers.calls.load(Ordering::SeqCst), 3);
    assert_eq!(summary.failed, 3);
    assert!(crate::source::job_tracking::prune_outcome_warning(&summary).is_some());
}

#[tokio::test(start_paused = true)]
async fn finalizer_cancellation_ends_busy_wait_and_preserves_debt() {
    let (ledger, generation) = graph_retry_fixture().await;
    let providers = BusyGraph {
        calls: AtomicUsize::new(0),
        retry_only: AtomicBool::new(false),
        vector_calls: AtomicUsize::new(0),
        vector_fails: AtomicBool::new(false),
        busy: AtomicBool::new(true),
        retry_generation: Some(generation.generation.clone()),
    };
    let cancellation = CancellationToken::new();
    let trigger = cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        trigger.cancel();
    });
    let summary = super::super::super::finalizer::drain(
        &ledger,
        &index_counts(&generation.generation),
        Some(&cancellation),
        Duration::from_secs(120),
        Duration::from_secs(60),
        |retry_only| {
            providers.retry_only.store(retry_only, Ordering::SeqCst);
            run_retry(&ledger, &providers, &generation.generation)
        },
    )
    .await;
    assert_eq!(providers.calls.load(Ordering::SeqCst), 1);
    assert_eq!(summary.failed, 3);
}

#[tokio::test(start_paused = true)]
async fn finalizer_retries_graph_only_and_preserves_unrelated_vector_failure() {
    let (ledger, generation) = graph_retry_fixture().await;
    let mut debt = cleanup_debt(
        CleanupDebtKind::VectorDelete,
        CleanupSelector::Generation {
            source_id: SourceId::new(SRC),
            generation: SourceGenerationId::new("old-generation"),
        },
    );
    debt.debt_id = CleanupDebtId::new("vector-failure");
    debt.generation = Some(SourceGenerationId::new("old-generation"));
    debt.vector_collection = Some(COLLECTION.into());
    ledger.record_cleanup_debt(debt).await.unwrap();
    let providers = BusyGraph {
        calls: AtomicUsize::new(0),
        busy: AtomicBool::new(true),
        retry_only: AtomicBool::new(false),
        vector_calls: AtomicUsize::new(0),
        vector_fails: AtomicBool::new(true),
        retry_generation: Some(generation.generation.clone()),
    };
    let summary = super::super::super::finalizer::drain(
        &ledger,
        &index_counts(&generation.generation),
        None,
        Duration::from_secs(3),
        Duration::from_secs(1),
        |retry_only| {
            providers.retry_only.store(retry_only, Ordering::SeqCst);
            if retry_only {
                providers.busy.store(false, Ordering::SeqCst);
            }
            run_retry(&ledger, &providers, &generation.generation)
        },
    )
    .await;
    assert_eq!(providers.vector_calls.load(Ordering::SeqCst), 1);
    assert_eq!(summary.resolved, 3);
    assert_eq!(summary.failed, 1);
    assert!(crate::source::job_tracking::prune_outcome_warning(&summary).is_some());
}

#[tokio::test]
async fn finalizer_does_not_retry_nonbusy_or_other_generation_failures() {
    for (code, same_generation) in [("graph.storage_error", true), ("graph.source_busy", false)] {
        let (ledger, generation) = graph_retry_fixture().await;
        for mut debt in ledger
            .list_pending_cleanup_debt(SourceId::new(SRC))
            .await
            .unwrap()
        {
            debt.last_error = Some(SourceError {
                code: code.into(),
                severity: Severity::Warning,
                message: "fixture".into(),
                source_item_key: None,
                retryable: true,
                provider_id: None,
                cause: None,
            });
            if !same_generation {
                if let CleanupSelector::GraphItemEvidence {
                    retirement_generation,
                    ..
                } = &mut debt.selector
                {
                    *retirement_generation = SourceGenerationId::new("different-generation");
                }
            }
            ledger.record_cleanup_debt(debt).await.unwrap();
        }
        let passes = AtomicUsize::new(0);
        let summary = super::super::super::finalizer::drain(
            &ledger,
            &index_counts(&generation.generation),
            None,
            Duration::from_secs(120),
            Duration::from_secs(1),
            |_| {
                passes.fetch_add(1, Ordering::SeqCst);
                std::future::ready(DebtDrainSummary {
                    failed: 3,
                    ..Default::default()
                })
            },
        )
        .await;
        assert_eq!(passes.load(Ordering::SeqCst), 1);
        assert_eq!(summary.failed, 3);
    }
}

#[tokio::test]
async fn finalizer_finishes_ledger_dependency_after_busy_graph_retry() {
    let ledger = FakeLedgerStore::new();
    let (previous, committed) = seed_two_generations(&ledger).await;
    let mut debt = cleanup_debt(
        CleanupDebtKind::LedgerPrune,
        CleanupSelector::LedgerGenerations {
            source_id: SourceId::new(SRC),
            up_to_generation: previous.clone(),
        },
    );
    debt.debt_id = CleanupDebtId::new("zzz-ledger-after-graph");
    debt.created_at = Timestamp::from(chrono::Utc::now() + chrono::Duration::seconds(1));
    ledger.record_cleanup_debt(debt).await.unwrap();
    let providers = BusyGraph {
        calls: AtomicUsize::new(0),
        retry_only: AtomicBool::new(false),
        vector_calls: AtomicUsize::new(0),
        vector_fails: AtomicBool::new(false),
        busy: AtomicBool::new(true),
        retry_generation: Some(committed.clone()),
    };
    let summary = super::super::super::finalizer::drain(
        &ledger,
        &index_counts(&committed),
        None,
        Duration::from_secs(1),
        Duration::from_millis(1),
        |retry_only| {
            providers.retry_only.store(retry_only, Ordering::SeqCst);
            if retry_only {
                providers.busy.store(false, Ordering::SeqCst);
            }
            run_retry(&ledger, &providers, &committed)
        },
    )
    .await;
    assert_eq!(summary.failed, 0, "ledger dependency was skipped on retry");
    assert_eq!(summary.resolved, 3);
    assert_eq!(providers.vector_calls.load(Ordering::SeqCst), 1);
    assert!(
        ledger
            .list_pending_cleanup_debt(SourceId::new(SRC))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        ledger
            .get_manifest(SourceId::new(SRC), previous)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        ledger
            .get_manifest(SourceId::new(SRC), committed)
            .await
            .unwrap()
            .is_some()
    );
    assert!(crate::source::job_tracking::prune_outcome_warning(&summary).is_none());
}

#[test]
fn ledger_retry_excludes_current_generation_and_recorded_errors() {
    let current = SourceGenerationId::new("current");
    let mut debt = cleanup_debt(
        CleanupDebtKind::LedgerPrune,
        CleanupSelector::LedgerGenerations {
            source_id: SourceId::new(SRC),
            up_to_generation: SourceGenerationId::new("previous"),
        },
    );
    assert!(super::super::super::finalizer::eligible_retry(
        &debt,
        Some(&current)
    ));
    assert!(!super::super::super::finalizer::eligible_retry(&debt, None));
    debt.last_error = Some(SourceError {
        code: "ledger.storage_error".into(),
        severity: Severity::Warning,
        message: "storage unavailable".into(),
        source_item_key: None,
        retryable: true,
        provider_id: None,
        cause: None,
    });
    assert!(!super::super::super::finalizer::eligible_retry(
        &debt,
        Some(&current)
    ));
    debt.last_error = None;
    debt.selector = CleanupSelector::LedgerGenerations {
        source_id: SourceId::new(SRC),
        up_to_generation: current.clone(),
    };
    assert!(!super::super::super::finalizer::eligible_retry(
        &debt,
        Some(&current)
    ));
}

#[tokio::test]
async fn finalizer_preserves_pending_nonledger_debt_after_graph_retry() {
    let ledger = FakeLedgerStore::new();
    let (previous, committed) = seed_two_generations(&ledger).await;
    let mut debt = cleanup_debt(
        CleanupDebtKind::LedgerPrune,
        CleanupSelector::LedgerGenerations {
            source_id: SourceId::new(SRC),
            up_to_generation: previous.clone(),
        },
    );
    debt.debt_id = CleanupDebtId::new("zzz-ledger-after-graph");
    debt.created_at = Timestamp::from(chrono::Utc::now() + chrono::Duration::seconds(1));
    ledger.record_cleanup_debt(debt).await.unwrap();
    let providers = BusyGraph {
        calls: AtomicUsize::new(0),
        retry_only: AtomicBool::new(false),
        vector_calls: AtomicUsize::new(0),
        vector_fails: AtomicBool::new(true),
        busy: AtomicBool::new(true),
        retry_generation: Some(committed.clone()),
    };
    let summary = super::super::super::finalizer::drain(
        &ledger,
        &index_counts(&committed),
        None,
        Duration::from_secs(1),
        Duration::from_millis(1),
        |retry_only| {
            providers.retry_only.store(retry_only, Ordering::SeqCst);
            if retry_only {
                providers.busy.store(false, Ordering::SeqCst);
            }
            run_retry(&ledger, &providers, &committed)
        },
    )
    .await;
    assert_eq!(summary.failed, 2);
    assert_eq!(summary.resolved, 1);
    assert_eq!(providers.vector_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        ledger
            .list_pending_cleanup_debt(SourceId::new(SRC))
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(
        ledger
            .get_manifest(SourceId::new(SRC), previous)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        ledger
            .get_manifest(SourceId::new(SRC), committed)
            .await
            .unwrap()
            .is_some()
    );
    assert!(crate::source::job_tracking::prune_outcome_warning(&summary).is_some());
}
