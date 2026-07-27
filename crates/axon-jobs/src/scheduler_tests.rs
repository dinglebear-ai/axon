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
    sqlx::query("UPDATE provider_reservations SET grant_deadline = datetime('now', '-1 second') WHERE reservation_id = ?")
        .bind(&grant.reservation_id)
        .execute(&pool)
        .await
        .expect("expire grant");
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
}
