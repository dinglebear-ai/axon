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
    let mut tx = ImmediateTx::begin_with_gate(pool, gate)
        .await
        .map_err(error)?;
    // Delete an edge only when every supporting evidence row belongs to this
    // item. Manual/no-evidence edges and other items' evidence remain intact.
    let edges_deleted = sqlx::query("DELETE FROM graph_edges WHERE edge_id IN (SELECT edge_id FROM graph_evidence WHERE source_id = ?1 AND coalesce(json_extract(metadata_json, '$.contained_source_item_key'), source_item_key) = ?2) AND NOT EXISTS (SELECT 1 FROM graph_evidence e WHERE e.edge_id = graph_edges.edge_id AND (e.source_id != ?1 OR coalesce(json_extract(e.metadata_json, '$.contained_source_item_key'), e.source_item_key) != ?2))")
        .bind(&source.0).bind(&item.0).execute(&mut *tx).await.map_err(error)?.rows_affected();
    sqlx::query("DELETE FROM graph_evidence WHERE source_id = ?1 AND coalesce(json_extract(metadata_json, '$.contained_source_item_key'), source_item_key) = ?2")
        .bind(&source.0)
        .bind(&item.0)
        .execute(&mut *tx)
        .await
        .map_err(error)?;
    let nodes_deleted = sqlx::query("DELETE FROM graph_nodes WHERE stable_key = ?1 AND json_array_length(source_ids_json) = 1 AND json_extract(source_ids_json, '$[0]') = ?2 AND NOT EXISTS (SELECT 1 FROM graph_edges e WHERE e.from_node_id = graph_nodes.node_id OR e.to_node_id = graph_nodes.node_id)")
        .bind(&item.0).bind(&source.0).execute(&mut *tx).await.map_err(error)?.rows_affected();
    tx.commit().await.map_err(error)?;
    Ok(GraphDeleteResult {
        nodes_deleted,
        edges_deleted,
    })
}

fn error(error: sqlx::Error) -> axon_api::source::ApiError {
    graph_storage_error(format!("failed to retire graph item evidence: {error}"))
}
