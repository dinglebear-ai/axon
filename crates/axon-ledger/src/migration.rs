//! SQLite migration helpers for the ledger store.

use crate::store::Result;
use axon_api::source::{ApiError, ErrorStage};
use sqlx::{Executor, SqlitePool};

pub async fn migrate_ledger(pool: &SqlitePool) -> Result<()> {
    for statement in [
        r#"
            CREATE TABLE IF NOT EXISTS axon_ledger_sources (
                source_id TEXT PRIMARY KEY NOT NULL,
                committed_generation TEXT,
                summary_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_axon_ledger_sources_canonical_uri
            ON axon_ledger_sources(json_extract(summary_json, '$.canonical_uri'))
            "#,
        r#"
            CREATE TABLE IF NOT EXISTS axon_ledger_generations (
                source_id TEXT NOT NULL,
                generation TEXT NOT NULL,
                sequence INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                generation_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                published_at TEXT,
                PRIMARY KEY (source_id, generation),
                FOREIGN KEY (source_id) REFERENCES axon_ledger_sources(source_id) ON DELETE CASCADE
            )
            "#,
        r#"
            CREATE INDEX IF NOT EXISTS idx_axon_ledger_generations_source_status_created
            ON axon_ledger_generations(source_id, status, created_at)
            "#,
        r#"
            CREATE TABLE IF NOT EXISTS axon_ledger_source_manifests (
                source_id TEXT NOT NULL,
                generation TEXT NOT NULL,
                manifest_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (source_id, generation),
                FOREIGN KEY (source_id) REFERENCES axon_ledger_sources(source_id) ON DELETE CASCADE
            )
            "#,
        r#"
            CREATE TABLE IF NOT EXISTS axon_ledger_source_items (
                source_id TEXT NOT NULL,
                source_item_key TEXT NOT NULL,
                generation TEXT NOT NULL,
                item_canonical_uri TEXT NOT NULL,
                content_hash TEXT,
                version TEXT,
                mtime TEXT,
                item_json TEXT NOT NULL,
                PRIMARY KEY (source_id, generation, source_item_key),
                FOREIGN KEY (source_id, generation)
                    REFERENCES axon_ledger_source_manifests(source_id, generation)
                    ON DELETE CASCADE
            )
            "#,
        r#"
            CREATE INDEX IF NOT EXISTS idx_axon_ledger_source_items_key_generation
            ON axon_ledger_source_items(source_id, source_item_key, generation)
            "#,
        r#"
            CREATE INDEX IF NOT EXISTS idx_axon_ledger_source_items_canonical_uri
            ON axon_ledger_source_items(source_id, item_canonical_uri)
            "#,
    ] {
        pool.execute(statement).await.map_err(sqlite_error)?;
    }

    Ok(())
}

pub(crate) async fn clear_ledger(pool: &SqlitePool) -> Result<()> {
    for statement in [
        "DELETE FROM axon_ledger_source_items",
        "DELETE FROM axon_ledger_source_manifests",
        "DELETE FROM axon_ledger_generations",
        "DELETE FROM axon_ledger_sources",
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
