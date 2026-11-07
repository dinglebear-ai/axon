//! Atomic removal of one item's evidence and only output it solely supported.
use crate::{error::graph_storage_error, store::Result};
use axon_api::source::{GraphDeleteResult, SourceId, SourceItemKey};
use axon_core::sqlite::{ImmediateTx, SqliteWriteGate};
use sqlx::SqlitePool;

pub(super) async fn retire(
    pool: &SqlitePool,
    gate: &SqliteWriteGate,
    source: &SourceId,
    item: &SourceItemKey,
) -> Result<GraphDeleteResult> {
    retire_many(pool, gate, source, std::slice::from_ref(item)).await
}

pub(super) async fn retire_many(
    pool: &SqlitePool,
    gate: &SqliteWriteGate,
    source: &SourceId,
    items: &[SourceItemKey],
) -> Result<GraphDeleteResult> {
    let mut total = GraphDeleteResult::default();
    // Bound write-gate occupancy and release between groups.
    for items in items.chunks(64) {
        let keys =
            serde_json::to_string(&items.iter().map(|item| item.0.as_str()).collect::<Vec<_>>())
                .map_err(|e| {
                    graph_storage_error(format!("failed to encode graph retirement items: {e}"))
                })?;
        let mut tx = ImmediateTx::begin_with_gate(pool, gate)
            .await
            .map_err(error)?;
        // Shared edges survive when any evidence belongs to another source/item.
        total.edges_deleted += sqlx::query("DELETE FROM graph_edges WHERE edge_id IN (SELECT edge_id FROM graph_evidence WHERE source_id = ?1 AND coalesce(json_extract(metadata_json, '$.contained_source_item_key'), source_item_key) IN (SELECT value FROM json_each(?2))) AND NOT EXISTS (SELECT 1 FROM graph_evidence e WHERE e.edge_id = graph_edges.edge_id AND (e.source_id != ?1 OR coalesce(json_extract(e.metadata_json, '$.contained_source_item_key'), e.source_item_key) NOT IN (SELECT value FROM json_each(?2))))")
            .bind(&source.0).bind(&keys).execute(&mut *tx).await.map_err(error)?.rows_affected();
        sqlx::query("DELETE FROM graph_evidence WHERE source_id = ?1 AND coalesce(json_extract(metadata_json, '$.contained_source_item_key'), source_item_key) IN (SELECT value FROM json_each(?2))")
            .bind(&source.0).bind(&keys).execute(&mut *tx).await.map_err(error)?;
        total.nodes_deleted += sqlx::query("DELETE FROM graph_nodes WHERE stable_key IN (SELECT value FROM json_each(?1)) AND json_array_length(source_ids_json) = 1 AND json_extract(source_ids_json, '$[0]') = ?2 AND NOT EXISTS (SELECT 1 FROM graph_edges e WHERE e.from_node_id = graph_nodes.node_id OR e.to_node_id = graph_nodes.node_id)")
            .bind(&keys).bind(&source.0).execute(&mut *tx).await.map_err(error)?.rows_affected();
        tx.commit().await.map_err(error)?;
    }
    Ok(total)
}

fn error(error: sqlx::Error) -> axon_api::source::ApiError {
    graph_storage_error(format!("failed to retire graph item evidence: {error}"))
}
