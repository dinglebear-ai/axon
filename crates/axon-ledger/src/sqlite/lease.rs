use axon_api::source::*;
use sqlx::Row;

use crate::migration::sqlite_error;
use crate::sqlite::SqliteLedgerStore;
use crate::sqlite::util::{
    add_seconds, json_error, timestamp, timestamp_after, timestamp_str_after,
};
use crate::store::Result;

pub(super) async fn acquire_lease(
    store: &SqliteLedgerStore,
    request: LeaseRequest,
) -> Result<Option<LeaseGuard>> {
    let now = timestamp();
    let mut tx = store.pool.begin().await.map_err(sqlite_error)?;
    let existing = sqlx::query(
        r#"
        SELECT lease_id, owner_id, acquired_at, expires_at
        FROM leases
        WHERE lease_key = ?1
        "#,
    )
    .bind(&request.lease_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(sqlite_error)?;

    if let Some(row) = existing {
        let expires_at: String = row.get("expires_at");
        let owner_id: String = row.get("owner_id");
        let lease_id: String = row.get("lease_id");
        let acquired_at: String = row.get("acquired_at");
        if timestamp_str_after(&expires_at, &now.0)? {
            if owner_id != request.owner_id {
                tx.rollback().await.map_err(sqlite_error)?;
                return Ok(None);
            }

            let guard = LeaseGuard {
                lease_id: LeaseId::new(lease_id),
                lease_key: request.lease_key,
                owner_id: request.owner_id,
                expires_at: add_seconds(&now, request.ttl_seconds),
                heartbeat_at: now.clone(),
                acquired_at: Timestamp(acquired_at),
                job_id: request.job_id,
                metadata: request.metadata,
            };
            let lease_json = serde_json::to_string(&guard).map_err(json_error)?;
            sqlx::query(
                r#"
                UPDATE leases
                SET expires_at = ?1,
                    heartbeat_at = ?2,
                    job_id = ?3,
                    lease_json = ?4
                WHERE lease_id = ?5
                "#,
            )
            .bind(&guard.expires_at.0)
            .bind(&guard.heartbeat_at.0)
            .bind(guard.job_id.map(|value| value.0.to_string()))
            .bind(lease_json)
            .bind(&guard.lease_id.0)
            .execute(&mut *tx)
            .await
            .map_err(sqlite_error)?;
            tx.commit().await.map_err(sqlite_error)?;
            return Ok(Some(guard));
        }
        sqlx::query("DELETE FROM leases WHERE lease_id = ?1")
            .bind(lease_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlite_error)?;
    }

    let guard = LeaseGuard {
        lease_id: LeaseId::new(format!("lease_{}", uuid::Uuid::new_v4())),
        lease_key: request.lease_key,
        owner_id: request.owner_id,
        expires_at: add_seconds(&now, request.ttl_seconds),
        heartbeat_at: now.clone(),
        acquired_at: now,
        job_id: request.job_id,
        metadata: request.metadata,
    };
    let lease_json = serde_json::to_string(&guard).map_err(json_error)?;
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO leases (
            lease_id,
            lease_key,
            owner_id,
            acquired_at,
            expires_at,
            heartbeat_at,
            job_id,
            lease_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
    )
    .bind(&guard.lease_id.0)
    .bind(&guard.lease_key)
    .bind(&guard.owner_id)
    .bind(&guard.acquired_at.0)
    .bind(&guard.expires_at.0)
    .bind(&guard.heartbeat_at.0)
    .bind(guard.job_id.map(|value| value.0.to_string()))
    .bind(lease_json)
    .execute(&mut *tx)
    .await
    .map_err(sqlite_error)?;
    let inserted = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM leases
        WHERE lease_id = ?1
        "#,
    )
    .bind(&guard.lease_id.0)
    .fetch_one(&mut *tx)
    .await
    .map_err(sqlite_error)?
        == 1;
    if !inserted {
        tx.rollback().await.map_err(sqlite_error)?;
        return Ok(None);
    }
    tx.commit().await.map_err(sqlite_error)?;
    Ok(Some(guard))
}

pub(super) async fn release_lease(
    store: &SqliteLedgerStore,
    lease_id: LeaseId,
    owner_id: String,
) -> Result<()> {
    let result = sqlx::query("DELETE FROM leases WHERE lease_id = ?1 AND owner_id = ?2")
        .bind(&lease_id.0)
        .bind(&owner_id)
        .execute(&store.pool)
        .await
        .map_err(sqlite_error)?;
    if result.rows_affected() == 1 {
        return Ok(());
    }

    let existing_owner: Option<String> =
        sqlx::query_scalar("SELECT owner_id FROM leases WHERE lease_id = ?1")
            .bind(&lease_id.0)
            .fetch_optional(&store.pool)
            .await
            .map_err(sqlite_error)?;
    if existing_owner.is_some() {
        return Err(ApiError::new(
            "source.ledger.lease_owner_mismatch",
            ErrorStage::Leasing,
            "lease owner does not match release owner",
        ));
    }
    Ok(())
}

pub(super) async fn heartbeat_lease(
    store: &SqliteLedgerStore,
    lease_id: LeaseId,
    owner_id: String,
    ttl_seconds: u64,
) -> Result<Option<LeaseGuard>> {
    let now = timestamp();
    let row = sqlx::query(
        r#"
        SELECT lease_json
        FROM leases
        WHERE lease_id = ?1 AND owner_id = ?2
        "#,
    )
    .bind(&lease_id.0)
    .bind(&owner_id)
    .fetch_optional(&store.pool)
    .await
    .map_err(sqlite_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let lease_json: String = row.get("lease_json");
    let existing: LeaseGuard = serde_json::from_str(&lease_json).map_err(json_error)?;
    if !timestamp_after(&existing.expires_at, &now)? {
        return Ok(None);
    }
    let guard = LeaseGuard {
        heartbeat_at: now.clone(),
        expires_at: add_seconds(&now, ttl_seconds),
        ..existing
    };
    let lease_json = serde_json::to_string(&guard).map_err(json_error)?;
    let result = sqlx::query(
        r#"
        UPDATE leases
        SET expires_at = ?1,
            heartbeat_at = ?2,
            lease_json = ?3
        WHERE lease_id = ?4
          AND owner_id = ?5
          AND expires_at = ?6
        "#,
    )
    .bind(&guard.expires_at.0)
    .bind(&guard.heartbeat_at.0)
    .bind(lease_json)
    .bind(&guard.lease_id.0)
    .bind(&owner_id)
    .bind(&existing.expires_at.0)
    .execute(&store.pool)
    .await
    .map_err(sqlite_error)?;
    if result.rows_affected() != 1 {
        return Ok(None);
    }
    Ok(Some(guard))
}
