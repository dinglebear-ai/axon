use super::*;
use crate::store::open_sqlite_pool;

async fn fixture(job_ids: &[u128]) -> SqlitePool {
    let pool = open_sqlite_pool(":memory:").await.expect("migrations");
    sqlx::query("INSERT INTO sources (source_id, summary_json, created_at, updated_at) VALUES ('scheduler-fairness-source', '{}', '', '')")
        .execute(&pool).await.expect("source");
    for id in job_ids {
        sqlx::query("INSERT INTO jobs (job_id, kind, status, phase, priority, source_id, created_at, updated_at) VALUES (?, 'source', 'queued', 'queued', 'normal', 'scheduler-fairness-source', '', '')")
            .bind(Uuid::from_u128(*id).to_string())
            .execute(&pool).await.expect("job");
    }
    pool
}

fn scheduler(pool: SqlitePool, capacity: u32, interactive_reserve: u32) -> ProviderScheduler {
    ProviderScheduler::new(
        pool,
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "tei-fairness".into(),
            authority_id: "authority-fairness".into(),
        },
        SchedulerConfig {
            capacity,
            interactive_reserve,
            max_entries: 32,
            max_units: 64,
        },
    )
    .expect("scheduler")
}

fn request(id: u128, fence: &str, priority: JobPriority, units: u32) -> ReservationRequest {
    ReservationRequest {
        job_id: JobId::new(Uuid::from_u128(id)),
        stage_id: None,
        attempt: 1,
        fence: fence.to_string(),
        priority,
        units,
    }
}

#[tokio::test]
async fn equal_priority_waiters_are_granted_fifo_by_enqueue_sequence() {
    let pool = fixture(&[21, 22, 23]).await;
    let scheduler = scheduler(pool.clone(), 1, 0);
    let held = scheduler
        .reserve(request(21, "held", JobPriority::Normal, 1))
        .await
        .expect("held");
    let first = scheduler
        .reserve(request(22, "first", JobPriority::Normal, 1))
        .await
        .expect("first");
    let second = scheduler
        .reserve(request(23, "second", JobPriority::Normal, 1))
        .await
        .expect("second");
    assert!(held.is_granted() && !first.is_granted() && !second.is_granted());

    scheduler
        .complete(held.reservation_id(), "held")
        .await
        .expect("release held");
    assert!(
        scheduler
            .try_grant_existing(first.reservation_id())
            .await
            .expect("grant first")
            .is_granted()
    );
    let second_status: String =
        sqlx::query_scalar("SELECT status FROM provider_reservations WHERE reservation_id = ?")
            .bind(second.reservation_id())
            .fetch_one(&pool)
            .await
            .expect("second status");
    assert_eq!(second_status, "queued");
}

#[tokio::test]
async fn aged_maintenance_work_overtakes_newer_normal_work() {
    let pool = fixture(&[31, 32, 33]).await;
    let scheduler = scheduler(pool.clone(), 1, 0);
    let held = scheduler
        .reserve(request(31, "held", JobPriority::Normal, 1))
        .await
        .expect("held");
    let old = scheduler
        .reserve(request(32, "old-maintenance", JobPriority::Maintenance, 1))
        .await
        .expect("old");
    let new = scheduler
        .reserve(request(33, "new-normal", JobPriority::Normal, 1))
        .await
        .expect("new");
    assert!(held.is_granted() && !old.is_granted() && !new.is_granted());

    sqlx::query("UPDATE provider_reservations SET updated_at = datetime('now', '-121 seconds') WHERE reservation_id = ?")
        .bind(old.reservation_id()).execute(&pool).await.expect("age waiter");
    scheduler
        .complete(held.reservation_id(), "held")
        .await
        .expect("release held");
    assert!(
        scheduler
            .try_grant_existing(old.reservation_id())
            .await
            .expect("grant aged")
            .is_granted()
    );

    let row: (String, String) = sqlx::query_as(
        "SELECT status, effective_priority FROM provider_reservations WHERE reservation_id = ?",
    )
    .bind(old.reservation_id())
    .fetch_one(&pool)
    .await
    .expect("aged row");
    assert_eq!(row, ("granted".into(), "interactive".into()));
    let newer_status: String =
        sqlx::query_scalar("SELECT status FROM provider_reservations WHERE reservation_id = ?")
            .bind(new.reservation_id())
            .fetch_one(&pool)
            .await
            .expect("newer status");
    assert_eq!(newer_status, "queued");
}

#[tokio::test]
async fn corrupt_queued_reservation_timestamp_fails_priority_refresh_closed() {
    let pool = fixture(&[34, 35, 36]).await;
    let scheduler = scheduler(pool.clone(), 1, 0);
    let held = scheduler
        .reserve(request(34, "corrupt-aging-held", JobPriority::Normal, 1))
        .await
        .expect("held");
    let corrupt = scheduler
        .reserve(request(35, "corrupt-aging-waiter", JobPriority::Normal, 1))
        .await
        .expect("queued waiter");
    assert!(held.is_granted() && !corrupt.is_granted());

    sqlx::query(
        "UPDATE provider_reservations SET updated_at = 'not-a-timestamp'
         WHERE reservation_id = ?",
    )
    .bind(corrupt.reservation_id())
    .execute(&pool)
    .await
    .expect("corrupt aging timestamp");

    let error = scheduler
        .reserve(request(36, "corrupt-aging-trigger", JobPriority::Normal, 1))
        .await
        .expect_err("priority refresh must reject corrupt aging state");
    assert!(matches!(error, SchedulerError::DatabaseState(_)));

    let (status, effective_priority): (String, String) = sqlx::query_as(
        "SELECT status, effective_priority FROM provider_reservations
         WHERE reservation_id = ?",
    )
    .bind(corrupt.reservation_id())
    .fetch_one(&pool)
    .await
    .expect("corrupt waiter row");
    assert_eq!(status, "queued");
    assert_eq!(
        effective_priority, "normal",
        "corrupt state must not be silently demoted to maintenance"
    );
}

#[tokio::test]
async fn interactive_waiter_cannot_be_bypassed_by_lower_priority_capacity() {
    let pool = fixture(&[41, 42, 43, 44]).await;
    let scheduler = scheduler(pool.clone(), 2, 1);
    let held_a = scheduler
        .reserve(request(41, "held-a", JobPriority::Normal, 1))
        .await
        .expect("held a");
    let held_b = scheduler
        .reserve(request(42, "held-b", JobPriority::Normal, 1))
        .await
        .expect("held b");
    let interactive = scheduler
        .reserve(request(43, "interactive", JobPriority::Interactive, 1))
        .await
        .expect("interactive");
    let background = scheduler
        .reserve(request(44, "background", JobPriority::Background, 1))
        .await
        .expect("background");
    assert!(
        held_a.is_granted()
            && held_b.is_granted()
            && !interactive.is_granted()
            && !background.is_granted()
    );

    scheduler
        .complete(held_a.reservation_id(), "held-a")
        .await
        .expect("release one");
    let background_probe = scheduler
        .try_grant_existing(background.reservation_id())
        .await
        .expect("probe background");
    assert!(!background_probe.is_granted());
    let interactive_status: String =
        sqlx::query_scalar("SELECT status FROM provider_reservations WHERE reservation_id = ?")
            .bind(interactive.reservation_id())
            .fetch_one(&pool)
            .await
            .expect("interactive status");
    assert_eq!(interactive_status, "granted");
}

#[tokio::test]
async fn later_fitting_waiter_uses_capacity_stranded_by_non_fitting_head() {
    let pool = fixture(&[71, 72, 73]).await;
    let scheduler = scheduler(pool.clone(), 3, 0);
    let held = scheduler
        .reserve(request(71, "held", JobPriority::Normal, 2))
        .await
        .expect("held");
    let head = scheduler
        .reserve(request(72, "large-head", JobPriority::Normal, 2))
        .await
        .expect("head");
    let fitting = scheduler
        .reserve(request(73, "fitting", JobPriority::Normal, 1))
        .await
        .expect("fitting");

    assert!(held.is_granted());
    assert!(
        !head.is_granted(),
        "the large head cannot fit the one remaining unit"
    );
    assert!(
        fitting.is_granted(),
        "a later fitting waiter must use residual capacity"
    );

    scheduler
        .complete(held.reservation_id(), "held")
        .await
        .expect("release held");
    assert!(
        scheduler
            .try_grant_existing(head.reservation_id())
            .await
            .expect("grant head after capacity release")
            .is_granted(),
        "the bypassed head must advance once it fits"
    );
}

#[tokio::test]
async fn non_fitting_head_allows_only_one_durable_bypass() {
    let pool = fixture(&[81, 82, 83, 84]).await;
    let scheduler = scheduler(pool.clone(), 3, 0);
    let held = scheduler
        .reserve(request(81, "held", JobPriority::Normal, 2))
        .await
        .expect("held");
    let head = scheduler
        .reserve(request(82, "large-head", JobPriority::Normal, 2))
        .await
        .expect("head");
    let first = scheduler
        .reserve(request(83, "first-bypass", JobPriority::Normal, 1))
        .await
        .expect("first bypass");
    assert!(held.is_granted() && !head.is_granted() && first.is_granted());

    scheduler
        .complete(first.reservation_id(), "first-bypass")
        .await
        .expect("release first bypass");
    let second = scheduler
        .reserve(request(84, "second-bypass", JobPriority::Normal, 1))
        .await
        .expect("second waiter");
    assert!(
        !second.is_granted(),
        "the durable acquired marker must bound bypasses of the same head"
    );

    scheduler
        .complete(held.reservation_id(), "held")
        .await
        .expect("release held");
    assert!(
        scheduler
            .try_grant_existing(head.reservation_id())
            .await
            .expect("grant protected head")
            .is_granted(),
        "the protected head must receive the next fitting opportunity"
    );
}

#[tokio::test]
async fn fitting_waiter_beyond_sixty_four_blocked_rows_uses_residual_capacity() {
    let ids = (1_000_u128..1_067).collect::<Vec<_>>();
    let pool = fixture(&ids).await;
    let scheduler = ProviderScheduler::new(
        pool,
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "tei-fairness".into(),
            authority_id: "authority-fairness".into(),
        },
        SchedulerConfig {
            capacity: 3,
            interactive_reserve: 0,
            max_entries: 128,
            max_units: 256,
        },
    )
    .expect("scheduler");
    let held = scheduler
        .reserve(request(1_000, "held", JobPriority::Normal, 2))
        .await
        .expect("held");
    assert!(held.is_granted());
    for id in 1_001_u128..1_066 {
        let blocked = scheduler
            .reserve(request(
                id,
                &format!("blocked-{id}"),
                JobPriority::Normal,
                2,
            ))
            .await
            .expect("blocked waiter");
        assert!(!blocked.is_granted());
    }
    let fitting = scheduler
        .reserve(request(1_066, "fitting", JobPriority::Normal, 1))
        .await
        .expect("fitting waiter");
    assert!(
        fitting.is_granted(),
        "the fitting waiter after row 64 must run"
    );
}

#[tokio::test]
async fn embedding_saturation_does_not_consume_vector_capacity() {
    let pool = fixture(&[61, 62]).await;
    let embedding = ProviderScheduler::new(
        pool.clone(),
        ProviderCapacityDomain {
            kind: ProviderKind::Embedding,
            instance_id: "shared-provider".into(),
            authority_id: "authority-fairness".into(),
        },
        SchedulerConfig {
            capacity: 1,
            interactive_reserve: 0,
            max_entries: 32,
            max_units: 64,
        },
    )
    .expect("embedding scheduler");
    let vector = ProviderScheduler::new(
        pool,
        ProviderCapacityDomain {
            kind: ProviderKind::Vector,
            instance_id: "shared-provider".into(),
            authority_id: "authority-fairness".into(),
        },
        SchedulerConfig {
            capacity: 1,
            interactive_reserve: 0,
            max_entries: 32,
            max_units: 64,
        },
    )
    .expect("vector scheduler");

    let embedding_grant = embedding
        .reserve(request(61, "embedding-held", JobPriority::Normal, 1))
        .await
        .expect("embedding reservation");
    assert!(embedding_grant.is_granted());

    let vector_grant = vector
        .reserve(request(
            62,
            "vector-independent",
            JobPriority::Interactive,
            1,
        ))
        .await
        .expect("vector reservation");
    assert!(
        vector_grant.is_granted(),
        "embedding saturation must not starve the independent vector capacity domain"
    );
}

#[tokio::test]
async fn scheduler_hot_queries_use_declared_indexes() {
    let pool = fixture(&[51]).await;
    let plans = [
        (
            "head",
            "EXPLAIN QUERY PLAN SELECT reservation_id, requested_units, effective_priority FROM provider_reservations WHERE capacity_domain = 'embedding' AND instance_id = 'tei-fairness' AND status = 'queued' ORDER BY CASE effective_priority WHEN 'interactive' THEN 0 WHEN 'high' THEN 1 WHEN 'normal' THEN 2 WHEN 'background' THEN 3 ELSE 4 END, enqueue_sequence, reservation_id LIMIT 1",
            "provider_reservations_scheduler_instance_state_idx",
        ),
        (
            "job admission",
            "EXPLAIN QUERY PLAN SELECT COUNT(*) FROM provider_reservations WHERE job_id = '00000000-0000-0000-0000-000000000033' AND status IN ('queued','granted','active')",
            "provider_reservations_scheduler_job_state_idx",
        ),
    ];
    for (label, sql, expected_index) in plans {
        let rows: Vec<(i64, i64, i64, String)> = sqlx::query_as(sql)
            .fetch_all(&pool)
            .await
            .expect("query plan");
        let detail = rows
            .into_iter()
            .map(|(_, _, _, detail)| detail)
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            detail.contains(expected_index),
            "{label} query must use {expected_index}: {detail}"
        );
        assert!(
            !detail.contains("SCAN provider_reservations"),
            "{label} query must not full-scan reservations: {detail}"
        );
    }
}
