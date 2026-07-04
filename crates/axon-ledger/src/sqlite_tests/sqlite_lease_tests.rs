use super::*;
use crate::migration::migrate_ledger;

#[tokio::test]
async fn sqlite_acquires_conflicts_reclaims_and_releases_leases() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");

    let first = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-a"))
        .await
        .expect("acquire first")
        .expect("first lease");
    let conflict = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-b"))
        .await
        .expect("conflicting acquire");
    assert_eq!(conflict, None);

    let wrong_owner_release = store
        .release_lease(first.lease_id.clone(), "owner-b".to_string())
        .await
        .expect_err("wrong owner cannot release an active lease");
    assert_eq!(
        wrong_owner_release.code.to_string(),
        "source.ledger.lease_owner_mismatch"
    );

    store
        .release_lease(first.lease_id.clone(), "owner-a".to_string())
        .await
        .expect("release first lease");
    let expired = store
        .acquire_lease(lease_request_ttl("source:src_sqlite:refresh", "owner-a", 0))
        .await
        .expect("acquire expired lease")
        .expect("zero ttl lease");
    let reclaimed = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-b"))
        .await
        .expect("reclaim expired")
        .expect("expired lease should be reclaimable");
    assert_ne!(expired.lease_id, reclaimed.lease_id);
    assert_eq!(reclaimed.owner_id, "owner-b");

    let stale_heartbeat = store
        .heartbeat_lease(expired.lease_id.clone(), "owner-a".to_string(), 30)
        .await
        .expect("heartbeat with stale guard");
    assert_eq!(stale_heartbeat, None);
    let stale_release = store
        .release_lease(expired.lease_id, "owner-a".to_string())
        .await
        .expect_err("stale guard cannot release reclaimed lease");
    assert_eq!(
        stale_release.code.to_string(),
        "source.ledger.lease_missing"
    );
    let owner_c_conflict = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-c"))
        .await
        .expect("owner-c conflict");
    assert_eq!(owner_c_conflict, None);

    store
        .release_lease(reclaimed.lease_id.clone(), "owner-b".to_string())
        .await
        .expect("release lease");
    let reacquired = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-a"))
        .await
        .expect("reacquire after release");
    assert!(reacquired.is_some());
}

#[tokio::test]
async fn sqlite_same_owner_can_renew_lease() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    let first = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-a"))
        .await
        .expect("acquire")
        .expect("lease");

    let renewed = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-a"))
        .await
        .expect("renew")
        .expect("renewed lease");

    assert_eq!(renewed.lease_id, first.lease_id);
    assert_eq!(renewed.acquired_at, first.acquired_at);
}

#[tokio::test]
async fn sqlite_rejects_oversized_lease_ttl() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");

    let acquire_err = store
        .acquire_lease(lease_request_ttl(
            "source:src_sqlite:refresh",
            "owner-a",
            u64::MAX,
        ))
        .await
        .expect_err("oversized ttl is rejected");
    assert_eq!(
        acquire_err.code.to_string(),
        "source.ledger.lease_ttl_invalid"
    );

    let first = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-a"))
        .await
        .expect("acquire")
        .expect("lease");
    let heartbeat_err = store
        .heartbeat_lease(first.lease_id, "owner-a".to_string(), u64::MAX)
        .await
        .expect_err("oversized heartbeat ttl is rejected");
    assert_eq!(
        heartbeat_err.code.to_string(),
        "source.ledger.lease_ttl_invalid"
    );
}

#[tokio::test]
async fn sqlite_heartbeat_extends_lease_by_id() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    let first = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-a"))
        .await
        .expect("acquire")
        .expect("lease");

    let heartbeat = store
        .heartbeat_lease(first.lease_id.clone(), "owner-a".to_string(), 30)
        .await
        .expect("heartbeat")
        .expect("lease should still exist");

    assert_eq!(heartbeat.lease_id, first.lease_id);
    assert_eq!(heartbeat.owner_id, "owner-a");
    let first_heartbeat_at =
        chrono::DateTime::parse_from_rfc3339(&first.heartbeat_at.0).expect("first heartbeat");
    let heartbeat_at =
        chrono::DateTime::parse_from_rfc3339(&heartbeat.heartbeat_at.0).expect("heartbeat");
    assert!(heartbeat_at >= first_heartbeat_at);
    let stored_heartbeat_at: String =
        sqlx::query_scalar("SELECT heartbeat_at FROM leases WHERE lease_id = ?1")
            .bind(&heartbeat.lease_id.0)
            .fetch_one(&store.pool)
            .await
            .expect("stored heartbeat_at");
    assert_eq!(stored_heartbeat_at, heartbeat.heartbeat_at.0);
}

#[tokio::test]
async fn sqlite_heartbeat_rejects_expired_lease() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    let first = store
        .acquire_lease(lease_request_ttl("source:src_sqlite:refresh", "owner-a", 0))
        .await
        .expect("acquire")
        .expect("lease");

    let heartbeat = store
        .heartbeat_lease(first.lease_id, "owner-a".to_string(), 30)
        .await
        .expect("heartbeat");

    assert_eq!(heartbeat, None);
}

#[tokio::test]
async fn sqlite_heartbeat_rejects_wrong_owner() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");
    let first = store
        .acquire_lease(lease_request("source:src_sqlite:refresh", "owner-a"))
        .await
        .expect("acquire")
        .expect("lease");

    let heartbeat = store
        .heartbeat_lease(first.lease_id, "owner-b".to_string(), 30)
        .await
        .expect("heartbeat");

    assert_eq!(heartbeat, None);
}

#[tokio::test]
async fn sqlite_migration_creates_required_ledger_tables() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    migrate_ledger(&pool).await.expect("migrate ledger");

    let tables = sqlx::query_scalar::<_, String>(
        r#"
        SELECT name
        FROM sqlite_master
        WHERE type = 'table'
          AND name IN (
            'sources',
            'source_generations',
            'source_items',
            'source_manifests',
            'document_status',
            'cleanup_debt',
            'leases'
          )
        ORDER BY name
        "#,
    )
    .fetch_all(&pool)
    .await
    .expect("table names");

    assert_eq!(
        tables,
        vec![
            "cleanup_debt",
            "document_status",
            "leases",
            "source_generations",
            "source_items",
            "source_manifests",
            "sources",
        ]
    );
}

#[tokio::test]
async fn sqlite_store_enables_foreign_keys() {
    let store = SqliteLedgerStore::in_memory().await.expect("store");

    assert!(store.foreign_keys_enabled().await.expect("foreign keys"));
}
