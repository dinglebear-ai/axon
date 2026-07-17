//! Candidate write path for the SQLite graph store.

use axon_api::source::{GraphCandidate, GraphWriteResult, SourceId};
use axon_core::redact::{
    DefaultRedactor, RedactionContext, Redactor, redact_metadata_checked, stamp_redaction_metadata,
};
use sqlx::SqlitePool;

use super::header::{now_timestamp, stage_header};
use super::row::{authority_to_str, metadata_to_json, range_to_json, source_ids_to_json};
use crate::authority::{Authority, resolve_authority};
use crate::candidate::validate_candidate;
use crate::error::graph_storage_error;
use crate::merge::{ResolvedNode, resolve_edge, resolve_node};

type StoreResult<T> = Result<T, axon_api::source::ApiError>;

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
    for candidate in &candidates {
        validate_candidate(candidate)?;
    }

    let candidates_seen = candidates.len() as u64;
    let source_id = candidates
        .first()
        .map(|c| c.source_id.clone())
        .unwrap_or_else(|| SourceId::new("graph"));

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| graph_storage_error(format!("failed to open graph transaction: {e}")))?;

    let mut nodes_upserted = 0u64;
    let mut edges_upserted = 0u64;
    let mut evidence_records = 0u64;

    for candidate in candidates {
        let resolved_nodes: Vec<ResolvedNode> = candidate.nodes.iter().map(resolve_node).collect();

        for node in &resolved_nodes {
            upsert_node(&mut tx, node, &candidate.source_id, candidate.confidence).await?;
            nodes_upserted += 1;
        }

        for edge in &candidate.edges {
            let edge_evidence = candidate
                .evidence
                .iter()
                .filter(|evidence| edge.evidence_ids.contains(&evidence.evidence_id))
                .cloned()
                .collect::<Vec<_>>();
            let Some(resolved) =
                resolve_edge(edge, &resolved_nodes, &edge_evidence, candidate.confidence)
            else {
                continue;
            };
            upsert_edge(&mut tx, &resolved).await?;
            edges_upserted += 1;
            for ev in &edge_evidence {
                upsert_evidence(&mut tx, &resolved.edge_id.0, ev).await?;
                evidence_records += 1;
            }
        }
    }

    tx.commit()
        .await
        .map_err(|e| graph_storage_error(format!("failed to commit graph transaction: {e}")))?;

    Ok(GraphWriteResult {
        header: stage_header(),
        source_id,
        candidates_seen,
        nodes_upserted,
        edges_upserted,
        evidence_records,
        warnings: Vec::new(),
    })
}

/// Upsert one node by (kind, stable_key), merging authority under the
/// keep-highest-authority policy and unioning source ids.
async fn upsert_node(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    node: &ResolvedNode,
    source_id: &SourceId,
    fallback_confidence: f32,
) -> StoreResult<()> {
    let now = now_timestamp();
    // Fail-closed redaction boundary: node properties are adapter-supplied
    // evidence metadata surfaced back through graph queries — scrub before
    // the write, not after.
    let (redacted_properties, redaction_report) = redact_metadata_checked(
        node.properties.clone(),
        &RedactionContext::graph_evidence(),
        &DefaultRedactor::new(),
    )?;
    let redacted_properties = stamp_redaction_metadata(redacted_properties, &redaction_report);
    // Read the existing node (if any) to merge authority + source ids.
    let existing = sqlx::query(
        "SELECT authority, source_ids_json, confidence FROM graph_nodes WHERE node_id = ?",
    )
    .bind(&node.node_id.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| graph_storage_error(format!("failed to read node for upsert: {e}")))?;

    let (authority, confidence, source_ids_json) = match existing {
        Some(row) => {
            use sqlx::Row;
            let prior = Authority::from_level(super::row::authority_from_str(
                &row.get::<String, _>("authority"),
            ));
            let winner = resolve_authority(prior, node.authority).winner;
            let prior_conf = row.get::<f64, _>("confidence") as f32;
            let conf = prior_conf.max(fallback_confidence).clamp(0.0, 1.0);
            let mut ids =
                super::row::source_ids_from_json(&row.get::<String, _>("source_ids_json"))?;
            if !ids.contains(source_id) {
                ids.push(source_id.clone());
            }
            (winner, conf, source_ids_to_json(&ids)?)
        }
        None => (
            node.authority,
            fallback_confidence.clamp(0.0, 1.0),
            source_ids_to_json(std::slice::from_ref(source_id))?,
        ),
    };

    sqlx::query(
        "INSERT INTO graph_nodes (
            node_id, kind, stable_key, canonical_uri, display_name, authority,
            confidence, metadata_json, source_ids_json, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(node_id) DO UPDATE SET
            canonical_uri = excluded.canonical_uri,
            display_name  = excluded.display_name,
            authority     = excluded.authority,
            confidence    = excluded.confidence,
            metadata_json = excluded.metadata_json,
            source_ids_json = excluded.source_ids_json,
            updated_at    = excluded.updated_at",
    )
    .bind(&node.node_id.0)
    .bind(&node.kind)
    .bind(&node.stable_key)
    .bind(&node.canonical_uri)
    .bind(&node.label)
    .bind(authority_to_str(authority.to_level()))
    .bind(confidence as f64)
    .bind(metadata_to_json(&redacted_properties)?)
    .bind(source_ids_json)
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await
    .map_err(|e| graph_storage_error(format!("failed to upsert node: {e}")))?;

    // Populate alias entries so resolve() can find this node by stable key,
    // canonical uri, or node id.
    for (alias_kind, alias_value) in [
        ("stable_key", node.stable_key.as_str()),
        ("canonical_uri", node.canonical_uri.as_str()),
        ("node_id", node.node_id.0.as_str()),
    ] {
        sqlx::query(
            "INSERT INTO graph_aliases (alias_kind, alias_value, node_id)
             VALUES (?, ?, ?)
             ON CONFLICT(alias_kind, alias_value) DO UPDATE SET node_id = excluded.node_id",
        )
        .bind(alias_kind)
        .bind(alias_value)
        .bind(&node.node_id.0)
        .execute(&mut **tx)
        .await
        .map_err(|e| graph_storage_error(format!("failed to upsert alias: {e}")))?;
    }

    Ok(())
}

/// Upsert one edge by (kind, from, to). On conflict the authority is resolved
/// under keep-highest-authority; equal authoritative claims record a conflict.
async fn upsert_edge(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    edge: &crate::merge::ResolvedEdge,
) -> StoreResult<()> {
    let now = now_timestamp();
    let (redacted_properties, redaction_report) = redact_metadata_checked(
        edge.properties.clone(),
        &RedactionContext::graph_evidence(),
        &DefaultRedactor::new(),
    )?;
    let redacted_properties = stamp_redaction_metadata(redacted_properties, &redaction_report);
    let existing = sqlx::query("SELECT authority, confidence FROM graph_edges WHERE edge_id = ?")
        .bind(&edge.edge_id.0)
        .fetch_optional(&mut **tx)
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
                // Preserve the existing authoritative claim; mark the edge as
                // conflicting so downstream never silently trusts one side.
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
    .execute(&mut **tx)
    .await
    .map_err(|e| graph_storage_error(format!("failed to upsert edge: {e}")))?;

    Ok(())
}

/// Upsert one evidence record for an edge (idempotent by (edge_id, evidence_id)).
async fn upsert_evidence(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    edge_id: &str,
    ev: &axon_api::source::GraphEvidence,
) -> StoreResult<()> {
    let redactor = DefaultRedactor::new();
    let context = RedactionContext::graph_evidence();
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
        evidence_metadata.insert("document_id".to_string(), serde_json::json!(document_id.0));
    }
    if let Some(chunk_id) = &ev.chunk_id {
        evidence_metadata.insert("chunk_id".to_string(), serde_json::json!(chunk_id.0));
    }
    let (redacted_metadata, redaction_report) =
        redact_metadata_checked(evidence_metadata, &context, &redactor)?;
    let redacted_metadata = stamp_redaction_metadata(redacted_metadata, &redaction_report);
    sqlx::query(
        "INSERT INTO graph_evidence (
            evidence_id, edge_id, evidence_kind, source_id, source_item_key,
            document_id, chunk_id, range_json, quote, confidence, metadata_json
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(edge_id, evidence_id) DO UPDATE SET
            evidence_kind = excluded.evidence_kind,
            confidence    = excluded.confidence,
            metadata_json = excluded.metadata_json",
    )
    .bind(&ev.evidence_id)
    .bind(edge_id)
    .bind(&ev.evidence_kind)
    .bind(&ev.source_id.0)
    .bind(&ev.source_item_key.0)
    .bind(ev.document_id.as_ref().map(|d| d.0.clone()))
    .bind(ev.chunk_id.as_ref().map(|c| c.0.clone()))
    .bind(range_to_json(&ev.range)?)
    .bind(&redacted_quote)
    .bind(ev.confidence as f64)
    .bind(metadata_to_json(&redacted_metadata)?)
    .execute(&mut **tx)
    .await
    .map_err(|e| graph_storage_error(format!("failed to upsert evidence: {e}")))?;
    Ok(())
}
