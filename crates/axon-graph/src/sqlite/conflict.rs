//! Explicit conflict recording for the SQLite graph store.
//!
//! Per source-graph.md "Conflict Rules", competing authoritative claims are not
//! silently overwritten — they are preserved as explicit `graph_conflicts` rows.

use uuid::Uuid;

use super::header::now_timestamp;
use crate::authority::Authority;
use crate::error::graph_storage_error;
use crate::merge::ResolvedEdge;

type StoreResult<T> = Result<T, axon_api::source::ApiError>;

/// Record a conflict on an edge whose incoming authority equals an existing
/// authoritative claim.
pub async fn record_edge_conflict(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    edge: &ResolvedEdge,
    existing_authority: Authority,
) -> StoreResult<()> {
    let conflict_id = format!("conf_{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO graph_conflicts (
            conflict_id, target_kind, target_id, field, existing_value,
            incoming_value, existing_authority, incoming_authority, detected_at
         ) VALUES (?, 'edge', ?, 'authority', ?, ?, ?, ?, ?)",
    )
    .bind(&conflict_id)
    .bind(&edge.edge_id.0)
    .bind(super::row::authority_to_str(existing_authority.to_level()))
    .bind(super::row::authority_to_str(edge.authority.to_level()))
    .bind(super::row::authority_to_str(existing_authority.to_level()))
    .bind(super::row::authority_to_str(edge.authority.to_level()))
    .bind(now_timestamp())
    .execute(&mut **tx)
    .await
    .map_err(|e| graph_storage_error(format!("failed to record edge conflict: {e}")))?;
    Ok(())
}

/// Count conflict rows recorded for a given edge (used in tests/introspection).
pub async fn conflict_count_for_edge(pool: &sqlx::SqlitePool, edge_id: &str) -> StoreResult<u64> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT COUNT(*) AS n FROM graph_conflicts WHERE target_kind = 'edge' AND target_id = ?",
    )
    .bind(edge_id)
    .fetch_one(pool)
    .await
    .map_err(|e| graph_storage_error(format!("failed to count conflicts: {e}")))?;
    Ok(row.get::<i64, _>("n") as u64)
}
