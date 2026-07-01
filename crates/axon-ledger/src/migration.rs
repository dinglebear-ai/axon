//! SQLite migration helpers for the ledger store.

use crate::store::Result;
use axon_api::source::{ApiError, ErrorStage};
use sqlx::{Executor, SqlitePool};

pub async fn migrate_ledger(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("src/migrations")
        .run(pool)
        .await
        .map_err(|error| {
            ApiError::new(
                "source.ledger.migration",
                ErrorStage::Upserting,
                format!("ledger SQLite migration failed: {error}"),
            )
        })?;
    Ok(())
}

pub(crate) async fn clear_ledger(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await.map_err(sqlite_error)?;
    for statement in [
        "DELETE FROM source_items",
        "DELETE FROM document_status",
        "DELETE FROM cleanup_debt",
        "DELETE FROM leases",
        "DELETE FROM source_manifests",
        "DELETE FROM source_generations",
        "DELETE FROM sources",
    ] {
        tx.execute(statement).await.map_err(sqlite_error)?;
    }
    tx.commit().await.map_err(sqlite_error)?;
    Ok(())
}

pub(crate) fn sqlite_error(error: sqlx::Error) -> ApiError {
    ApiError::new(
        "source.ledger.sqlite",
        ErrorStage::Upserting,
        format!("ledger SQLite operation failed: {error}"),
    )
}
