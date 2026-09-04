//! Candidate write path for the SQLite graph store.

use std::collections::HashMap;

use axon_api::source::{
    GraphCandidate, GraphEdgeCandidate, GraphEvidence, GraphWriteResult, MetadataMap, SourceId,
};
use axon_core::redact::{
    DefaultRedactor, RedactionContext, redact_metadata_checked, stamp_redaction_metadata,
};
use axon_core::sqlite::ImmediateTx;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::header::{now_timestamp, stage_header};
use super::row::{authority_to_str, metadata_from_json, metadata_to_json};
use crate::authority::{Authority, resolve_authority};
use crate::candidate::validate_candidate;
use crate::error::{graph_storage_error, graph_validation_error};
use crate::merge::{
    ResolvedEdge, ResolvedNode, authority_from_evidence, confidence_from_evidence, edge_id_for,
    resolve_node,
};

type StoreResult<T> = Result<T, axon_api::source::ApiError>;
const SQLITE_SAFE_BIND_LIMIT: usize = 900;
const EDGE_READ_BATCH_SIZE: usize = SQLITE_SAFE_BIND_LIMIT;
const EDGE_WRITE_BINDS_PER_ROW: usize = 9;
const EDGE_WRITE_BATCH_SIZE: usize = SQLITE_SAFE_BIND_LIMIT / EDGE_WRITE_BINDS_PER_ROW;

#[cfg(test)]
fn batch_sizes(total: usize, size: usize) -> Vec<usize> {
    let full = total / size;
    let mut batches = vec![size; full];
    if !total.is_multiple_of(size) {
        batches.push(total % size);
    }
    batches
}

#[cfg(test)]
fn edge_read_batch_sizes(total: usize) -> Vec<usize> {
    batch_sizes(total, EDGE_READ_BATCH_SIZE)
}

#[cfg(test)]
fn edge_write_batch_sizes(total: usize) -> Vec<usize> {
    batch_sizes(total, EDGE_WRITE_BATCH_SIZE)
}

mod evidence;
mod nodes;

use evidence::upsert_evidence_batch;

#[cfg(test)]
#[path = "upsert_tests.rs"]
mod tests;

/// Write a batch of validated candidates into the durable graph.
///
/// Each candidate is validated first; a rejection fails the whole batch. Then
/// nodes are upserted by stable key, edges by tuple (merging evidence), and
/// aliases populated for resolution. Runs in a single transaction so a partial
/// batch never lands.
pub async fn upsert_candidates(
    pool: &SqlitePool,
    candidates: Vec<GraphCandidate>,
) -> StoreResult<GraphWriteResult> {
    upsert_candidate_iter(pool, candidates).await
}

pub async fn upsert_candidate_iter<I>(
    pool: &SqlitePool,
    candidates: I,
) -> StoreResult<GraphWriteResult>
where
    I: IntoIterator<Item = GraphCandidate>,
{
    let candidates = candidates.into_iter().collect::<Vec<_>>();
    let source_id = prevalidate_candidate_batch(&candidates)?;
    let mut tx = ImmediateTx::begin(pool)
        .await
        .map_err(|e| graph_storage_error(format!("failed to open graph transaction: {e}")))?;

    let mut candidates_seen = 0u64;
    let mut nodes_upserted = 0u64;
    let mut edges_upserted = 0u64;
    let mut evidence_records = 0u64;
    for candidate in candidates {
        candidates_seen = candidates_seen.saturating_add(1);
        let (resolved_nodes, resolved_edges) = resolve_candidate(&candidate);

        nodes::upsert_nodes(
            &mut tx,
            &resolved_nodes,
            &candidate.source_id,
            candidate.confidence,
        )
        .await?;
        nodes_upserted = nodes_upserted.saturating_add(resolved_nodes.len() as u64);
        upsert_aliases(&mut tx, &resolved_nodes).await?;

        let mut pending_evidence = Vec::new();
        for edge_batch in resolved_edges.chunks(EDGE_WRITE_BATCH_SIZE) {
            let existing_edges = fetch_edge_states(&mut tx, edge_batch).await?;
            let mut edge_writes = Vec::with_capacity(edge_batch.len());
            pending_evidence.clear();
            for (resolved, edge_evidence) in edge_batch {
                edge_writes.push(
                    prepare_edge_write(
                        &mut tx,
                        resolved,
                        existing_edges.get(&resolved.edge_id.0).cloned(),
                    )
                    .await?,
                );
                edges_upserted += 1;
                for ev in edge_evidence {
                    pending_evidence.push((resolved.edge_id.0.clone(), *ev));
                    evidence_records += 1;
                }
            }
            upsert_edge_batch(&mut tx, &edge_writes).await?;
            upsert_evidence_batch(&mut tx, &pending_evidence).await?;
        }
        // Checkpoints were written by the former partial-commit implementation.
        // Atomic candidates never consult them: doing so could silently skip a
        // changed/reordered edge prefix. Delete matching legacy state only as
        // part of the same transaction that durably writes the whole candidate.
        sqlx::query("DELETE FROM graph_write_checkpoints WHERE job_id = ? AND candidate_id = ?")
            .bind(candidate.job_id.0.to_string())
            .bind(&candidate.candidate_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                graph_storage_error(format!("failed to clear graph checkpoint: {error}"))
            })?;
    }

    tx.commit()
        .await
        .map_err(|e| graph_storage_error(format!("failed to commit graph transaction: {e}")))?;

    Ok(GraphWriteResult {
        header: stage_header(),
        source_id: source_id.unwrap_or_else(|| SourceId::new("graph")),
        candidates_seen,
        nodes_upserted,
        edges_upserted,
        evidence_records,
        warnings: Vec::new(),
    })
}

type ResolvedEdgeEvidence<'a> = (ResolvedEdge, Vec<&'a GraphEvidence>);

fn resolve_candidate(
    candidate: &GraphCandidate,
) -> (Vec<ResolvedNode>, Vec<ResolvedEdgeEvidence<'_>>) {
    let nodes = candidate.nodes.iter().map(resolve_node).collect::<Vec<_>>();
    let nodes_by_key = nodes
        .iter()
        .map(|node| (node.stable_key.as_str(), node))
        .collect::<HashMap<_, _>>();
    let evidence_by_id = candidate
        .evidence
        .iter()
        .map(|evidence| (evidence.evidence_id.as_str(), evidence))
        .collect::<HashMap<_, _>>();
    let edges = candidate
        .edges
        .iter()
        .filter_map(|edge| {
            let evidence = edge
                .evidence_ids
                .iter()
                .filter_map(|id| evidence_by_id.get(id.as_str()).copied())
                .collect::<Vec<_>>();
            resolve_edge_indexed(edge, &nodes_by_key, &evidence, candidate.confidence)
                .map(|resolved| (resolved, evidence))
        })
        .collect();
    (nodes, edges)
}

fn prevalidate_candidate_batch(candidates: &[GraphCandidate]) -> StoreResult<Option<SourceId>> {
    let source_id = candidates
        .first()
        .map(|candidate| candidate.source_id.clone());
    for candidate in candidates {
        validate_candidate(candidate)?;
        if source_id.as_ref() != Some(&candidate.source_id) {
            return Err(graph_validation_error(
                "mixed-source graph batches require per-source receipts",
            ));
        }
    }
    Ok(source_id)
}

fn resolve_edge_indexed(
    edge: &GraphEdgeCandidate,
    nodes_by_stable_key: &HashMap<&str, &ResolvedNode>,
    evidence: &[&GraphEvidence],
    fallback_confidence: f32,
) -> Option<ResolvedEdge> {
    let from = nodes_by_stable_key
        .get(edge.from_stable_key.as_str())?
        .node_id
        .clone();
    let to = nodes_by_stable_key
        .get(edge.to_stable_key.as_str())?
        .node_id
        .clone();
    let evidence = evidence
        .iter()
        .map(|evidence| (*evidence).clone())
        .collect::<Vec<_>>();
    Some(ResolvedEdge {
        edge_id: edge_id_for(&edge.edge_kind, &from, &to),
        kind: edge.edge_kind.clone(),
        from_node_id: from,
        to_node_id: to,
        authority: authority_from_evidence(&evidence),
        confidence: confidence_from_evidence(&evidence, fallback_confidence),
        properties: edge.properties.clone(),
    })
}

/// Upsert one edge by (kind, from, to). On conflict the authority is resolved
/// under keep-highest-authority; equal authoritative claims record a conflict.
struct EdgeWrite {
    edge_id: String,
    kind: String,
    from_node_id: String,
    to_node_id: String,
    authority: String,
    confidence: f64,
    metadata_json: String,
    created_at: String,
    updated_at: String,
}

#[derive(Clone)]
struct ExistingEdgeState {
    authority: Authority,
    confidence: f32,
    metadata: MetadataMap,
}

async fn prepare_edge_write(
    tx: &mut sqlx::SqliteConnection,
    edge: &ResolvedEdge,
    existing: Option<ExistingEdgeState>,
) -> StoreResult<EdgeWrite> {
    let now = now_timestamp();
    let (redacted_properties, redaction_report) = redact_metadata_checked(
        edge.properties.clone(),
        &RedactionContext::graph_evidence(),
        &DefaultRedactor::new(),
    )?;
    let mut merged_properties = stamp_redaction_metadata(redacted_properties, &redaction_report);
    let (authority, confidence) = match existing {
        Some(existing) => {
            let prior = existing.authority;
            let decision = resolve_authority(prior, edge.authority);
            if decision.winner == prior {
                // Preserve fields asserted by the higher-authority (or existing
                // equal-authority) claim while still accepting non-conflicting
                // metadata discovered by this claim.
                merged_properties.0.extend(existing.metadata.0);
            } else {
                let mut metadata = existing.metadata;
                metadata.0.extend(merged_properties.0);
                merged_properties = metadata;
            }
            let winner = if decision.conflict {
                super::conflict::record_edge_conflict(tx, edge, prior).await?;
                axon_api::source::AuthorityLevel::Conflicting
            } else {
                decision.winner.to_level()
            };
            (
                winner,
                existing.confidence.max(edge.confidence).clamp(0.0, 1.0),
            )
        }
        None => (edge.authority.to_level(), edge.confidence.clamp(0.0, 1.0)),
    };

    Ok(EdgeWrite {
        edge_id: edge.edge_id.0.clone(),
        kind: edge.kind.clone(),
        from_node_id: edge.from_node_id.0.clone(),
        to_node_id: edge.to_node_id.0.clone(),
        authority: authority_to_str(authority).to_string(),
        confidence: confidence as f64,
        metadata_json: metadata_to_json(&merged_properties)?,
        created_at: now.clone(),
        updated_at: now,
    })
}

async fn upsert_edge_batch(
    tx: &mut sqlx::SqliteConnection,
    writes: &[EdgeWrite],
) -> StoreResult<()> {
    for batch in writes.chunks(EDGE_WRITE_BATCH_SIZE) {
        let mut query = QueryBuilder::<Sqlite>::new(
            "INSERT INTO graph_edges (edge_id, kind, from_node_id, to_node_id, authority, confidence, metadata_json, created_at, updated_at) ",
        );
        query.push_values(batch, |mut row, write| {
            row.push_bind(&write.edge_id)
                .push_bind(&write.kind)
                .push_bind(&write.from_node_id)
                .push_bind(&write.to_node_id)
                .push_bind(&write.authority)
                .push_bind(write.confidence)
                .push_bind(&write.metadata_json)
                .push_bind(&write.created_at)
                .push_bind(&write.updated_at);
        });
        query.push(
            " ON CONFLICT(edge_id) DO UPDATE SET authority = excluded.authority, confidence = excluded.confidence, metadata_json = excluded.metadata_json, updated_at = excluded.updated_at",
        );
        query
            .build()
            .execute(&mut *tx)
            .await
            .map_err(|e| graph_storage_error(format!("failed to batch upsert edges: {e}")))?;
    }
    Ok(())
}

async fn fetch_edge_states(
    tx: &mut sqlx::SqliteConnection,
    edges: &[(ResolvedEdge, Vec<&GraphEvidence>)],
) -> StoreResult<HashMap<String, ExistingEdgeState>> {
    if edges.is_empty() {
        return Ok(HashMap::new());
    }
    use sqlx::Row;
    let mut states = HashMap::new();
    for batch in edges.chunks(EDGE_READ_BATCH_SIZE) {
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT edge_id, authority, confidence, metadata_json FROM graph_edges WHERE edge_id IN (",
        );
        let mut separated = query.separated(", ");
        for (edge, _) in batch {
            separated.push_bind(&edge.edge_id.0);
        }
        separated.push_unseparated(")");
        let rows = query
            .build()
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| graph_storage_error(format!("failed to batch read edges: {e}")))?;
        for row in rows {
            let id: String = row.get("edge_id");
            let authority = Authority::from_level(super::row::authority_from_str(
                &row.get::<String, _>("authority"),
            ));
            let confidence = row.get::<f64, _>("confidence") as f32;
            let metadata = metadata_from_json(&row.get::<String, _>("metadata_json"))?;
            states.insert(
                id,
                ExistingEdgeState {
                    authority,
                    confidence,
                    metadata,
                },
            );
        }
    }
    Ok(states)
}

async fn upsert_aliases(
    tx: &mut sqlx::SqliteConnection,
    nodes: &[ResolvedNode],
) -> StoreResult<()> {
    const ALIAS_BATCH_SIZE: usize = 300;
    let mut aliases = Vec::<(String, String, String)>::with_capacity(ALIAS_BATCH_SIZE);
    for node in nodes {
        for (kind, value) in [
            ("stable_key", node.stable_key.as_str()),
            ("canonical_uri", node.canonical_uri.as_str()),
            ("node_id", node.node_id.0.as_str()),
        ] {
            aliases.push((kind.to_string(), value.to_string(), node.node_id.0.clone()));
            if aliases.len() == ALIAS_BATCH_SIZE {
                execute_alias_batch(tx, &aliases).await?;
                aliases.clear();
            }
        }
    }
    if !aliases.is_empty() {
        execute_alias_batch(tx, &aliases).await?;
    }
    Ok(())
}

async fn execute_alias_batch(
    tx: &mut sqlx::SqliteConnection,
    aliases: &[(String, String, String)],
) -> StoreResult<()> {
    let mut query = QueryBuilder::<Sqlite>::new(
        "INSERT INTO graph_aliases (alias_kind, alias_value, node_id) ",
    );
    query.push_values(aliases, |mut row, (kind, value, node_id)| {
        row.push_bind(kind).push_bind(value).push_bind(node_id);
    });
    query.push(" ON CONFLICT(alias_kind, alias_value) DO UPDATE SET node_id = excluded.node_id");
    query
        .build()
        .execute(&mut *tx)
        .await
        .map_err(|e| graph_storage_error(format!("failed to batch upsert aliases: {e}")))?;
    Ok(())
}
