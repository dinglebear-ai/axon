//! SQLite migration helpers for the ledger store.

use crate::store::Result;
use axon_api::source::{ApiError, ErrorStage};
use sqlx::{Executor, SqlitePool};

const LEDGER_SCHEMA: &str = include_str!("migrations/0001_ledger_lifecycle.sql");

pub async fn migrate_ledger(pool: &SqlitePool) -> Result<()> {
    for statement in LEDGER_SCHEMA.split(';').map(str::trim) {
        if statement.is_empty() {
            continue;
        }
        pool.execute(statement).await.map_err(sqlite_error)?;
    }

    Ok(())
}

pub(crate) async fn clear_ledger(pool: &SqlitePool) -> Result<()> {
    for statement in [
        "DELETE FROM source_items",
        "DELETE FROM document_status",
        "DELETE FROM cleanup_debt",
        "DELETE FROM leases",
        "DELETE FROM source_manifests",
        "DELETE FROM source_generations",
        "DELETE FROM sources",
    ] {
        pool.execute(statement).await.map_err(sqlite_error)?;
    }
    Ok(())
}

pub(crate) fn sqlite_error(error: sqlx::Error) -> ApiError {
    ApiError::new(
        "source.ledger.sqlite",
        ErrorStage::Upserting,
        format!("ledger SQLite operation failed: {error}"),
    )
}
