use axon_api::source::SourceGeneration;
use sqlx::Row;

use crate::migration::sqlite_error;
use crate::sqlite::util::timestamp;
use crate::store::Result;

pub(super) async fn record_committed_epoch(
    tx: &mut sqlx::SqliteConnection,
    generation: &SourceGeneration,
) -> Result<()> {
    let row = sqlx::query(
        "SELECT sequence FROM source_generations WHERE source_id = ?1 AND generation = ?2",
    )
    .bind(&generation.source_id.0)
    .bind(&generation.generation.0)
    .fetch_one(&mut *tx)
    .await
    .map_err(sqlite_error)?;
    let committed_epoch: i64 = row.get("sequence");
    let previous_epoch: Option<i64> = match &generation.previous_generation {
        Some(previous) => sqlx::query_scalar(
            "SELECT sequence FROM source_generations WHERE source_id = ?1 AND generation = ?2",
        )
        .bind(&generation.source_id.0)
        .bind(&previous.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlite_error)?,
        None => None,
    };
    sqlx::query(
        r#"
        INSERT INTO source_publication_state (
            source_id, committed_epoch, previous_epoch, updated_at
        ) VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(source_id) DO UPDATE SET
            committed_epoch = excluded.committed_epoch,
            previous_epoch = excluded.previous_epoch,
            finalizer_lease_id = NULL,
            finalizer_owner_id = NULL,
            finalizer_expires_at = NULL,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&generation.source_id.0)
    .bind(committed_epoch)
    .bind(previous_epoch)
    .bind(timestamp().0)
    .execute(&mut *tx)
    .await
    .map_err(sqlite_error)?;
    Ok(())
}
