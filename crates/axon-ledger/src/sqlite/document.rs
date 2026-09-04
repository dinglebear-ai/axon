use std::collections::HashSet;

use axon_api::source::*;
use axon_core::sqlite::ImmediateTx;
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::migration::sqlite_error;
use crate::sqlite::SqliteLedgerStore;
use crate::sqlite::util::{enum_wire_value, json_error};
use crate::store::Result;
use crate::validation::source_missing_error;

/// Seven bind parameters per status keep each SQLite statement comfortably
/// below the default 999-variable ceiling while still amortizing a pipeline
/// stage into a bounded transaction.
const DOCUMENT_STATUS_TX_BATCH_SIZE: usize = 100;

pub(super) async fn update_document_status(
    store: &SqliteLedgerStore,
    status: DocumentStatus,
) -> Result<()> {
    update_document_statuses(store, vec![status]).await
}

pub(super) async fn update_document_statuses(
    store: &SqliteLedgerStore,
    statuses: Vec<DocumentStatus>,
) -> Result<()> {
    for statuses in statuses.chunks(DOCUMENT_STATUS_TX_BATCH_SIZE) {
        let writes = status_writes(statuses)?;
        let mut tx = ImmediateTx::begin(&store.pool)
            .await
            .map_err(sqlite_error)?;
        validate_status_sources(&mut tx, &writes).await?;
        validate_status_items(&mut tx, &writes).await?;
        upsert_status_batch(&mut tx, &writes).await?;
        tx.commit().await.map_err(sqlite_error)?;
    }
    Ok(())
}

pub(super) async fn publish_document_statuses(
    store: &SqliteLedgerStore,
    source_id: SourceId,
    generation: SourceGenerationId,
    updated_at: Timestamp,
) -> Result<u64> {
    let published = enum_wire_value(DocumentLifecycleStatus::Published)?;
    let result = sqlx::query(
        r#"
        UPDATE document_status
        SET status = ?1,
            status_json = json_set(status_json, '$.status', ?1, '$.updated_at', ?2),
            updated_at = ?2
        WHERE source_id = ?3 AND generation = ?4
        "#,
    )
    .bind(published)
    .bind(&updated_at.0)
    .bind(&source_id.0)
    .bind(&generation.0)
    .execute(&store.pool)
    .await
    .map_err(sqlite_error)?;
    Ok(result.rows_affected())
}

struct StatusWrite<'a> {
    status: &'a DocumentStatus,
    generation: &'a SourceGenerationId,
    status_wire: String,
    status_json: String,
}

fn status_writes(statuses: &[DocumentStatus]) -> Result<Vec<StatusWrite<'_>>> {
    statuses
        .iter()
        .map(|status| {
            let generation = status.generation.as_ref().ok_or_else(|| {
                ApiError::new(
                    "source.ledger.generation_required",
                    ErrorStage::Planning,
                    "document status writes require a source generation",
                )
                .with_source_id(status.source_id.0.clone())
            })?;
            Ok(StatusWrite {
                status,
                generation,
                status_wire: enum_wire_value(status.status)?,
                status_json: serde_json::to_string(status).map_err(json_error)?,
            })
        })
        .collect()
}

async fn validate_status_sources(
    tx: &mut sqlx::SqliteConnection,
    writes: &[StatusWrite<'_>],
) -> Result<()> {
    if writes.is_empty() {
        return Ok(());
    }
    let unique = writes
        .iter()
        .map(|write| write.status.source_id.0.as_str())
        .collect::<HashSet<_>>();
    let mut query =
        QueryBuilder::<Sqlite>::new("SELECT source_id FROM sources WHERE source_id IN (");
    let mut separated = query.separated(", ");
    for source_id in &unique {
        separated.push_bind(*source_id);
    }
    separated.push_unseparated(")");
    let existing = query
        .build()
        .fetch_all(&mut *tx)
        .await
        .map_err(sqlite_error)?
        .into_iter()
        .map(|row| row.get::<String, _>("source_id"))
        .collect::<HashSet<_>>();
    if let Some(missing) = writes
        .iter()
        .find(|write| !existing.contains(&write.status.source_id.0))
    {
        return Err(source_missing_error(&missing.status.source_id));
    }
    Ok(())
}

async fn validate_status_items(
    tx: &mut sqlx::SqliteConnection,
    writes: &[StatusWrite<'_>],
) -> Result<()> {
    if writes.is_empty() {
        return Ok(());
    }
    let mut query =
        QueryBuilder::<Sqlite>::new("WITH wanted(source_id, generation, source_item_key) AS (");
    query.push_values(writes, |mut row, write| {
        row.push_bind(&write.status.source_id.0)
            .push_bind(&write.generation.0)
            .push_bind(&write.status.source_item_key.0);
    });
    query.push(
        ") SELECT wanted.source_id, wanted.generation, wanted.source_item_key \
         FROM wanted LEFT JOIN source_items ON \
           source_items.source_id = wanted.source_id AND \
           source_items.generation = wanted.generation AND \
           source_items.source_item_key = wanted.source_item_key \
         WHERE source_items.source_item_key IS NULL LIMIT 1",
    );
    if let Some(row) = query
        .build()
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlite_error)?
    {
        let source_id: String = row.get("source_id");
        let generation: String = row.get("generation");
        let item_key: String = row.get("source_item_key");
        return Err(ApiError::new(
            "source.ledger.source_item_missing",
            ErrorStage::Planning,
            format!("source item {item_key} does not exist in generation {generation}"),
        )
        .with_source_id(source_id));
    }
    Ok(())
}

async fn upsert_status_batch(
    tx: &mut sqlx::SqliteConnection,
    writes: &[StatusWrite<'_>],
) -> Result<()> {
    if writes.is_empty() {
        return Ok(());
    }
    let mut query = QueryBuilder::<Sqlite>::new(
        "INSERT INTO document_status (document_id, source_id, source_item_key, generation, \
         status, status_json, updated_at) ",
    );
    query.push_values(writes, |mut row, write| {
        row.push_bind(&write.status.document_id.0)
            .push_bind(&write.status.source_id.0)
            .push_bind(&write.status.source_item_key.0)
            .push_bind(&write.generation.0)
            .push_bind(&write.status_wire)
            .push_bind(&write.status_json)
            .push_bind(&write.status.updated_at.0);
    });
    query.push(
        " ON CONFLICT(document_id) DO UPDATE SET \
         source_id = excluded.source_id, source_item_key = excluded.source_item_key, \
         generation = excluded.generation, status = excluded.status, \
         status_json = excluded.status_json, updated_at = excluded.updated_at \
         WHERE excluded.updated_at >= document_status.updated_at",
    );
    query
        .build()
        .execute(&mut *tx)
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
