use super::*;
use crate::store::open_sqlite_pool;

#[test]
fn priority_serialization_matches_scheduler_lane_order() {
    assert_eq!(enum_name(JobPriority::Interactive).unwrap(), "interactive");
    assert_eq!(enum_name(JobPriority::Maintenance).unwrap(), "maintenance");
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
            1, 0, 'queued', datetime('now', '-31 seconds'), 'embedding', 'tei', 'a',
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
