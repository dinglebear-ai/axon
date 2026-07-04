use axon_api::source::*;
use sqlx::Row;

use crate::migration::sqlite_error;
use crate::sqlite::SqliteLedgerStore;
use crate::sqlite::generation::ensure_source_exists_in_tx;
use crate::sqlite::util::{cleanup_selector_hash, enum_wire_value, json_error, timestamp};
use crate::store::Result;
use crate::validation::validate_cleanup_debt;

pub(super) async fn record_cleanup_debt(
    store: &SqliteLedgerStore,
    debt: CleanupDebt,
) -> Result<()> {
    validate_cleanup_debt(&debt)?;
    let mut tx = store.pool.begin().await.map_err(sqlite_error)?;
    ensure_source_exists_in_tx(&mut tx, &debt.source_id).await?;
    if let Some(generation) = &debt.generation {
        ensure_generation_exists_in_tx(&mut tx, &debt.source_id, generation).await?;
    }
    insert_cleanup_debt_in_tx(&mut tx, debt).await?;
    tx.commit().await.map_err(sqlite_error)?;
    Ok(())
}

pub(super) async fn cleanup_debt_count(store: &SqliteLedgerStore) -> Result<usize> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cleanup_debt")
        .fetch_one(&store.pool)
        .await
        .map_err(sqlite_error)?;
    Ok(count as usize)
}

pub(super) async fn cleanup_debt(
    store: &SqliteLedgerStore,
    debt_id: &CleanupDebtId,
) -> Result<Option<CleanupDebt>> {
    let row = sqlx::query(
        r#"
        SELECT debt_json
        FROM cleanup_debt
        WHERE debt_id = ?1
        "#,
    )
    .bind(&debt_id.0)
    .fetch_optional(&store.pool)
    .await
    .map_err(sqlite_error)?;

    row.map(|row| {
        let debt_json: String = row.get("debt_json");
        serde_json::from_str(&debt_json).map_err(json_error)
    })
    .transpose()
}

pub(super) async fn list_pending_cleanup_debt(
    store: &SqliteLedgerStore,
    source_id: &SourceId,
) -> Result<Vec<CleanupDebt>> {
    let rows = sqlx::query(
        r#"
        SELECT debt_json
        FROM cleanup_debt
        WHERE source_id = ?1 AND completed_at IS NULL
        ORDER BY created_at ASC, debt_id ASC
        "#,
    )
    .bind(&source_id.0)
    .fetch_all(&store.pool)
    .await
    .map_err(sqlite_error)?;

    rows.into_iter()
        .map(|row| {
            let debt_json: String = row.get("debt_json");
            serde_json::from_str(&debt_json).map_err(json_error)
        })
        .collect()
}

pub(super) async fn resolve_cleanup_debt(
    store: &SqliteLedgerStore,
    debt_id: &CleanupDebtId,
) -> Result<()> {
    let mut tx = store.pool.begin().await.map_err(sqlite_error)?;
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT debt_json FROM cleanup_debt WHERE debt_id = ?1 AND completed_at IS NULL",
    )
    .bind(&debt_id.0)
    .fetch_optional(&mut *tx)
    .await
    .map_err(sqlite_error)?;

    // Idempotent: unknown or already-resolved debt is a no-op.
    let Some(debt_json) = existing else {
        tx.commit().await.map_err(sqlite_error)?;
        return Ok(());
    };

    let mut debt: CleanupDebt = serde_json::from_str(&debt_json).map_err(json_error)?;
    let now = timestamp();
    debt.status = LifecycleStatus::Completed;
    debt.completed_at = Some(now.clone());
    let updated_json = serde_json::to_string(&debt).map_err(json_error)?;

    sqlx::query(
        r#"
        UPDATE cleanup_debt
        SET status = ?2, completed_at = ?3, debt_json = ?4
        WHERE debt_id = ?1
        "#,
    )
    .bind(&debt_id.0)
    .bind(enum_wire_value(LifecycleStatus::Completed)?)
    .bind(&now.0)
    .bind(updated_json)
    .execute(&mut *tx)
    .await
    .map_err(sqlite_error)?;
    tx.commit().await.map_err(sqlite_error)?;
    Ok(())
}

pub(super) async fn insert_cleanup_debt_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    debt: CleanupDebt,
) -> Result<()> {
    validate_cleanup_debt(&debt)?;
    insert_cleanup_debt_with_conflict(tx, debt, CleanupDebtConflict::UpdateFresh).await
}

pub(super) async fn insert_cleanup_debt_once_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    debt: CleanupDebt,
) -> Result<()> {
    validate_cleanup_debt(&debt)?;
    insert_cleanup_debt_with_conflict(tx, debt, CleanupDebtConflict::DoNothing).await
}

enum CleanupDebtConflict {
    DoNothing,
    UpdateFresh,
}

async fn insert_cleanup_debt_with_conflict(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    debt: CleanupDebt,
    conflict: CleanupDebtConflict,
) -> Result<()> {
    validate_cleanup_debt(&debt)?;
    let debt_json = serde_json::to_string(&debt).map_err(json_error)?;
    let selector_hash = cleanup_selector_hash(&debt.selector)?;
    let generation_key = debt
        .generation
        .as_ref()
        .map(|value| value.0.as_str())
        .unwrap_or("");
    let sql = match conflict {
        CleanupDebtConflict::DoNothing => {
            r#"
            INSERT INTO cleanup_debt (
                debt_id, job_id, source_id, generation, generation_key, kind,
                selector_hash, status, debt_json, attempts, created_at,
                next_retry_at, completed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(source_id, generation_key, kind, selector_hash) DO NOTHING
            "#
        }
        CleanupDebtConflict::UpdateFresh => {
            r#"
            INSERT INTO cleanup_debt (
                debt_id, job_id, source_id, generation, generation_key, kind,
                selector_hash, status, debt_json, attempts, created_at,
                next_retry_at, completed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(source_id, generation_key, kind, selector_hash) DO UPDATE SET
                debt_id = CASE
                    WHEN cleanup_debt.completed_at IS NULL
                        AND excluded.created_at >= cleanup_debt.created_at
                    THEN excluded.debt_id
                    ELSE cleanup_debt.debt_id
                END,
                job_id = CASE
                    WHEN cleanup_debt.completed_at IS NULL
                        AND excluded.created_at >= cleanup_debt.created_at
                    THEN excluded.job_id
                    ELSE cleanup_debt.job_id
                END,
                status = CASE
                    WHEN cleanup_debt.completed_at IS NULL
                        AND excluded.created_at >= cleanup_debt.created_at
                    THEN excluded.status
                    ELSE cleanup_debt.status
                END,
                debt_json = CASE
                    WHEN cleanup_debt.completed_at IS NULL
                        AND excluded.created_at >= cleanup_debt.created_at
                    THEN excluded.debt_json
                    ELSE cleanup_debt.debt_json
                END,
                attempts = CASE
                    WHEN cleanup_debt.completed_at IS NULL
                        AND excluded.created_at >= cleanup_debt.created_at
                    THEN MAX(cleanup_debt.attempts, excluded.attempts)
                    ELSE cleanup_debt.attempts
                END,
                created_at = CASE
                    WHEN cleanup_debt.completed_at IS NULL
                        AND excluded.created_at >= cleanup_debt.created_at
                    THEN excluded.created_at
                    ELSE cleanup_debt.created_at
                END,
                next_retry_at = CASE
                    WHEN cleanup_debt.completed_at IS NULL
                        AND excluded.created_at >= cleanup_debt.created_at
                    THEN excluded.next_retry_at
                    ELSE cleanup_debt.next_retry_at
                END,
                completed_at = CASE
                    WHEN cleanup_debt.completed_at IS NULL
                        AND excluded.created_at >= cleanup_debt.created_at
                    THEN excluded.completed_at
                    ELSE cleanup_debt.completed_at
                END
            "#
        }
    };
    sqlx::query(sql)
        .bind(&debt.debt_id.0)
        .bind(debt.job_id.0.to_string())
        .bind(&debt.source_id.0)
        .bind(debt.generation.as_ref().map(|value| value.0.as_str()))
        .bind(generation_key)
        .bind(enum_wire_value(debt.kind)?)
        .bind(selector_hash)
        .bind(enum_wire_value(debt.status)?)
        .bind(debt_json)
        .bind(i64::from(debt.attempts))
        .bind(&debt.created_at.0)
        .bind(debt.next_retry_at.as_ref().map(|value| value.0.as_str()))
        .bind(debt.completed_at.as_ref().map(|value| value.0.as_str()))
        .execute(&mut **tx)
        .await
        .map_err(sqlite_error)?;
    Ok(())
}

async fn ensure_generation_exists_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> Result<()> {
    let exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM source_generations WHERE source_id = ?1 AND generation = ?2",
    )
    .bind(&source_id.0)
    .bind(&generation.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(sqlite_error)?;
    if exists.is_none() {
        return Err(ApiError::new(
            "source.ledger.generation_missing",
            ErrorStage::Planning,
            format!("generation {} does not exist", generation.0),
        )
        .with_source_id(source_id.0.clone()));
    }
    Ok(())
}
