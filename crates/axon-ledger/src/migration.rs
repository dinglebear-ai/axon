//! SQLite migration helpers for the ledger store.

use crate::store::Result;
use axon_api::source::{ApiError, ErrorStage};
use sqlx::{Executor, Sqlite};

pub async fn migrate_ledger<'e, E>(executor: E) -> Result<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    executor
        .execute(
            r#"
            CREATE TABLE IF NOT EXISTS axon_ledger_sources (
                source_id TEXT PRIMARY KEY NOT NULL,
                summary_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .await
        .map_err(sqlite_error)?;

    Ok(())
}

pub(crate) fn sqlite_error(error: sqlx::Error) -> ApiError {
    ApiError::new(
        "source.ledger.sqlite",
        ErrorStage::Upserting,
        format!("ledger SQLite operation failed: {error}"),
    )
}
