//! Candidate write path for the SQLite graph store.

use std::collections::HashMap;
use std::str::FromStr;

use axon_api::source::{
    GraphCandidate, GraphEdgeCandidate, GraphEvidence, GraphWriteResult, SourceId,
};
use axon_core::redact::{
    DefaultRedactor, RedactionContext, Redactor, redact_metadata_checked, stamp_redaction_metadata,
};
use axon_core::sqlite::ImmediateTx;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use super::header::{now_timestamp, stage_header};
use super::row::{authority_to_str, metadata_to_json, range_to_json};
use crate::authority::{Authority, resolve_authority};
use crate::candidate::validate_candidate;
use crate::error::graph_storage_error;
use crate::evidence::EvidenceKind;
use crate::merge::{
    ResolvedEdge, ResolvedNode, authority_from_evidence, confidence_from_evidence, edge_id_for,
    resolve_node,
};

type StoreResult<T> = Result<T, axon_api::source::ApiError>;

mod baseline;
use baseline::upsert_source_baseline;
mod nodes;

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
    let mut tx = ImmediateTx::begin(pool)
        .await
        .map_err(|e| graph_storage_error(format!("failed to open graph transaction: {e}")))?;

    let mut candidates_seen = 0u64;
    let mut source_id = None;
    let mut nodes_upserted = 0u64;
    let mut edges_upserted = 0u64;
    let mut evidence_records = 0u64;

    for candidate in candidates {
        validate_candidate(&candidate)?;
        candidates_seen = candidates_seen.saturating_add(1);
        if source_id.is_none() {
            source_id = Some(candidate.source_id.clone());
        }
        if is_inferred_source_baseline(&candidate) {
            let (nodes, edges, evidence) = upsert_source_baseline(&mut tx, &candidate).await?;
            nodes_upserted = nodes_upserted.saturating_add(nodes);
            edges_upserted = edges_upserted.saturating_add(edges);
            evidence_records = evidence_records.saturating_add(evidence);
            continue;
        }

        let resolved_nodes: Vec<ResolvedNode> = candidate.nodes.iter().map(resolve_node).collect();
        let nodes_by_stable_key = resolved_nodes
            .iter()
            .map(|node| (node.stable_key.as_str(), node))
            .collect::<HashMap<_, _>>();
        let evidence_by_id = candidate
            .evidence
            .iter()
            .map(|evidence| (evidence.evidence_id.as_str(), evidence))
            .collect::<HashMap<_, _>>();

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
        for edge in &candidate.edges {
            let edge_evidence = edge
                .evidence_ids
                .iter()
                .filter_map(|evidence_id| evidence_by_id.get(evidence_id.as_str()).copied())
                .collect::<Vec<_>>();
            let Some(resolved) = resolve_edge_indexed(
                edge,
                &nodes_by_stable_key,
                &edge_evidence,
                candidate.confidence,
            ) else {
                continue;
            };
            upsert_edge(&mut tx, &resolved).await?;
            edges_upserted += 1;
            for ev in edge_evidence {
                pending_evidence.push((resolved.edge_id.0.clone(), ev));
                evidence_records += 1;
            }
        }
        upsert_evidence_batch(&mut tx, &pending_evidence).await?;
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

fn is_inferred_source_baseline(candidate: &GraphCandidate) -> bool {
    if candidate.kind != "source_baseline" {
        return false;
    }
    let evidence_by_id = candidate
        .evidence
        .iter()
        .map(|evidence| (evidence.evidence_id.as_str(), evidence))
        .collect::<HashMap<_, _>>();
    candidate.edges.iter().all(|edge| {
        !edge.evidence_ids.is_empty()
            && edge.evidence_ids.iter().all(|id| {
                evidence_by_id
                    .get(id.as_str())
                    .and_then(|evidence| EvidenceKind::from_str(&evidence.evidence_kind).ok())
                    .is_some_and(|kind| kind.authority() == Authority::Inferred)
            })
    })
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
async fn upsert_edge(tx: &mut sqlx::SqliteConnection, edge: &ResolvedEdge) -> StoreResult<()> {
    let now = now_timestamp();
    let (redacted_properties, redaction_report) = redact_metadata_checked(
        edge.properties.clone(),
        &RedactionContext::graph_evidence(),
        &DefaultRedactor::new(),
    )?;
    let redacted_properties = stamp_redaction_metadata(redacted_properties, &redaction_report);
    let existing = sqlx::query("SELECT authority, confidence FROM graph_edges WHERE edge_id = ?")
        .bind(&edge.edge_id.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| graph_storage_error(format!("failed to read edge for upsert: {e}")))?;

    let (authority, confidence) = match existing {
        Some(row) => {
            use sqlx::Row;
            let prior = Authority::from_level(super::row::authority_from_str(
                &row.get::<String, _>("authority"),
            ));
            let decision = resolve_authority(prior, edge.authority);
            let prior_conf = row.get::<f64, _>("confidence") as f32;
            let winner = if decision.conflict {
                super::conflict::record_edge_conflict(tx, edge, prior).await?;
                axon_api::source::AuthorityLevel::Conflicting
            } else {
                decision.winner.to_level()
            };
            (winner, prior_conf.max(edge.confidence).clamp(0.0, 1.0))
        }
        None => (edge.authority.to_level(), edge.confidence.clamp(0.0, 1.0)),
    };

    sqlx::query(
        "INSERT INTO graph_edges (
            edge_id, kind, from_node_id, to_node_id, authority, confidence,
            metadata_json, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(edge_id) DO UPDATE SET
            authority     = excluded.authority,
            confidence    = excluded.confidence,
            metadata_json = excluded.metadata_json,
            updated_at    = excluded.updated_at",
    )
    .bind(&edge.edge_id.0)
    .bind(&edge.kind)
    .bind(&edge.from_node_id.0)
    .bind(&edge.to_node_id.0)
    .bind(authority_to_str(authority))
    .bind(confidence as f64)
    .bind(metadata_to_json(&redacted_properties)?)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|e| graph_storage_error(format!("failed to upsert edge: {e}")))?;

    Ok(())
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

struct EvidenceWrite {
    evidence_id: String,
    edge_id: String,
    evidence_kind: String,
    source_id: String,
    source_item_key: String,
    document_id: Option<String>,
    chunk_id: Option<String>,
    range_json: Option<String>,
    quote: Option<String>,
    confidence: f64,
    metadata_json: String,
}

/// Upsert evidence in bounded multi-row statements. Eleven binds per row keep
/// batches of 80 under SQLite's conservative 999-variable ceiling.
async fn upsert_evidence_batch(
    tx: &mut sqlx::SqliteConnection,
    entries: &[(String, &GraphEvidence)],
) -> StoreResult<()> {
    const EVIDENCE_BATCH_SIZE: usize = 80;
    let redactor = DefaultRedactor::new();
    let context = RedactionContext::graph_evidence();
    for entries in entries.chunks(EVIDENCE_BATCH_SIZE) {
        let mut rows = Vec::with_capacity(entries.len());
        for (edge_id, ev) in entries {
            let redacted_quote = ev
                .quote
                .as_ref()
                .map(|quote| redactor.redact_text(quote, &context));
            let mut evidence_metadata = ev.metadata.clone();
            evidence_metadata.insert("source_id".to_string(), serde_json::json!(ev.source_id.0));
            evidence_metadata.insert(
                "source_item_key".to_string(),
                serde_json::json!(ev.source_item_key.0),
            );
            if let Some(document_id) = &ev.document_id {
                evidence_metadata
                    .insert("document_id".to_string(), serde_json::json!(document_id.0));
            }
            if let Some(chunk_id) = &ev.chunk_id {
                evidence_metadata.insert("chunk_id".to_string(), serde_json::json!(chunk_id.0));
            }
            let (redacted_metadata, redaction_report) =
                redact_metadata_checked(evidence_metadata, &context, &redactor)?;
            let redacted_metadata = stamp_redaction_metadata(redacted_metadata, &redaction_report);
            rows.push(EvidenceWrite {
                evidence_id: ev.evidence_id.clone(),
                edge_id: edge_id.clone(),
                evidence_kind: ev.evidence_kind.clone(),
                source_id: ev.source_id.0.clone(),
                source_item_key: ev.source_item_key.0.clone(),
                document_id: ev.document_id.as_ref().map(|d| d.0.clone()),
                chunk_id: ev.chunk_id.as_ref().map(|c| c.0.clone()),
                range_json: range_to_json(&ev.range)?,
                quote: redacted_quote,
                confidence: ev.confidence as f64,
                metadata_json: metadata_to_json(&redacted_metadata)?,
            });
        }

        let mut query = QueryBuilder::<Sqlite>::new(
            "INSERT INTO graph_evidence (evidence_id, edge_id, evidence_kind, source_id, \
             source_item_key, document_id, chunk_id, range_json, quote, confidence, metadata_json) ",
        );
        query.push_values(&rows, |mut row, value| {
            row.push_bind(&value.evidence_id)
                .push_bind(&value.edge_id)
                .push_bind(&value.evidence_kind)
                .push_bind(&value.source_id)
                .push_bind(&value.source_item_key)
                .push_bind(&value.document_id)
                .push_bind(&value.chunk_id)
                .push_bind(&value.range_json)
                .push_bind(&value.quote)
                .push_bind(value.confidence)
                .push_bind(&value.metadata_json);
        });
        query.push(
            " ON CONFLICT(edge_id, evidence_id) DO UPDATE SET \
             evidence_kind = excluded.evidence_kind, \
             confidence = excluded.confidence, metadata_json = excluded.metadata_json",
        );
        query
            .build()
            .execute(&mut *tx)
            .await
            .map_err(|e| graph_storage_error(format!("failed to batch upsert evidence: {e}")))?;
    }
    Ok(())
}
