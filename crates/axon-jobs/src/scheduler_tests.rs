use super::*;
use crate::store::open_sqlite_pool;
use std::future::{Future, poll_fn};
use std::task::Poll;

#[test]
fn priority_serialization_matches_scheduler_lane_order() {
    assert_eq!(enum_name(JobPriority::Interactive).unwrap(), "interactive");
    assert_eq!(enum_name(JobPriority::Maintenance).unwrap(), "maintenance");
}

#[test]
fn shared_dispatch_signal_elects_only_one_recovery_dispatcher() {
    let signal = Arc::new(DispatchSignal::default());
    let first = signal.try_claim_recovery().expect("first waiter is leader");
    assert!(
        signal.try_claim_recovery().is_none(),
        "a second waiter must not start another recovery poll"
    );
    drop(first);
    assert!(signal.try_claim_recovery().is_some());
}

#[test]
fn shared_dispatch_signal_spans_authorities_in_one_capacity_domain() {
    let first = shared_dispatch_signal("authority-a", "vector", "shared-qdrant");
    let second = shared_dispatch_signal("authority-b", "vector", "shared-qdrant");
    assert!(
        Arc::ptr_eq(&first, &second),
        "authorities sharing provider capacity must wake the same waiters"
    );
}

#[tokio::test]
async fn invalid_scheduler_capacity_is_rejected() {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let error = ProviderScheduler::new(
        pool,
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "tei".into(),
            authority_id: "authority-a".into(),
        },
        SchedulerConfig {
            capacity: 1,
            interactive_reserve: 2,
            max_entries: 10,
            max_units: 10,
        },
    )
    .expect_err("reserve larger than capacity must be rejected");
    assert!(matches!(error, SchedulerError::RequestTooLarge));
}

#[tokio::test]
async fn shared_write_gate_blocks_before_acquiring_pool_connections() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");

    // Pre-warm all eight connections so a regression that acquires from the
    // pool before waiting on the write gate does so synchronously on its first
    // poll. This makes the assertion deterministic instead of relying on a
    // sleep to guess whether spawned tasks reached SQLite contention.
    let mut warmed = Vec::new();
    for _ in 0..pool.options().get_max_connections() {
        warmed.push(pool.acquire().await.expect("pre-warm pool connection"));
    }
    drop(warmed);

    let held = axon_core::sqlite::ImmediateTx::begin(&pool)
        .await
        .expect("hold writer lock");
    let gate = SchedulerWriteGate::default();
    let held_gate = gate.lock().await;
    let mut schedulers = Vec::new();
    for index in 0..7 {
        schedulers.push(
            ProviderScheduler::new_with_write_gate(
                pool.clone(),
                ProviderCapacityDomain {
                    kind: ProviderKind::Fetch,
                    instance_id: format!("fetch-{index}"),
                    authority_id: "authority-a".into(),
                },
                SchedulerConfig {
                    capacity: 1,
                    interactive_reserve: 0,
                    max_entries: 10,
                    max_units: 10,
                },
                gate.clone(),
            )
            .expect("scheduler"),
        );
    }

    let mut waiters = schedulers
        .iter()
        .map(|scheduler| Box::pin(scheduler.reconcile()))
        .collect::<Vec<_>>();
    for waiter in &mut waiters {
        poll_fn(|cx| {
            assert!(
                waiter.as_mut().poll(cx).is_pending(),
                "reconcile must wait while the shared gate is held"
            );
            Poll::Ready(())
        })
        .await;
    }

    let control_connection = pool
        .try_acquire()
        .expect("gate waiters must not consume control-plane pool connections");
    drop(control_connection);
    held.rollback().await;
    drop(held_gate);
    for waiter in waiters {
        waiter.await.expect("reconcile");
    }
}

#[tokio::test]
async fn sqlite_scheduler_grants_and_fences_a_reservation() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    sqlx::query(
        "INSERT INTO sources (source_id, summary_json, created_at, updated_at)
         VALUES ('scheduler-source', '{}', '', '')",
    )
    .execute(&pool)
    .await
    .expect("source");
    sqlx::query(
        "INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at)
         VALUES ('00000000-0000-0000-0000-000000000007', 'source', 'queued', 'queued', 'normal', 'scheduler-source', '', '')",
    )
    .execute(&pool)
    .await
    .expect("job");
    let scheduler = ProviderScheduler::new(
        pool,
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "tei".into(),
            authority_id: "authority-a".into(),
        },
        SchedulerConfig {
            capacity: 2,
            interactive_reserve: 1,
            max_entries: 10,
            max_units: 10,
        },
    )
    .expect("scheduler");
    let grant = scheduler
        .reserve(ReservationRequest {
            job_id: JobId::new(Uuid::from_u128(7)),
            stage_id: None,
            attempt: 1,
            fence: "fence-1".into(),
            priority: JobPriority::Interactive,
            units: 1,
        })
        .await
        .expect("grant");
    assert!(grant.granted);
    assert_eq!(grant.units, 1);
    scheduler
        .complete(&grant.reservation_id, "fence-1")
        .await
        .expect("completion");
    assert!(matches!(
        scheduler.complete(&grant.reservation_id, "fence-1").await,
        Err(SchedulerError::StaleFence)
    ));
}

#[tokio::test]
async fn reserved_call_releases_capacity_after_provider_completion() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    sqlx::query("INSERT INTO sources (source_id, summary_json, created_at, updated_at) VALUES ('s', '{}', '', '')")
        .execute(&pool)
        .await
        .expect("source");
    sqlx::query("INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at) VALUES ('00000000-0000-0000-0000-000000000008', 'source', 'queued', 'queued', 'normal', 's', '', '')")
        .execute(&pool)
        .await
        .expect("job");
    let scheduler = ProviderScheduler::new(
        pool,
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "tei".into(),
            authority_id: "a".into(),
        },
        SchedulerConfig {
            capacity: 1,
            interactive_reserve: 0,
            max_entries: 4,
            max_units: 4,
        },
    )
    .expect("scheduler");
    let result = call_reserved::<(), _, _, _, _>(
        &scheduler,
        ReservationRequest {
            job_id: JobId::new(Uuid::from_u128(8)),
            stage_id: None,
            attempt: 1,
            fence: "fence".into(),
            priority: JobPriority::Normal,
            units: 1,
        },
        |_lease| async { Ok::<_, &'static str>("ok") },
    )
    .await
    .expect("reserved call");
    assert_eq!(result, "ok");
}

#[tokio::test]
async fn reserved_call_releases_capacity_after_provider_failure() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    sqlx::query("INSERT INTO sources (source_id, summary_json, created_at, updated_at) VALUES ('failed', '{}', '', '')")
        .execute(&pool)
        .await
        .expect("source");
    sqlx::query("INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at) VALUES ('00000000-0000-0000-0000-000000000009', 'source', 'queued', 'queued', 'normal', 'failed', '', '')")
        .execute(&pool)
        .await
        .expect("job");
    let scheduler = ProviderScheduler::new(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "tei".into(),
            authority_id: "a".into(),
        },
        SchedulerConfig {
            capacity: 1,
            interactive_reserve: 0,
            max_entries: 4,
            max_units: 4,
        },
    )
    .expect("scheduler");
    let error = call_reserved::<(), (), _, _, _>(
        &scheduler,
        ReservationRequest {
            job_id: JobId::new(Uuid::from_u128(9)),
            stage_id: None,
            attempt: 1,
            fence: "failure-fence".into(),
            priority: JobPriority::Normal,
            units: 1,
        },
        |_lease| async { Err::<(), _>("provider failed") },
    )
    .await
    .expect_err("provider failure propagates");
    assert!(matches!(
        error,
        ReservedCallError::Provider("provider failed")
    ));
    let row: (String, String) = sqlx::query_as(
        "SELECT status, terminal_reason FROM provider_reservations WHERE fence = 'failure-fence'",
    )
    .fetch_one(&pool)
    .await
    .expect("reservation row");
    assert_eq!(row, ("released".to_string(), "provider_failed".to_string()));
}

#[tokio::test]
async fn reconcile_cancels_expired_grants_and_quarantines_uncertain_calls() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    sqlx::query("INSERT INTO sources (source_id, summary_json, created_at, updated_at) VALUES ('reconcile', '{}', '', '')")
        .execute(&pool)
        .await
        .expect("source");
    sqlx::query("INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at) VALUES ('00000000-0000-0000-0000-00000000000a', 'source', 'queued', 'queued', 'normal', 'reconcile', '', '')")
        .execute(&pool)
        .await
        .expect("job");
    let scheduler = ProviderScheduler::new(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "tei".into(),
            authority_id: "a".into(),
        },
        SchedulerConfig {
            capacity: 2,
            interactive_reserve: 0,
            max_entries: 4,
            max_units: 4,
        },
    )
    .expect("scheduler");
    let grant = scheduler
        .reserve(ReservationRequest {
            job_id: JobId::new(Uuid::from_u128(10)),
            stage_id: None,
            attempt: 1,
            fence: "grant-fence".into(),
            priority: JobPriority::Normal,
            units: 1,
        })
        .await
        .expect("grant");
    // A crashed/replaced scheduler authority must not permanently consume
    // shared capacity. Grant deadlines are authority-independent because a
    // grant has not activated provider work yet.
    sqlx::query("UPDATE provider_reservations SET grant_deadline = datetime('now', '-1 second'), authority_id = 'replaced-authority' WHERE reservation_id = ?")
        .bind(&grant.reservation_id)
        .execute(&pool)
        .await
        .expect("expire grant");
    let queued_id = "abandoned-queued-reservation";
    sqlx::query(
        "INSERT INTO provider_reservations (
            reservation_id, job_id, provider_kind, priority, requested_units,
            granted_units, status, updated_at, capacity_domain, instance_id,
            authority_id, fence
         ) VALUES (?, '00000000-0000-0000-0000-00000000000a', 'embedding', 'normal',
            1, 0, 'queued', datetime('now', '-91 seconds'), 'embedding', 'tei', 'a',
            'queued-fence')",
    )
    .bind(queued_id)
    .execute(&pool)
    .await
    .expect("abandoned queued reservation");
    let active_id = "active-reservation";
    sqlx::query(
        "INSERT INTO provider_reservations (
            reservation_id, job_id, provider_kind, priority, requested_units,
            granted_units, status, updated_at, capacity_domain, instance_id,
            authority_id, renewed_at, expires_at, fence
         ) VALUES (?, '00000000-0000-0000-0000-00000000000a', 'embedding', 'normal',
            1, 1, 'active', datetime('now'), 'embedding', 'tei', 'a',
            datetime('now', '-61 seconds'), datetime('now', '+1 minute'), 'active-fence')",
    )
    .bind(active_id)
    .execute(&pool)
    .await
    .expect("active reservation");

    let result = scheduler.reconcile().await.expect("reconcile");
    assert_eq!(result.expired_queued, 1);
    assert_eq!(result.expired_grants, 1);
    assert_eq!(result.quarantined_active, 1);
    let rows: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT status, quarantined, terminal_reason FROM provider_reservations
         WHERE reservation_id IN (?, ?) ORDER BY reservation_id",
    )
    .bind(active_id)
    .bind(&grant.reservation_id)
    .fetch_all(&pool)
    .await
    .expect("reconciled rows");
    assert_eq!(
        rows,
        vec![
            ("active".into(), 1, "active_lease_uncertain".into()),
            ("canceled".into(), 0, "grant_expired".into()),
        ]
    );
    let queued: (String, String) = sqlx::query_as(
        "SELECT status, terminal_reason FROM provider_reservations WHERE reservation_id = ?",
    )
    .bind(queued_id)
    .fetch_one(&pool)
    .await
    .expect("expired queued row");
    assert_eq!(queued, ("expired".into(), "abandoned_waiter".into()));
}

#[tokio::test]
async fn waiter_on_second_pool_observes_release_without_shared_notification() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("scheduler.db");
    let first_pool = open_sqlite_pool(path.to_str().unwrap())
        .await
        .expect("first pool");
    let second_pool = open_sqlite_pool(path.to_str().unwrap())
        .await
        .expect("second pool");
    sqlx::query(
        "INSERT INTO sources (source_id, summary_json, created_at, updated_at)
         VALUES ('cross-process-source', '{}', '', '')",
    )
    .execute(&first_pool)
    .await
    .expect("source");
    for suffix in [11_u128, 12_u128] {
        sqlx::query(
            "INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at)
             VALUES (?, 'source', 'queued', 'queued', 'normal', 'cross-process-source', '', '')",
        )
        .bind(Uuid::from_u128(suffix).to_string())
        .execute(&first_pool)
        .await
        .expect("job");
    }
    let domain = ProviderCapacityDomain {
        kind: ProviderKind::Embedding,
        instance_id: "tei-shared".into(),
        authority_id: "authority-shared".into(),
    };
    let config = SchedulerConfig {
        capacity: 1,
        interactive_reserve: 0,
        max_entries: 8,
        max_units: 8,
    };
    let first = ProviderScheduler::new(first_pool.clone(), domain.clone(), config)
        .expect("first scheduler");
    let second = ProviderScheduler::new(second_pool, domain, config).expect("second scheduler");
    let held = first
        .reserve(ReservationRequest {
            job_id: JobId::new(Uuid::from_u128(11)),
            stage_id: None,
            attempt: 1,
            fence: "held-fence".into(),
            priority: JobPriority::Normal,
            units: 1,
        })
        .await
        .expect("held grant");
    assert!(held.granted);

    let waiter = tokio::spawn(async move {
        call_reserved::<(), _, &'static str, _, _>(
            &second,
            ReservationRequest {
                job_id: JobId::new(Uuid::from_u128(12)),
                stage_id: None,
                attempt: 1,
                fence: "waiter-fence".into(),
                priority: JobPriority::Interactive,
                units: 1,
            },
            |_lease| async { Ok("waiter-ran") },
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!waiter.is_finished(), "waiter should remain durably queued");
    first
        .complete(&held.reservation_id, "held-fence")
        .await
        .expect("release held capacity");
    let result = tokio::time::timeout(Duration::from_secs(3), waiter)
        .await
        .expect("waiter observed release before deadline")
        .expect("waiter task")
        .expect("reserved call");
    assert_eq!(result, "waiter-ran");
    let queued: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM provider_reservations WHERE status = 'queued'")
            .fetch_one(&first_pool)
            .await
            .expect("queued count");
    assert_eq!(queued, 0);
}

#[tokio::test]
async fn cross_process_recovery_is_independent_for_domains_sharing_an_authority() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("scheduler-domains.db");
    let pool = open_sqlite_pool(path.to_str().unwrap())
        .await
        .expect("scheduler pool");
    sqlx::query(
        "INSERT INTO sources (source_id, summary_json, created_at, updated_at)
         VALUES ('domain-source', '{}', '', '')",
    )
    .execute(&pool)
    .await
    .expect("source");
    for suffix in [31_u128, 32_u128] {
        sqlx::query(
            "INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at)
             VALUES (?, 'source', 'queued', 'queued', 'normal', 'domain-source', '', '')",
        )
        .bind(Uuid::from_u128(suffix).to_string())
        .execute(&pool)
        .await
        .expect("job");
    }
    let config = SchedulerConfig {
        capacity: 1,
        interactive_reserve: 0,
        max_entries: 8,
        max_units: 8,
    };
    let embedding = ProviderScheduler::new(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "tei".into(),
            authority_id: "shared-authority".into(),
        },
        config,
    )
    .expect("embedding scheduler");
    let vector = ProviderScheduler::new(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Vector,
            instance_id: "qdrant".into(),
            authority_id: "shared-authority".into(),
        },
        config,
    )
    .expect("vector scheduler");
    let held = vector
        .reserve(ReservationRequest {
            job_id: JobId::new(Uuid::from_u128(31)),
            stage_id: None,
            attempt: 1,
            fence: "vector-held".into(),
            priority: JobPriority::Normal,
            units: 1,
        })
        .await
        .expect("held vector grant");

    // Model another provider domain whose recovery pass is stalled. This must
    // not prevent the vector domain from electing its own recovery dispatcher.
    let unrelated_recovery = embedding
        .dispatch_signal
        .try_claim_recovery()
        .expect("embedding recovery leader");
    let waiter = tokio::spawn(async move {
        call_reserved::<(), _, &'static str, _, _>(
            &vector,
            ReservationRequest {
                job_id: JobId::new(Uuid::from_u128(32)),
                stage_id: None,
                attempt: 1,
                fence: "vector-waiter".into(),
                priority: JobPriority::Normal,
                units: 1,
            },
            |_lease| async { Ok("vector-ran") },
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    sqlx::query(
        "UPDATE provider_reservations SET status = 'released', granted_units = 0
         WHERE reservation_id = ?",
    )
    .bind(&held.reservation_id)
    .execute(&pool)
    .await
    .expect("external release");

    let result = tokio::time::timeout(Duration::from_secs(7), waiter)
        .await
        .expect("vector domain recovered independently")
        .expect("waiter task")
        .expect("reserved call");
    assert_eq!(result, "vector-ran");
    drop(unrelated_recovery);
}

#[tokio::test]
async fn cross_process_dispatch_preserves_the_waiters_authority() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("scheduler-authorities.db");
    let owner_pool = open_sqlite_pool(path.to_str().unwrap())
        .await
        .expect("owner pool");
    let dispatcher_pool = open_sqlite_pool(path.to_str().unwrap())
        .await
        .expect("dispatcher pool");
    sqlx::query(
        "INSERT INTO sources (source_id, summary_json, created_at, updated_at)
         VALUES ('authority-source', '{}', '', '')",
    )
    .execute(&owner_pool)
    .await
    .expect("source");
    for suffix in [41_u128, 42_u128] {
        sqlx::query(
            "INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at)
             VALUES (?, 'source', 'queued', 'queued', 'normal', 'authority-source', '', '')",
        )
        .bind(Uuid::from_u128(suffix).to_string())
        .execute(&owner_pool)
        .await
        .expect("job");
    }
    let config = SchedulerConfig {
        capacity: 1,
        interactive_reserve: 0,
        max_entries: 8,
        max_units: 8,
    };
    let dispatcher = ProviderScheduler::new(
        dispatcher_pool,
        ProviderCapacityDomain {
            kind: ProviderKind::Vector,
            instance_id: "shared-qdrant".into(),
            authority_id: "dispatcher-authority".into(),
        },
        config,
    )
    .expect("dispatcher scheduler");
    let owner = ProviderScheduler::new(
        owner_pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Vector,
            instance_id: "shared-qdrant".into(),
            authority_id: "waiter-authority".into(),
        },
        config,
    )
    .expect("owner scheduler");
    let held = dispatcher
        .reserve(ReservationRequest {
            job_id: JobId::new(Uuid::from_u128(41)),
            stage_id: None,
            attempt: 1,
            fence: "dispatcher-held".into(),
            priority: JobPriority::Normal,
            units: 1,
        })
        .await
        .expect("held grant");
    let queued = owner
        .reserve(ReservationRequest {
            job_id: JobId::new(Uuid::from_u128(42)),
            stage_id: None,
            attempt: 1,
            fence: "owner-fence".into(),
            priority: JobPriority::Normal,
            units: 1,
        })
        .await
        .expect("queued reservation");
    assert!(!queued.granted);

    dispatcher
        .complete(&held.reservation_id, "dispatcher-held")
        .await
        .expect("release capacity");
    dispatcher
        .dispatch_queued()
        .await
        .expect("cross-process dispatch");

    let grant = owner
        .reservation_grant(&queued.reservation_id)
        .await
        .expect("original authority still owns grant");
    assert!(grant.granted);
    owner
        .activate(&queued.reservation_id, "owner-fence")
        .await
        .expect("original authority activates grant");
    owner
        .complete(&queued.reservation_id, "owner-fence")
        .await
        .expect("original authority releases capacity");
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM provider_reservations
         WHERE status IN ('granted', 'active') AND granted_units > 0",
    )
    .fetch_one(&owner_pool)
    .await
    .expect("active capacity count");
    assert_eq!(active, 0);
}

#[tokio::test]
async fn capacity_release_wakes_a_waiter_owned_by_another_authority() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    sqlx::query(
        "INSERT INTO sources (source_id, summary_json, created_at, updated_at)
         VALUES ('cross-authority-source', '{}', '', '')",
    )
    .execute(&pool)
    .await
    .expect("source");
    for suffix in [81_u128, 82_u128] {
        sqlx::query(
            "INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at)
             VALUES (?, 'source', 'queued', 'queued', 'normal', 'cross-authority-source', '', '')",
        )
        .bind(Uuid::from_u128(suffix).to_string())
        .execute(&pool)
        .await
        .expect("job");
    }
    let config = SchedulerConfig {
        capacity: 1,
        interactive_reserve: 0,
        max_entries: 8,
        max_units: 8,
    };
    let make = |authority: &str| {
        ProviderScheduler::new(
            pool.clone(),
            ProviderCapacityDomain {
                kind: ProviderKind::Vector,
                instance_id: "cross-authority-qdrant".into(),
                authority_id: authority.into(),
            },
            config,
        )
        .expect("scheduler")
    };
    let owner = make("owner-authority");
    let waiter_scheduler = make("waiter-authority");
    let held = owner
        .reserve(request(81, "owner-held", JobPriority::Normal))
        .await
        .expect("held");
    assert!(held.granted);

    let waiter = tokio::spawn(async move {
        waiter_scheduler
            .reserve_wait(request(82, "other-waiter", JobPriority::Normal))
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    owner
        .complete(&held.reservation_id, "owner-held")
        .await
        .expect("release");
    let grant = tokio::time::timeout(Duration::from_secs(1), waiter)
        .await
        .expect("cross-authority waiter should wake without recovery poll")
        .expect("waiter task")
        .expect("grant");
    assert!(grant.granted);
}

#[tokio::test]
async fn dropping_a_waiter_cancels_its_durable_queue_row() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    sqlx::query(
        "INSERT INTO sources (source_id, summary_json, created_at, updated_at)
         VALUES ('drop-waiter-source', '{}', '', '')",
    )
    .execute(&pool)
    .await
    .expect("source");
    for suffix in [21_u128, 22_u128] {
        sqlx::query(
            "INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at)
             VALUES (?, 'source', 'queued', 'queued', 'normal', 'drop-waiter-source', '', '')",
        )
        .bind(Uuid::from_u128(suffix).to_string())
        .execute(&pool)
        .await
        .expect("job");
    }
    let scheduler = ProviderScheduler::new(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "tei-drop".into(),
            authority_id: "authority-drop".into(),
        },
        SchedulerConfig {
            capacity: 1,
            interactive_reserve: 0,
            max_entries: 8,
            max_units: 8,
        },
    )
    .expect("scheduler");
    let held = scheduler
        .reserve(ReservationRequest {
            job_id: JobId::new(Uuid::from_u128(21)),
            stage_id: None,
            attempt: 1,
            fence: "held-drop-fence".into(),
            priority: JobPriority::Normal,
            units: 1,
        })
        .await
        .expect("held grant");
    assert!(held.granted);

    let waiting_scheduler = scheduler.clone();
    let waiter = tokio::spawn(async move {
        waiting_scheduler
            .reserve_wait(ReservationRequest {
                job_id: JobId::new(Uuid::from_u128(22)),
                stage_id: None,
                attempt: 1,
                fence: "dropped-waiter-fence".into(),
                priority: JobPriority::Interactive,
                units: 1,
            })
            .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    waiter.abort();
    let _ = waiter.await;

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let row: Option<(String, String)> = sqlx::query_as(
                "SELECT status, terminal_reason FROM provider_reservations WHERE fence = ?",
            )
            .bind("dropped-waiter-fence")
            .fetch_optional(&pool)
            .await
            .expect("waiter row");
            if row == Some(("canceled".into(), "waiter_dropped".into())) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("dropped waiter was canceled");

    scheduler
        .complete(&held.reservation_id, "held-drop-fence")
        .await
        .expect("release held grant");
}

#[tokio::test]
async fn reserved_call_renews_long_running_active_lease() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    sqlx::query(
        "INSERT INTO sources (source_id, summary_json, created_at, updated_at)
         VALUES ('renew-source', '{}', '', '')",
    )
    .execute(&pool)
    .await
    .expect("source");
    sqlx::query(
        "INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at)
         VALUES ('00000000-0000-0000-0000-00000000001f', 'source', 'running', 'embedding', 'normal', 'renew-source', '', '')",
    )
    .execute(&pool)
    .await
    .expect("job");
    let scheduler = ProviderScheduler::new(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "tei-renew".into(),
            authority_id: "authority-renew".into(),
        },
        SchedulerConfig {
            capacity: 1,
            interactive_reserve: 0,
            max_entries: 8,
            max_units: 8,
        },
    )
    .expect("scheduler");
    let task_scheduler = scheduler.clone();
    let task = tokio::spawn(async move {
        call_reserved::<(), _, &'static str, _, _>(
            &task_scheduler,
            ReservationRequest {
                job_id: JobId::new(Uuid::from_u128(31)),
                stage_id: None,
                attempt: 1,
                fence: "renew-fence".into(),
                priority: JobPriority::Normal,
                units: 1,
            },
            |_lease| async move {
                tokio::time::sleep(Duration::from_millis(180)).await;
                Ok("done")
            },
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let active: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM provider_reservations WHERE fence = ? AND status = 'active'",
            )
            .bind("renew-fence")
            .fetch_one(&pool)
            .await
            .expect("active count");
            if active == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("reservation activated");

    sqlx::query(
        "UPDATE provider_reservations SET renewed_at = datetime('now', '-61 seconds'),
         expires_at = datetime('now', '-1 second') WHERE fence = ?",
    )
    .bind("renew-fence")
    .execute(&pool)
    .await
    .expect("age active lease");
    tokio::time::sleep(Duration::from_millis(50)).await;
    let reconciliation = scheduler.reconcile().await.expect("reconcile");
    assert_eq!(reconciliation.quarantined_active, 0);

    let result = task.await.expect("task join").expect("reserved call");
    assert_eq!(result, "done");
    let row: (String, i64) =
        sqlx::query_as("SELECT status, quarantined FROM provider_reservations WHERE fence = ?")
            .bind("renew-fence")
            .fetch_one(&pool)
            .await
            .expect("reservation row");
    assert_eq!(row, ("released".into(), 0));
}

async fn seed_jobs(pool: &SqlitePool, source_id: &str, suffixes: &[u128]) {
    sqlx::query(
        "INSERT INTO sources (source_id, summary_json, created_at, updated_at)
         VALUES (?, '{}', '', '')",
    )
    .bind(source_id)
    .execute(pool)
    .await
    .expect("source");
    for suffix in suffixes {
        sqlx::query(
            "INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at)
             VALUES (?, 'source', 'queued', 'queued', 'normal', ?, '', '')",
        )
        .bind(Uuid::from_u128(*suffix).to_string())
        .bind(source_id)
        .execute(pool)
        .await
        .expect("job");
    }
}

fn test_scheduler(pool: &SqlitePool, instance_id: &str) -> ProviderScheduler {
    ProviderScheduler::new(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: instance_id.into(),
            authority_id: "a".into(),
        },
        SchedulerConfig {
            capacity: 1,
            interactive_reserve: 0,
            max_entries: 8,
            max_units: 8,
        },
    )
    .expect("scheduler")
}

fn request(suffix: u128, fence: &str, priority: JobPriority) -> ReservationRequest {
    ReservationRequest {
        job_id: JobId::new(Uuid::from_u128(suffix)),
        stage_id: None,
        attempt: 1,
        fence: fence.into(),
        priority,
        units: 1,
    }
}

async fn wait_for_status(pool: &SqlitePool, fence: &str, status: &str) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM provider_reservations WHERE fence = ? AND status = ?",
            )
            .bind(fence)
            .bind(status)
            .fetch_one(pool)
            .await
            .expect("status count");
            if count == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("fence {fence} never reached status {status}"));
}

#[tokio::test]
async fn dropping_active_reserved_call_releases_capacity() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    seed_jobs(&pool, "drop-active-source", &[41, 42]).await;
    let scheduler = test_scheduler(&pool, "tei-drop-active");
    let task_scheduler = scheduler.clone();
    let task = tokio::spawn(async move {
        call_reserved::<(), _, &'static str, _, _>(
            &task_scheduler,
            request(41, "dropped-active-fence", JobPriority::Normal),
            |_lease| async move {
                tokio::time::sleep(Duration::from_secs(300)).await;
                Ok("never")
            },
        )
        .await
    });
    wait_for_status(&pool, "dropped-active-fence", "active").await;
    task.abort();
    let _ = task.await;
    wait_for_status(&pool, "dropped-active-fence", "released").await;
    let reason: String = sqlx::query_scalar(
        "SELECT terminal_reason FROM provider_reservations WHERE fence = 'dropped-active-fence'",
    )
    .fetch_one(&pool)
    .await
    .expect("terminal reason");
    assert_eq!(reason, "call_dropped");
    let next = scheduler
        .reserve(request(42, "next-after-drop", JobPriority::Normal))
        .await
        .expect("subsequent reserve");
    assert!(
        next.granted,
        "dropped call must free capacity for the next reserve"
    );
}

#[tokio::test]
async fn renew_failure_releases_capacity_without_wedging_domain() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    seed_jobs(&pool, "renew-fail-source", &[51, 52]).await;
    let scheduler = test_scheduler(&pool, "tei-renew-fail");
    let task_scheduler = scheduler.clone();
    let task = tokio::spawn(async move {
        call_reserved::<(), _, &'static str, _, _>(
            &task_scheduler,
            request(51, "renew-fail-fence", JobPriority::Normal),
            |_lease| async move {
                tokio::time::sleep(Duration::from_secs(300)).await;
                Ok("never")
            },
        )
        .await
    });
    wait_for_status(&pool, "renew-fail-fence", "active").await;
    // Revoke the fence out from under the running call; the next renewal tick
    // must fail, drop the operation, and leave no active row behind.
    let reservation_id: String = sqlx::query_scalar(
        "SELECT reservation_id FROM provider_reservations WHERE fence = 'renew-fail-fence'",
    )
    .fetch_one(&pool)
    .await
    .expect("reservation id");
    scheduler
        .cancel(&reservation_id, "renew-fail-fence")
        .await
        .expect("external cancel");
    let error = task
        .await
        .expect("task join")
        .expect_err("renew failure propagates");
    assert!(matches!(
        error,
        ReservedCallError::Scheduler(SchedulerError::StaleFence)
    ));
    let active: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM provider_reservations WHERE status IN ('granted','active')",
    )
    .fetch_one(&pool)
    .await
    .expect("active count");
    assert_eq!(active, 0);
    let next = scheduler
        .reserve(request(52, "next-after-renew-fail", JobPriority::Normal))
        .await
        .expect("subsequent reserve");
    assert!(next.granted, "renew failure must not wedge the domain");
}

#[tokio::test]
async fn reconcile_terminalizes_stale_quarantined_active_rows() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    seed_jobs(&pool, "stale-active-source", &[61, 62]).await;
    let scheduler = test_scheduler(&pool, "tei-stale-active");
    // A restart leaves an orphaned active row whose holder is gone: same
    // authority (stable per DB path), renewals long stopped.
    sqlx::query(
        "INSERT INTO provider_reservations (
            reservation_id, job_id, provider_kind, priority, requested_units,
            granted_units, status, updated_at, capacity_domain, instance_id,
            authority_id, renewed_at, expires_at, fence
         ) VALUES ('stale-active-reservation', ?, 'embedding', 'normal',
            1, 1, 'active', datetime('now', '-200 seconds'), 'embedding',
            'tei-stale-active', 'a', datetime('now', '-200 seconds'),
            datetime('now', '-100 seconds'), 'stale-active-fence')",
    )
    .bind(Uuid::from_u128(61).to_string())
    .execute(&pool)
    .await
    .expect("stale active reservation");
    let reconciliation = scheduler.reconcile().await.expect("reconcile");
    assert_eq!(reconciliation.quarantined_active, 1);
    assert_eq!(reconciliation.released_quarantined, 1);
    let row: (String, i64, String) = sqlx::query_as(
        "SELECT status, granted_units, terminal_reason FROM provider_reservations
         WHERE reservation_id = 'stale-active-reservation'",
    )
    .fetch_one(&pool)
    .await
    .expect("terminalized row");
    assert_eq!(row, ("expired".into(), 0, "quarantine_expired".into()));
    let next = scheduler
        .reserve(request(62, "after-stale-recovery", JobPriority::Normal))
        .await
        .expect("subsequent reserve");
    assert!(next.granted, "recovered capacity must be grantable again");
}

#[tokio::test]
async fn live_polling_waiter_is_not_expired_and_ages_up() {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    seed_jobs(&pool, "aging-waiter-source", &[71, 72, 73]).await;
    let scheduler = test_scheduler(&pool, "tei-aging");
    let held = scheduler
        .reserve(request(71, "aging-holder-fence", JobPriority::Normal))
        .await
        .expect("holder grant");
    assert!(held.granted);
    let waiter = scheduler
        .reserve(request(72, "aging-waiter-fence", JobPriority::Normal))
        .await
        .expect("waiter enqueue");
    assert!(!waiter.granted);
    // Simulate a waiter that has been queued (and polling) past WAIT_TIMEOUT:
    // insertion age 40s, last poll long ago.
    sqlx::query(
        "UPDATE provider_reservations
         SET updated_at = datetime('now', '-40 seconds'),
             renewed_at = datetime('now', '-40 seconds')
         WHERE reservation_id = ?",
    )
    .bind(&waiter.reservation_id)
    .execute(&pool)
    .await
    .expect("age waiter row");
    // One grant poll refreshes the liveness heartbeat without touching the
    // aging clock.
    let polled = scheduler
        .try_grant_existing(&waiter.reservation_id)
        .await
        .expect("poll");
    assert!(!polled.granted);
    // A third party's reserve() runs abandonment expiry and priority aging.
    let third = scheduler
        .reserve(request(73, "aging-third-fence", JobPriority::Normal))
        .await
        .expect("third-party reserve");
    assert!(!third.granted);
    let (status, effective_priority): (String, String) = sqlx::query_as(
        "SELECT status, effective_priority FROM provider_reservations WHERE reservation_id = ?",
    )
    .bind(&waiter.reservation_id)
    .fetch_one(&pool)
    .await
    .expect("waiter row");
    assert_eq!(
        status, "queued",
        "a live polling waiter must not be expired"
    );
    assert_eq!(
        effective_priority, "high",
        "a 40s-old normal-priority waiter must age up one level"
    );
    scheduler
        .complete(&held.reservation_id, "aging-holder-fence")
        .await
        .expect("release holder");
}

/// Every current `ProviderKind` variant. Kept in lockstep with the
/// `provider_kind` CHECK in `migrations/0009_provider_scheduler_kind_registry.sql`
/// by `provider_kind_registry_is_exhaustive` below — adding an enum variant
/// breaks that witness until both this list and a new migration widen the
/// registry.
const ALL_PROVIDER_KINDS: &[ProviderKind] = &[
    ProviderKind::Llm,
    ProviderKind::Embedding,
    ProviderKind::Vector,
    ProviderKind::Search,
    ProviderKind::Fetch,
    ProviderKind::Render,
    ProviderKind::Parser,
    ProviderKind::NetworkCapture,
    ProviderKind::Artifact,
    ProviderKind::Ledger,
    ProviderKind::Graph,
    ProviderKind::Memory,
    ProviderKind::Job,
    ProviderKind::Watch,
    ProviderKind::Config,
    ProviderKind::Credential,
    ProviderKind::Cache,
    ProviderKind::Security,
    ProviderKind::RateLimiter,
    ProviderKind::HealthProbe,
];

#[test]
fn provider_kind_registry_is_exhaustive() {
    // Compiler-enforced: a new ProviderKind variant fails this match, which is
    // the signal to extend ALL_PROVIDER_KINDS *and* ship a migration widening
    // the provider_reservations provider_kind CHECK (see 0009).
    let witness = |kind: ProviderKind| match kind {
        ProviderKind::Llm
        | ProviderKind::Embedding
        | ProviderKind::Vector
        | ProviderKind::Search
        | ProviderKind::Fetch
        | ProviderKind::Render
        | ProviderKind::Parser
        | ProviderKind::NetworkCapture
        | ProviderKind::Artifact
        | ProviderKind::Ledger
        | ProviderKind::Graph
        | ProviderKind::Memory
        | ProviderKind::Job
        | ProviderKind::Watch
        | ProviderKind::Config
        | ProviderKind::Credential
        | ProviderKind::Cache
        | ProviderKind::Security
        | ProviderKind::RateLimiter
        | ProviderKind::HealthProbe => (),
    };
    for kind in ALL_PROVIDER_KINDS {
        witness(*kind);
    }
    assert_eq!(ALL_PROVIDER_KINDS.len(), 20);
}

#[tokio::test]
async fn every_provider_kind_passes_the_reservation_check_constraint() {
    // Regression for the production failure "CHECK constraint failed:
    // provider_kind IN (...)": the 0004 CHECK lagged the ProviderKind enum, so
    // graph (and other newer) capacity domains could not insert reservations
    // and baseline graph upserts degraded. Reserve once per kind — the insert
    // itself exercises the CHECK.
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    sqlx::query("INSERT INTO sources (source_id, summary_json, created_at, updated_at) VALUES ('s', '{}', '', '')")
        .execute(&pool)
        .await
        .expect("source");
    sqlx::query("INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at) VALUES ('00000000-0000-0000-0000-000000000042', 'source', 'queued', 'queued', 'normal', 's', '', '')")
        .execute(&pool)
        .await
        .expect("job");

    for (index, kind) in ALL_PROVIDER_KINDS.iter().enumerate() {
        let scheduler = ProviderScheduler::new(
            pool.clone(),
            ProviderCapacityDomain {
                kind: *kind,
                instance_id: format!("registry-{index}"),
                authority_id: "authority-registry".into(),
            },
            SchedulerConfig {
                capacity: 1,
                interactive_reserve: 0,
                max_entries: 10,
                max_units: 10,
            },
        )
        .expect("scheduler");
        let grant = scheduler
            .reserve(ReservationRequest {
                job_id: JobId::new(Uuid::from_u128(0x42)),
                stage_id: None,
                attempt: 1,
                fence: format!("fence-registry-{index}"),
                priority: JobPriority::Normal,
                units: 1,
            })
            .await
            .unwrap_or_else(|error| {
                panic!("reserve must pass the provider_kind CHECK for {kind:?}: {error:?}")
            });
        assert!(grant.granted, "reservation for {kind:?} must be granted");
        scheduler
            .complete(&grant.reservation_id, &format!("fence-registry-{index}"))
            .await
            .expect("completion");
    }
}
