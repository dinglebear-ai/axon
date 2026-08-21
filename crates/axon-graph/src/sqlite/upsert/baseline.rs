//! Batched fast path for inferred source-baseline graph candidates.

use std::collections::HashMap;

use axon_api::source::GraphCandidate;
use axon_core::redact::{
    DefaultRedactor, RedactionContext, redact_metadata_checked, stamp_redaction_metadata,
};
use sqlx::{QueryBuilder, Sqlite};

use super::{StoreResult, resolve_edge_indexed, upsert_aliases, upsert_evidence_batch};
use crate::error::graph_storage_error;
use crate::merge::{ResolvedNode, resolve_node};
use crate::sqlite::header::now_timestamp;
use crate::sqlite::row::{authority_to_str, metadata_to_json, source_ids_to_json};

const NODE_BATCH_SIZE: usize = 80;
const EDGE_BATCH_SIZE: usize = 100;

struct BaselineNodeWrite {
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

struct BaselineEdgeWrite {
    edge_id: String,
    kind: String,
    from_node_id: String,
    to_node_id: String,
    authority: &'static str,
    confidence: f64,
    metadata_json: String,
    created_at: String,
    updated_at: String,
}

pub(super) async fn upsert_source_baseline(
    tx: &mut sqlx::SqliteConnection,
    candidate: &GraphCandidate,
) -> StoreResult<(u64, u64, u64)> {
    let resolved_nodes = candidate.nodes.iter().map(resolve_node).collect::<Vec<_>>();
    let redactor = DefaultRedactor::new();
    let context = RedactionContext::graph_evidence();
    let now = now_timestamp();
    let source_ids_json = source_ids_to_json(std::slice::from_ref(&candidate.source_id))?;

    upsert_baseline_nodes(
        tx,
        candidate,
        &resolved_nodes,
        &redactor,
        &context,
        &source_ids_json,
        &now,
    )
    .await?;
    upsert_aliases(tx, &resolved_nodes).await?;
    let (edges_written, evidence_written) =
        upsert_baseline_edges(tx, candidate, &resolved_nodes, &redactor, &context, &now).await?;

    Ok((resolved_nodes.len() as u64, edges_written, evidence_written))
}

#[allow(clippy::too_many_arguments)]
async fn upsert_baseline_nodes(
    tx: &mut sqlx::SqliteConnection,
    candidate: &GraphCandidate,
    resolved_nodes: &[ResolvedNode],
    redactor: &DefaultRedactor,
    context: &RedactionContext,
    source_ids_json: &str,
    now: &str,
) -> StoreResult<()> {
    for nodes in resolved_nodes.chunks(NODE_BATCH_SIZE) {
        let mut rows = Vec::with_capacity(nodes.len());
        for node in nodes {
            let (properties, report) =
                redact_metadata_checked(node.properties.clone(), context, redactor)?;
            let properties = stamp_redaction_metadata(properties, &report);
            rows.push(BaselineNodeWrite {
                node_id: node.node_id.0.clone(),
                kind: node.kind.clone(),
                stable_key: node.stable_key.clone(),
                canonical_uri: node.canonical_uri.clone(),
                display_name: node.label.clone(),
                authority: authority_to_str(node.authority.to_level()),
                confidence: candidate.confidence.clamp(0.0, 1.0) as f64,
                metadata_json: metadata_to_json(&properties)?,
                source_ids_json: source_ids_json.to_string(),
                created_at: now.to_string(),
                updated_at: now.to_string(),
            });
        }
        let mut query = QueryBuilder::<Sqlite>::new(
            "INSERT INTO graph_nodes (node_id, kind, stable_key, canonical_uri, display_name,              authority, confidence, metadata_json, source_ids_json, created_at, updated_at) ",
        );
        query.push_values(&rows, |mut row, value| {
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
            " ON CONFLICT(node_id) DO UPDATE SET              canonical_uri = CASE WHEN graph_nodes.authority = 'inferred'                THEN excluded.canonical_uri ELSE graph_nodes.canonical_uri END,              display_name = CASE WHEN graph_nodes.authority = 'inferred'                THEN excluded.display_name ELSE graph_nodes.display_name END,              authority = graph_nodes.authority,              confidence = MAX(graph_nodes.confidence, excluded.confidence),              metadata_json = CASE WHEN graph_nodes.authority = 'inferred'                THEN json_patch(graph_nodes.metadata_json, excluded.metadata_json)                ELSE json_patch(excluded.metadata_json, graph_nodes.metadata_json) END,              source_ids_json = CASE WHEN EXISTS (               SELECT 1 FROM json_each(graph_nodes.source_ids_json)                WHERE value = json_extract(excluded.source_ids_json, '$[0]')             ) THEN graph_nodes.source_ids_json ELSE json_insert(               graph_nodes.source_ids_json, '$[#]', json_extract(excluded.source_ids_json, '$[0]')             ) END, updated_at = excluded.updated_at",
        );
        query.build().execute(&mut *tx).await.map_err(|error| {
            graph_storage_error(format!("failed to batch upsert baseline nodes: {error}"))
        })?;
    }
    Ok(())
}

async fn upsert_baseline_edges(
    tx: &mut sqlx::SqliteConnection,
    candidate: &GraphCandidate,
    resolved_nodes: &[ResolvedNode],
    redactor: &DefaultRedactor,
    context: &RedactionContext,
    now: &str,
) -> StoreResult<(u64, u64)> {
    let nodes_by_stable_key = resolved_nodes
        .iter()
        .map(|node| (node.stable_key.as_str(), node))
        .collect::<HashMap<_, _>>();
    let evidence_by_id = candidate
        .evidence
        .iter()
        .map(|evidence| (evidence.evidence_id.as_str(), evidence))
        .collect::<HashMap<_, _>>();
    let mut edges_written = 0u64;
    let mut evidence_written = 0u64;

    for edges in candidate.edges.chunks(EDGE_BATCH_SIZE) {
        let mut rows = Vec::with_capacity(edges.len());
        let mut pending_evidence = Vec::new();
        for edge in edges {
            let edge_evidence = edge
                .evidence_ids
                .iter()
                .filter_map(|id| evidence_by_id.get(id.as_str()).copied())
                .collect::<Vec<_>>();
            let Some(resolved) = resolve_edge_indexed(
                edge,
                &nodes_by_stable_key,
                &edge_evidence,
                candidate.confidence,
            ) else {
                continue;
            };
            let (properties, report) =
                redact_metadata_checked(resolved.properties.clone(), context, redactor)?;
            let properties = stamp_redaction_metadata(properties, &report);
            rows.push(BaselineEdgeWrite {
                edge_id: resolved.edge_id.0.clone(),
                kind: resolved.kind.clone(),
                from_node_id: resolved.from_node_id.0.clone(),
                to_node_id: resolved.to_node_id.0.clone(),
                authority: authority_to_str(resolved.authority.to_level()),
                confidence: resolved.confidence.clamp(0.0, 1.0) as f64,
                metadata_json: metadata_to_json(&properties)?,
                created_at: now.to_string(),
                updated_at: now.to_string(),
            });
            edges_written = edges_written.saturating_add(1);
            for evidence in edge_evidence {
                pending_evidence.push((resolved.edge_id.0.clone(), evidence));
                evidence_written = evidence_written.saturating_add(1);
            }
        }
        execute_baseline_edge_batch(tx, &rows).await?;
        upsert_evidence_batch(tx, &pending_evidence).await?;
    }
    Ok((edges_written, evidence_written))
}

async fn execute_baseline_edge_batch(
    tx: &mut sqlx::SqliteConnection,
    rows: &[BaselineEdgeWrite],
) -> StoreResult<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut query = QueryBuilder::<Sqlite>::new(
        "INSERT INTO graph_edges (edge_id, kind, from_node_id, to_node_id, authority,          confidence, metadata_json, created_at, updated_at) ",
    );
    query.push_values(rows, |mut row, value| {
        row.push_bind(&value.edge_id)
            .push_bind(&value.kind)
            .push_bind(&value.from_node_id)
            .push_bind(&value.to_node_id)
            .push_bind(value.authority)
            .push_bind(value.confidence)
            .push_bind(&value.metadata_json)
            .push_bind(&value.created_at)
            .push_bind(&value.updated_at);
    });
    query.push(
        " ON CONFLICT(edge_id) DO UPDATE SET          authority = graph_edges.authority,          confidence = MAX(graph_edges.confidence, excluded.confidence),          metadata_json = CASE WHEN graph_edges.authority = 'inferred'            THEN json_patch(graph_edges.metadata_json, excluded.metadata_json)            ELSE json_patch(excluded.metadata_json, graph_edges.metadata_json) END,          updated_at = excluded.updated_at",
    );
    query.build().execute(&mut *tx).await.map_err(|error| {
        graph_storage_error(format!("failed to batch upsert baseline edges: {error}"))
    })?;
    Ok(())
}
