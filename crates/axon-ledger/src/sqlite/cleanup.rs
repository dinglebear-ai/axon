use axon_api::source::*;
use sqlx::Row;

use crate::migration::sqlite_error;
use crate::sqlite::SqliteLedgerStore;
use crate::sqlite::util::{cleanup_selector_hash, enum_wire_value, json_error};
use crate::store::Result;

pub(super) async fn record_cleanup_debt(
    store: &SqliteLedgerStore,
    debt: CleanupDebt,
) -> Result<()> {
    validate_cleanup_debt(&debt)?;
    let mut tx = store.pool.begin().await.map_err(sqlite_error)?;
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

pub(super) async fn insert_cleanup_debt_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    debt: CleanupDebt,
) -> Result<()> {
    validate_cleanup_debt(&debt)?;
    let debt_json = serde_json::to_string(&debt).map_err(json_error)?;
    let selector_hash = cleanup_selector_hash(&debt.selector)?;
    let generation_key = debt
        .generation
        .as_ref()
        .map(|value| value.0.as_str())
        .unwrap_or("");
    sqlx::query(
        r#"
        INSERT INTO cleanup_debt (
            debt_id,
            job_id,
            source_id,
            generation,
            generation_key,
            kind,
            selector_hash,
            status,
            debt_json,
            attempts,
            created_at,
            next_retry_at,
            completed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(source_id, generation_key, kind, selector_hash) DO UPDATE SET
            debt_id = CASE
                WHEN cleanup_debt.completed_at IS NULL THEN excluded.debt_id
                ELSE cleanup_debt.debt_id
            END,
            job_id = CASE
                WHEN cleanup_debt.completed_at IS NULL THEN excluded.job_id
                ELSE cleanup_debt.job_id
            END,
            status = CASE
                WHEN cleanup_debt.completed_at IS NULL THEN excluded.status
                ELSE cleanup_debt.status
            END,
            debt_json = CASE
                WHEN cleanup_debt.completed_at IS NULL THEN excluded.debt_json
                ELSE cleanup_debt.debt_json
            END,
            attempts = MAX(cleanup_debt.attempts, excluded.attempts),
            next_retry_at = CASE
                WHEN cleanup_debt.completed_at IS NULL THEN excluded.next_retry_at
                ELSE cleanup_debt.next_retry_at
            END,
            completed_at = COALESCE(cleanup_debt.completed_at, excluded.completed_at)
        "#,
    )
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

pub(super) async fn insert_cleanup_debt_once_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    debt: CleanupDebt,
) -> Result<()> {
    validate_cleanup_debt(&debt)?;
    let debt_json = serde_json::to_string(&debt).map_err(json_error)?;
    let selector_hash = cleanup_selector_hash(&debt.selector)?;
    let generation_key = debt
        .generation
        .as_ref()
        .map(|value| value.0.as_str())
        .unwrap_or("");
    sqlx::query(
        r#"
        INSERT INTO cleanup_debt (
            debt_id,
            job_id,
            source_id,
            generation,
            generation_key,
            kind,
            selector_hash,
            status,
            debt_json,
            attempts,
            created_at,
            next_retry_at,
            completed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(source_id, generation_key, kind, selector_hash) DO NOTHING
        "#,
    )
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

fn validate_cleanup_debt(debt: &CleanupDebt) -> Result<()> {
    match &debt.selector {
        CleanupSelector::Source { source_id } if source_id != &debt.source_id => {
            Err(cleanup_selector_mismatch_error(debt))
        }
        CleanupSelector::Generation {
            source_id,
            generation,
        } if source_id != &debt.source_id || Some(generation) != debt.generation.as_ref() => {
            Err(cleanup_selector_mismatch_error(debt))
        }
        CleanupSelector::SourceItem {
            source_id,
            generation,
            ..
        } if source_id != &debt.source_id || Some(generation) != debt.generation.as_ref() => {
            Err(cleanup_selector_mismatch_error(debt))
        }
        _ => Ok(()),
    }
}

fn cleanup_selector_mismatch_error(debt: &CleanupDebt) -> ApiError {
    ApiError::new(
        "source.ledger.cleanup_selector_mismatch",
        ErrorStage::Cleaning,
        "cleanup selector does not match cleanup debt source/generation",
    )
    .with_source_id(debt.source_id.0.clone())
}
