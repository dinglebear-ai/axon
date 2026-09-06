//! Bounded node reads and writes for parser-produced graph candidates.

use std::collections::{HashMap, HashSet};

use axon_api::source::SourceId;
use axon_core::redact::{
    DefaultRedactor, RedactionContext, redact_metadata_checked, stamp_redaction_metadata,
};
use sqlx::{QueryBuilder, Row, Sqlite};

use super::super::header::now_timestamp;
use super::super::row::{
    authority_from_str, authority_to_str, metadata_to_json, source_ids_from_json,
    source_ids_to_json,
};
use super::StoreResult;
use crate::authority::{Authority, resolve_authority};
use crate::error::graph_storage_error;
use crate::merge::ResolvedNode;

const NODE_READ_BATCH_SIZE: usize = 900;
const NODE_WRITE_BIND_COUNT: usize = 11;
const NODE_WRITE_BATCH_SIZE: usize = 999 / NODE_WRITE_BIND_COUNT;

#[derive(Clone)]
struct NodeState {
    authority: Authority,
    confidence: f32,
    source_ids: Vec<SourceId>,
    source_id_set: HashSet<String>,
}

struct NodeWrite {
    node_id: String,
    kind: String,
    stable_key: String,
    canonical_uri: String,
    display_name: String,
    authority: &'static str,
    confidence: f64,
    metadata_json: String,
    source_ids_json: String,
    created_at: String,
    updated_at: String,
}

pub(super) async fn upsert_nodes(
    tx: &mut sqlx::SqliteConnection,
    nodes: &[ResolvedNode],
    source_id: &SourceId,
    fallback_confidence: f32,
) -> StoreResult<()> {
    let mut states = fetch_node_states(tx, nodes).await?;
    let mut writes = Vec::with_capacity(nodes.len());
    let redactor = DefaultRedactor::new();
    let context = RedactionContext::graph_evidence();

    for node in nodes {
        let state = states
            .entry(node.node_id.0.clone())
            .or_insert_with(|| NodeState {
                authority: node.authority,
                confidence: fallback_confidence.clamp(0.0, 1.0),
                source_ids: vec![source_id.clone()],
                source_id_set: HashSet::from([source_id.0.clone()]),
            });
        state.authority = resolve_authority(state.authority, node.authority).winner;
        state.confidence = state.confidence.max(fallback_confidence).clamp(0.0, 1.0);
        if state.source_id_set.insert(source_id.0.clone()) {
            state.source_ids.push(source_id.clone());
        }

        let (properties, report) =
            redact_metadata_checked(node.properties.clone(), &context, &redactor)?;
        let properties = stamp_redaction_metadata(properties, &report);
        let now = now_timestamp();
        writes.push(NodeWrite {
            node_id: node.node_id.0.clone(),
            kind: node.kind.clone(),
            stable_key: node.stable_key.clone(),
            canonical_uri: node.canonical_uri.clone(),
            display_name: node.label.clone(),
            authority: authority_to_str(state.authority.to_level()),
            confidence: state.confidence as f64,
            metadata_json: metadata_to_json(&properties)?,
            source_ids_json: source_ids_to_json(&state.source_ids)?,
            created_at: now.clone(),
            updated_at: now,
        });
    }

    for batch in writes.chunks(NODE_WRITE_BATCH_SIZE) {
        execute_node_batch(tx, batch).await?;
    }
    Ok(())
}

async fn fetch_node_states(
    tx: &mut sqlx::SqliteConnection,
    nodes: &[ResolvedNode],
) -> StoreResult<HashMap<String, NodeState>> {
    let mut states = HashMap::new();
    for batch in nodes.chunks(NODE_READ_BATCH_SIZE) {
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT node_id, authority, confidence, source_ids_json FROM graph_nodes WHERE node_id IN (",
        );
        let mut separated = query.separated(", ");
        for node in batch {
            separated.push_bind(&node.node_id.0);
        }
        separated.push_unseparated(")");
        let rows = query
            .build()
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| graph_storage_error(format!("failed to batch read nodes: {e}")))?;
        for row in rows {
            let node_id: String = row.get("node_id");
            let source_ids = source_ids_from_json(&row.get::<String, _>("source_ids_json"))?;
            let source_id_set = source_ids
                .iter()
                .map(|source_id| source_id.0.clone())
                .collect();
            states.insert(
                node_id,
                NodeState {
                    authority: Authority::from_level(authority_from_str(
                        &row.get::<String, _>("authority"),
                    )),
                    confidence: row.get::<f64, _>("confidence") as f32,
                    source_ids,
                    source_id_set,
                },
            );
        }
    }
    Ok(states)
}

async fn execute_node_batch(
    tx: &mut sqlx::SqliteConnection,
    rows: &[NodeWrite],
) -> StoreResult<()> {
    let mut query = QueryBuilder::<Sqlite>::new(
        "INSERT INTO graph_nodes (node_id, kind, stable_key, canonical_uri, display_name, authority, confidence, metadata_json, source_ids_json, created_at, updated_at) ",
    );
    query.push_values(rows, |mut row, value| {
        row.push_bind(&value.node_id)
            .push_bind(&value.kind)
            .push_bind(&value.stable_key)
            .push_bind(&value.canonical_uri)
            .push_bind(&value.display_name)
            .push_bind(value.authority)
            .push_bind(value.confidence)
            .push_bind(&value.metadata_json)
            .push_bind(&value.source_ids_json)
            .push_bind(&value.created_at)
            .push_bind(&value.updated_at);
    });
    query.push(
        " ON CONFLICT(node_id) DO UPDATE SET canonical_uri = excluded.canonical_uri, display_name = excluded.display_name, authority = excluded.authority, confidence = excluded.confidence, metadata_json = excluded.metadata_json, source_ids_json = excluded.source_ids_json, updated_at = excluded.updated_at",
    );
    query
        .build()
        .execute(&mut *tx)
        .await
        .map_err(|e| graph_storage_error(format!("failed to batch upsert nodes: {e}")))?;
    Ok(())
}

#[cfg(test)]
#[path = "nodes_tests.rs"]
mod tests;
