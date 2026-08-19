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
use super::row::{authority_to_str, metadata_to_json, range_to_json, source_ids_to_json};
use crate::authority::{Authority, resolve_authority};
use crate::candidate::validate_candidate;
use crate::error::graph_storage_error;
use crate::evidence::EvidenceKind;
use crate::merge::{
    ResolvedEdge, ResolvedNode, authority_from_evidence, confidence_from_evidence, edge_id_for,
    resolve_node,
};

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

        for node in &resolved_nodes {
            upsert_node(&mut tx, node, &candidate.source_id, candidate.confidence).await?;
            nodes_upserted += 1;
        }
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

/// The source baseline is the largest graph candidate in normal indexing: one
/// node, edge, and evidence record per manifest item. Its incoming authority is
/// always inferred, so the merge can be expressed directly in SQLite and sent
/// in bounded multi-row batches instead of issuing a read + write per entity.
async fn upsert_source_baseline(
    tx: &mut sqlx::SqliteConnection,
    candidate: &GraphCandidate,
) -> StoreResult<(u64, u64, u64)> {
    const NODE_BATCH_SIZE: usize = 80;
    const EDGE_BATCH_SIZE: usize = 100;

    let resolved_nodes = candidate.nodes.iter().map(resolve_node).collect::<Vec<_>>();
    let redactor = DefaultRedactor::new();
    let context = RedactionContext::graph_evidence();
    let source_ids_json = source_ids_to_json(std::slice::from_ref(&candidate.source_id))?;
    let now = now_timestamp();

    for nodes in resolved_nodes.chunks(NODE_BATCH_SIZE) {
        let mut rows = Vec::with_capacity(nodes.len());
        for node in nodes {
            let (properties, report) =
                redact_metadata_checked(node.properties.clone(), &context, &redactor)?;
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
                source_ids_json: source_ids_json.clone(),
                created_at: now.clone(),
                updated_at: now.clone(),
            });
        }
        let mut query = QueryBuilder::<Sqlite>::new(
            "INSERT INTO graph_nodes (node_id, kind, stable_key, canonical_uri, display_name, \
             authority, confidence, metadata_json, source_ids_json, created_at, updated_at) ",
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
            " ON CONFLICT(node_id) DO UPDATE SET \
             canonical_uri = CASE WHEN graph_nodes.authority = 'inferred' \
               THEN excluded.canonical_uri ELSE graph_nodes.canonical_uri END, \
             display_name = CASE WHEN graph_nodes.authority = 'inferred' \
               THEN excluded.display_name ELSE graph_nodes.display_name END, \
             authority = graph_nodes.authority, \
             confidence = MAX(graph_nodes.confidence, excluded.confidence), \
             metadata_json = CASE WHEN graph_nodes.authority = 'inferred' \
               THEN json_patch(graph_nodes.metadata_json, excluded.metadata_json) \
               ELSE json_patch(excluded.metadata_json, graph_nodes.metadata_json) END, \
             source_ids_json = CASE WHEN EXISTS (\
               SELECT 1 FROM json_each(graph_nodes.source_ids_json) \
               WHERE value = json_extract(excluded.source_ids_json, '$[0]')\
             ) THEN graph_nodes.source_ids_json ELSE json_insert(\
               graph_nodes.source_ids_json, '$[#]', json_extract(excluded.source_ids_json, '$[0]')\
             ) END, updated_at = excluded.updated_at",
        );
        query.build().execute(&mut *tx).await.map_err(|e| {
            graph_storage_error(format!("failed to batch upsert baseline nodes: {e}"))
        })?;
    }
    upsert_aliases(tx, &resolved_nodes).await?;

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
                redact_metadata_checked(resolved.properties.clone(), &context, &redactor)?;
            let properties = stamp_redaction_metadata(properties, &report);
            rows.push(BaselineEdgeWrite {
                edge_id: resolved.edge_id.0.clone(),
                kind: resolved.kind.clone(),
                from_node_id: resolved.from_node_id.0.clone(),
                to_node_id: resolved.to_node_id.0.clone(),
                authority: authority_to_str(resolved.authority.to_level()),
                confidence: resolved.confidence.clamp(0.0, 1.0) as f64,
                metadata_json: metadata_to_json(&properties)?,
                created_at: now.clone(),
                updated_at: now.clone(),
            });
            edges_written = edges_written.saturating_add(1);
            for evidence in edge_evidence {
                pending_evidence.push((resolved.edge_id.0.clone(), evidence));
                evidence_written = evidence_written.saturating_add(1);
            }
        }
        if !rows.is_empty() {
            let mut query = QueryBuilder::<Sqlite>::new(
                "INSERT INTO graph_edges (edge_id, kind, from_node_id, to_node_id, authority, \
                 confidence, metadata_json, created_at, updated_at) ",
            );
            query.push_values(&rows, |mut row, value| {
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
                " ON CONFLICT(edge_id) DO UPDATE SET \
                 authority = graph_edges.authority, \
                 confidence = MAX(graph_edges.confidence, excluded.confidence), \
                 metadata_json = CASE WHEN graph_edges.authority = 'inferred' \
                   THEN json_patch(graph_edges.metadata_json, excluded.metadata_json) \
                   ELSE json_patch(excluded.metadata_json, graph_edges.metadata_json) END, \
                 updated_at = excluded.updated_at",
            );
            query.build().execute(&mut *tx).await.map_err(|e| {
                graph_storage_error(format!("failed to batch upsert baseline edges: {e}"))
            })?;
        }
        upsert_evidence_batch(tx, &pending_evidence).await?;
    }

    Ok((resolved_nodes.len() as u64, edges_written, evidence_written))
}

/// Upsert one node by (kind, stable_key), merging authority under the
/// keep-highest-authority policy and unioning source ids.
async fn upsert_node(
    tx: &mut sqlx::SqliteConnection,
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
    .fetch_optional(&mut *tx)
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
    .execute(&mut *tx)
    .await
    .map_err(|e| graph_storage_error(format!("failed to upsert node: {e}")))?;

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
    .execute(&mut *tx)
    .await
    .map_err(|e| graph_storage_error(format!("failed to upsert edge: {e}")))?;

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
