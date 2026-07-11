//! Shared manifest-item lookup for the generation-diff cleanup-debt
//! producers (`stale_cleanup`, `graph_prune`). Split out so both producers
//! read the same data the same way instead of hand-rolling their own query.

use axon_api::source::*;
use sqlx::Row;

use crate::migration::sqlite_error;
use crate::sqlite::util::json_error;
use crate::store::Result;

pub(super) async fn manifest_items_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> Result<Vec<ManifestItem>> {
    let rows = sqlx::query(
        r#"
        SELECT item_json
        FROM source_items
        WHERE source_id = ?1 AND generation = ?2
        "#,
    )
    .bind(&source_id.0)
    .bind(&generation.0)
    .fetch_all(&mut **tx)
    .await
    .map_err(sqlite_error)?;

    rows.into_iter()
        .map(|row| {
            let item_json: String = row.get("item_json");
            serde_json::from_str(&item_json).map_err(json_error)
        })
        .collect()
}
