use axon_api::source::*;
use sqlx::Row;

use crate::migration::sqlite_error;
use crate::sqlite::SqliteLedgerStore;
use crate::sqlite::util::json_error;
use crate::store::Result;

pub(super) async fn upsert_source(store: &SqliteLedgerStore, source: SourceSummary) -> Result<()> {
    let source_id = source.source_id.0.clone();
    let summary_json = serde_json::to_string(&source).map_err(json_error)?;
    sqlx::query(
        r#"
        INSERT INTO sources (
            source_id,
            summary_json,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(source_id) DO UPDATE SET
            summary_json = excluded.summary_json,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(source_id)
    .bind(summary_json)
    .bind(source.created_at.0)
    .bind(source.updated_at.0)
    .execute(&store.pool)
    .await
    .map_err(sqlite_error)?;

    Ok(())
}

pub(super) async fn get_source(
    store: &SqliteLedgerStore,
    source_id: SourceId,
) -> Result<Option<SourceSummary>> {
    let row = sqlx::query(
        r#"
        SELECT summary_json
        FROM sources
        WHERE source_id = ?1
        "#,
    )
    .bind(source_id.0)
    .fetch_optional(&store.pool)
    .await
    .map_err(sqlite_error)?;

    row.map(|row| {
        let summary_json: String = row.get("summary_json");
        serde_json::from_str(&summary_json).map_err(json_error)
    })
    .transpose()
}

pub(super) async fn foreign_keys_enabled(store: &SqliteLedgerStore) -> Result<bool> {
    let enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&store.pool)
        .await
        .map_err(sqlite_error)?;
    Ok(enabled == 1)
}
