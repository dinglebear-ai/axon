//! `SqliteGraphStore` — the durable SourceGraph store.
//!
//! A real [`GraphStore`] backed by SQLite (via `sqlx`). Owns the
//! `graph_nodes` / `graph_edges` / `graph_evidence` / `graph_aliases` /
//! `graph_conflicts` tables created by [`crate::migration::ensure_schema`].
//!
//! Contract: `docs/pipeline-unification/crates/axon-graph/CLAUDE.md` and
//! `schemas/graph-schema.md`. Candidates are validated against the closed kind
//! registry, merged idempotently by stable key / edge tuple, and conflicts are
//! preserved as explicit rows rather than silently overwritten.

mod conflict;
mod header;
mod query;
mod resolve;
mod row;
mod upsert;

use async_trait::async_trait;
use axon_api::source::{
    CapabilityBase, GraphCandidate, GraphEdge, GraphEdgeId, GraphNode, GraphNodeId,
    GraphQueryRequest, GraphQueryResult, GraphResolveRequest, GraphResolveResult,
    GraphStoreCapability, GraphWriteResult, HealthStatus, MetadataMap,
};
use sqlx::SqlitePool;

use crate::error::graph_storage_error;
use crate::migration::ensure_schema;
use crate::store::{GraphStore, Result};

/// SQLite-backed durable graph store.
#[derive(Debug, Clone)]
pub struct SqliteGraphStore {
    pool: SqlitePool,
}

impl SqliteGraphStore {
    /// Wrap an existing pool. The caller is responsible for having run
    /// [`ensure_schema`]; prefer [`SqliteGraphStore::connect`] for a
    /// self-contained store.
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Open a pool at `path` (`":memory:"` for tests), create the schema, and
    /// return a ready store.
    pub async fn connect(path: &str) -> Result<Self> {
        let pool = SqlitePool::connect(&sqlite_url(path))
            .await
            .map_err(|e| graph_storage_error(format!("failed to open graph sqlite pool: {e}")))?;
        ensure_schema(&pool).await?;
        Ok(Self { pool })
    }

    /// Access the underlying pool (for tests / introspection).
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Count conflict rows recorded for an edge (introspection/tests).
    pub async fn edge_conflict_count(&self, edge_id: &str) -> Result<u64> {
        conflict::conflict_count_for_edge(&self.pool, edge_id).await
    }
}

/// Build a sqlx SQLite connection URL from a path.
fn sqlite_url(path: &str) -> String {
    if path == ":memory:" {
        "sqlite::memory:".to_string()
    } else {
        format!("sqlite://{path}?mode=rwc")
    }
}

#[async_trait]
impl GraphStore for SqliteGraphStore {
    async fn upsert_candidates(&self, candidates: Vec<GraphCandidate>) -> Result<GraphWriteResult> {
        upsert::upsert_candidates(&self.pool, candidates).await
    }

    async fn get_node(&self, node_id: GraphNodeId) -> Result<Option<GraphNode>> {
        resolve::node_by_id(&self.pool, &node_id).await
    }

    async fn get_edge(&self, edge_id: GraphEdgeId) -> Result<Option<GraphEdge>> {
        let row = sqlx::query("SELECT * FROM graph_edges WHERE edge_id = ?")
            .bind(&edge_id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| graph_storage_error(format!("failed to fetch edge: {e}")))?;
        match row {
            Some(row) => {
                let mut edge = row::edge_from_row(&row)?;
                edge.evidence = query::evidence_for_edge(&self.pool, &edge.edge_id.0).await?;
                Ok(Some(edge))
            }
            None => Ok(None),
        }
    }

    async fn query(&self, request: GraphQueryRequest) -> Result<GraphQueryResult> {
        query::query(&self.pool, request).await
    }

    async fn resolve(&self, request: GraphResolveRequest) -> Result<GraphResolveResult> {
        resolve::resolve(&self.pool, request).await
    }

    async fn reset(&self) -> Result<()> {
        for table in [
            "graph_conflicts",
            "graph_aliases",
            "graph_evidence",
            "graph_edges",
            "graph_nodes",
        ] {
            sqlx::query(&format!("DELETE FROM {table}"))
                .execute(&self.pool)
                .await
                .map_err(|e| graph_storage_error(format!("failed to reset {table}: {e}")))?;
        }
        Ok(())
    }

    async fn capabilities(&self) -> Result<GraphStoreCapability> {
        Ok(CapabilityBase {
            name: "sqlite-graph".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-graph".to_string(),
            health: HealthStatus::Healthy,
            features: vec![
                "candidate_ingest".to_string(),
                "resolve".to_string(),
                "traversal_query".to_string(),
                "conflict_records".to_string(),
            ],
            limits: MetadataMap::new(),
        }
        .into())
    }
}

#[cfg(test)]
#[path = "sqlite_tests.rs"]
mod tests;
