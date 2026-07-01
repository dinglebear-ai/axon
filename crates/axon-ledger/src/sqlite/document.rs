use axon_api::source::*;
use sqlx::Row;

use crate::migration::sqlite_error;
use crate::sqlite::SqliteLedgerStore;
use crate::sqlite::util::{enum_wire_value, json_error};
use crate::store::Result;
use crate::validation::source_missing_error;

pub(super) async fn update_document_status(
    store: &SqliteLedgerStore,
    status: DocumentStatus,
) -> Result<()> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM sources WHERE source_id = ?1")
        .bind(&status.source_id.0)
        .fetch_optional(&store.pool)
        .await
        .map_err(sqlite_error)?;
    if exists.is_none() {
        return Err(source_missing_error(&status.source_id));
    }

    let status_json = serde_json::to_string(&status).map_err(json_error)?;
    sqlx::query(
        r#"
        INSERT INTO document_status (
            document_id,
            source_id,
            source_item_key,
            generation,
            status,
            status_json,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(document_id) DO UPDATE SET
            source_id = excluded.source_id,
            source_item_key = excluded.source_item_key,
            generation = excluded.generation,
            status = excluded.status,
            status_json = excluded.status_json,
            updated_at = excluded.updated_at
        WHERE excluded.updated_at >= document_status.updated_at
        "#,
    )
    .bind(&status.document_id.0)
    .bind(&status.source_id.0)
    .bind(&status.source_item_key.0)
    .bind(&status.generation.0)
    .bind(enum_wire_value(status.status)?)
    .bind(status_json)
    .bind(&status.updated_at.0)
    .execute(&store.pool)
    .await
    .map_err(sqlite_error)?;
    Ok(())
}

pub(super) async fn document_status(
    store: &SqliteLedgerStore,
    document_id: &DocumentId,
) -> Result<Option<DocumentStatus>> {
    let row = sqlx::query(
        r#"
        SELECT status_json
        FROM document_status
        WHERE document_id = ?1
        "#,
    )
    .bind(&document_id.0)
    .fetch_optional(&store.pool)
    .await
    .map_err(sqlite_error)?;

    row.map(|row| {
        let status_json: String = row.get("status_json");
        serde_json::from_str(&status_json).map_err(json_error)
    })
    .transpose()
}
