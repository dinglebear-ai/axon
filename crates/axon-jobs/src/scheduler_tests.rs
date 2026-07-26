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
