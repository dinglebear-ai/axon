//! SourceGraph write for `index_source` — the `graphing` stage.
//!
//! After a source is acquired and indexed, this module upserts two kinds of
//! [`GraphCandidate`] into the durable [`SqliteGraphStore`]:
//!
//! 1. the **baseline skeleton**: one container node for the source itself
//!    (kind chosen per family from the closed registry, keyed by the source's
//!    canonical URI), one document node per indexed manifest item (kind
//!    derived from the item's [`ItemKind`]), and one containment edge
//!    (container → document) per item, each backed by a `text_mention`
//!    evidence record so the candidate validates;
//! 2. the **real parser-produced candidates** carried up from every prepared
//!    document in this generation (`source-pipeline.md`'s `parsing` stage
//!    output — repo→package edges, compose topology, session tool calls, …),
//!    collected during vectorization and forwarded here via
//!    [`IndexCounts::graph_candidates`] instead of being dropped after
//!    preparation.
//!
//! Every candidate — baseline or parser-produced — is individually
//! re-validated against `axon-graph`'s closed kind registry
//! ([`axon_graph::candidate::validate_candidate`]) before the batch write:
//! `SqliteGraphStore::upsert_candidates` fails the *whole* transaction on the
//! first invalid candidate, so a single malformed candidate from a parser must
//! not be allowed to also block a source's valid baseline skeleton from
//! landing. Invalid candidates are dropped with a warning (fail-closed at the
//! candidate level), not published.
//!
//! Per the crate-ownership rule, `axon-graph` owns the store, the closed kind
//! registry, and candidate/merge-key/authority validation; this module only
//! assembles and filters [`GraphCandidate`] values and calls
//! `upsert_candidates` once per index. When no target pool is available (no
//! unified SQLite runtime), the write is skipped and a degraded
//! [`GraphWriteSummary`] with zero counts is returned — acquisition never
//! crashes because of the graph write.

use std::sync::Arc;

use axon_api::source::{
    EnrichmentKind, EnrichmentStatus, GraphCandidate, GraphCandidateProducer, GraphEdgeCandidate,
    GraphEvidence, GraphNodeCandidate, GraphWriteSummary, ItemKind, ManifestItem, MetadataMap,
    ParserHint, PipelinePhase, SourceEnrichment, SourceId, SourceItemKey, SourceManifest,
    StageCounts, StageId, StageResultHeader, Timestamp,
};
use axon_graph::candidate::validate_candidate;
use axon_graph::sqlite::SqliteGraphStore;
use axon_graph::store::GraphStore;
use axon_ledger::store::LedgerStore;
use sqlx::SqlitePool;

use super::classify::SourceInputKind;
use super::result_map::IndexCounts;

/// Confidence stamped on baseline skeleton nodes/edges. These are structural
/// containment facts derived directly from the acquired manifest, not inferred
/// text mentions, so they carry high confidence.
const BASELINE_CONFIDENCE: f32 = 0.95;

/// Producer version reported on every baseline candidate.
const PRODUCER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build and persist the source graph for a completed index: the baseline
/// skeleton plus every parser-produced candidate from this generation.
///
/// Reads the just-published manifest for `counts.source_id`/`counts.generation`
/// from the ledger, assembles one container node + one node/edge per document,
/// unions in `extra_candidates` (already individually validated here so one bad
/// candidate cannot sink the baseline write), runs the minimal `enriching`
/// stage over the valid extras, and upserts everything into the durable graph
/// in one batch. Returns the real [`GraphWriteSummary`] from the store result.
///
/// A missing pool, a missing manifest, or a store error degrades to a zero-count
/// summary (with `degraded = true`) rather than failing the index — the source
/// is already acquired and published by the time this runs.
pub async fn write_baseline_graph(
    kind: SourceInputKind,
    pool: Option<Arc<SqlitePool>>,
    ledger: &dyn LedgerStore,
    counts: &IndexCounts,
    canonical_uri: &str,
    extra_candidates: Vec<GraphCandidate>,
) -> GraphWriteSummary {
    let Some(pool) = pool else {
        tracing::debug!("no unified sqlite pool; skipping baseline graph write");
        return degraded_summary();
    };

    let manifest = match ledger
        .get_manifest(counts.source_id.clone(), counts.generation.clone())
        .await
    {
        Ok(Some(manifest)) => manifest,
        Ok(None) => {
            tracing::debug!(
                source_id = %counts.source_id.0,
                generation = %counts.generation.0,
                "no manifest for indexed generation; skipping baseline graph write"
            );
            return degraded_summary();
        }
        Err(err) => {
            tracing::warn!(
                error = %err.message,
                source_id = %counts.source_id.0,
                "failed to read manifest for baseline graph; skipping"
            );
            return degraded_summary();
        }
    };

    // Enriching stage (source-pipeline.md: "fetched/acquired items + source
    // metadata" -> `SourceEnrichment[]`). This is a minimal but real
    // producer: it derives an enrichment record from the parser-produced
    // candidates actually extracted for this generation (not a stub), and
    // must not itself persist graph data — only the store write below does.
    let valid_extras = filter_valid_candidates(extra_candidates, &counts.source_id);
    let enrichment = build_enrichment(counts, canonical_uri, &valid_extras);
    tracing::info!(
        source_id = %counts.source_id.0,
        enrichment_kind = ?enrichment.enrichment_kind,
        enrichment_status = ?enrichment.status,
        parse_hints = enrichment.parse_hints.len(),
        graph_candidates = enrichment.graph_candidates.len(),
        "enriching stage produced source enrichment record"
    );

    let mut candidates = vec![build_candidate(kind, counts, canonical_uri, &manifest)];
    candidates.extend(valid_extras);

    let store = SqliteGraphStore::from_pool((*pool).clone());
    match store.upsert_candidates(candidates).await {
        Ok(result) => GraphWriteSummary {
            nodes_upserted: result.nodes_upserted,
            edges_upserted: result.edges_upserted,
            evidence_records: result.evidence_records,
            degraded: false,
        },
        Err(err) => {
            tracing::warn!(
                error = %err.message,
                source_id = %counts.source_id.0,
                "baseline graph upsert failed; returning degraded summary"
            );
            degraded_summary()
        }
    }
}

/// Re-validate every extra (parser-produced) candidate against `axon-graph`'s
/// closed kind registry before it enters the write batch. `upsert_candidates`
/// fails the whole batch on the first invalid candidate, so filtering here —
/// fail-closed at the *candidate* level, not the whole index — keeps one
/// malformed parser candidate from also blocking the baseline skeleton write.
/// axon-parse already sanitizes at parse time (`validate::sanitize_result`);
/// this is the write path's own gate, so it never trusts an upstream caller to
/// have done so.
fn filter_valid_candidates(
    candidates: Vec<GraphCandidate>,
    source_id: &SourceId,
) -> Vec<GraphCandidate> {
    candidates
        .into_iter()
        .filter(|candidate| match validate_candidate(candidate) {
            Ok(()) => true,
            Err(err) => {
                tracing::warn!(
                    source_id = %source_id.0,
                    candidate_id = %candidate.candidate_id,
                    error = %err.message,
                    "dropping invalid graph candidate before write"
                );
                false
            }
        })
        .collect()
}

/// Build the minimal `enriching`-stage output for this generation.
/// `EnrichmentKind::Extraction`/`Completed` when the generation produced at
/// least one validated graph candidate; `EnrichmentKind::None`/`NotNeeded`
/// when it produced none. Parse hints are derived from the real parser ids
/// recorded on each candidate's producer, not fabricated.
fn build_enrichment(
    counts: &IndexCounts,
    canonical_uri: &str,
    candidates: &[GraphCandidate],
) -> SourceEnrichment {
    let now = Timestamp(chrono::Utc::now().to_rfc3339());
    let mut parser_ids: Vec<String> = candidates
        .iter()
        .filter_map(|candidate| candidate.producer.parser.clone())
        .collect();
    parser_ids.sort();
    parser_ids.dedup();
    let parse_hints = parser_ids
        .into_iter()
        .map(|parser_id| ParserHint {
            parser_id,
            reason: "graph candidate producer observed during parsing".to_string(),
            options: MetadataMap::new(),
        })
        .collect();

    let (enrichment_kind, status) = if candidates.is_empty() {
        (EnrichmentKind::None, EnrichmentStatus::NotNeeded)
    } else {
        (EnrichmentKind::Extraction, EnrichmentStatus::Completed)
    };

    SourceEnrichment {
        header: StageResultHeader {
            job_id: counts.job_id,
            stage_id: StageId::new(counts.job_id.0),
            phase: PipelinePhase::Preparing,
            status: axon_api::source::LifecycleStatus::Completed,
            started_at: now.clone(),
            completed_at: Some(now),
            counts: StageCounts {
                items_total: Some(candidates.len() as u64),
                items_done: candidates.len() as u64,
                documents_total: None,
                documents_done: 0,
                chunks_total: None,
                chunks_done: 0,
                bytes_total: None,
                bytes_done: 0,
            },
            warnings: Vec::new(),
            error: None,
        },
        source_id: counts.source_id.clone(),
        source_item_key: SourceItemKey::new(canonical_uri),
        enrichment_kind,
        status,
        metadata: MetadataMap::new(),
        parse_hints,
        chunk_hints: Vec::new(),
        graph_candidates: candidates.to_vec(),
        artifacts: Vec::new(),
        warnings: Vec::new(),
    }
}

/// A degraded, no-write summary. Used whenever the graph write is skipped or
/// fails so the source result still reports a truthful `degraded` flag.
fn degraded_summary() -> GraphWriteSummary {
    GraphWriteSummary {
        nodes_upserted: 0,
        edges_upserted: 0,
        evidence_records: 0,
        degraded: true,
    }
}

/// Assemble the baseline container + document skeleton for one source.
fn build_candidate(
    kind: SourceInputKind,
    counts: &IndexCounts,
    canonical_uri: &str,
    manifest: &SourceManifest,
) -> GraphCandidate {
    let source_id = counts.source_id.clone();
    let source_item_key = SourceItemKey::new(canonical_uri);
    let container_key = container_stable_key(&source_id, canonical_uri);
    let container = GraphNodeCandidate {
        node_kind: container_node_kind(kind).to_string(),
        stable_key: container_key.clone(),
        label: canonical_uri.to_string(),
        properties: MetadataMap::new(),
    };

    let mut nodes = vec![container];
    let mut edges = Vec::new();
    let mut evidence = Vec::new();
    let edge_kind = containment_edge_kind(kind);

    for item in &manifest.items {
        let doc_key = document_stable_key(item);
        let item_evidence = containment_evidence(&source_id, &source_item_key, item);
        nodes.push(GraphNodeCandidate {
            node_kind: document_node_kind(item).to_string(),
            stable_key: doc_key.clone(),
            label: item.canonical_uri.clone(),
            properties: MetadataMap::new(),
        });
        edges.push(GraphEdgeCandidate {
            edge_kind: edge_kind.to_string(),
            from_stable_key: container_key.clone(),
            to_stable_key: doc_key,
            evidence_ids: vec![item_evidence.evidence_id.clone()],
            properties: MetadataMap::new(),
        });
        evidence.push(item_evidence);
    }

    GraphCandidate {
        candidate_id: format!("source-baseline:{}:{}", source_id.0, counts.generation.0),
        job_id: counts.job_id.clone(),
        source_id: source_id.clone(),
        source_item_key,
        item_canonical_uri: canonical_uri.to_string(),
        document_id: None,
        kind: "source_baseline".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: super::adapter_name_for(kind).to_string(),
            parser: None,
            version: PRODUCER_VERSION.to_string(),
        },
        nodes,
        edges,
        evidence,
        confidence: BASELINE_CONFIDENCE,
        metadata: MetadataMap::new(),
    }
}

/// One `text_mention` evidence record per containment edge. The manifest is the
/// direct observation that the item belongs to this source, so it justifies the
/// containment claim (edges are never "just true").
fn containment_evidence(
    source_id: &SourceId,
    candidate_source_item_key: &SourceItemKey,
    item: &ManifestItem,
) -> GraphEvidence {
    let mut metadata = MetadataMap::new();
    metadata.insert(
        "contained_source_item_key".to_string(),
        serde_json::json!(item.source_item_key.0),
    );
    GraphEvidence {
        evidence_id: format!("contains:{}", item.source_item_key.0),
        evidence_kind: "text_mention".to_string(),
        source_id: source_id.clone(),
        source_item_key: candidate_source_item_key.clone(),
        document_id: None,
        chunk_id: None,
        range: None,
        quote: None,
        confidence: BASELINE_CONFIDENCE,
        metadata,
    }
}

/// Stable key for the container node — the source's canonical URI, namespaced by
/// source id so distinct sources never collide on a shared URI shape.
fn container_stable_key(source_id: &SourceId, canonical_uri: &str) -> String {
    format!("source:{}:{}", source_id.0, canonical_uri)
}

/// Stable key for a document node — the item's own stable source key.
fn document_stable_key(item: &ManifestItem) -> String {
    if item.item_kind == ItemKind::MemoryRecord {
        return format!("memory:{}", item.source_item_key.0);
    }
    item.source_item_key.0.clone()
}

/// Registry node kind for the source container, chosen per acquisition family.
/// Every returned name is a closed [`axon_graph::node::GraphNodeKind`] variant.
fn container_node_kind(kind: SourceInputKind) -> &'static str {
    match kind {
        SourceInputKind::Web => "web_origin",
        SourceInputKind::Git => "repo",
        SourceInputKind::Local => "local_checkout",
        SourceInputKind::Feed => "feed",
        SourceInputKind::Reddit => "reddit_subreddit",
        SourceInputKind::Youtube => "youtube_channel",
        SourceInputKind::Session => "session",
        SourceInputKind::Registry => "package",
        SourceInputKind::CliTool | SourceInputKind::McpTool => "artifact",
        SourceInputKind::Memory => "source",
        SourceInputKind::Upload => "derived_source",
        SourceInputKind::Unsupported => "source",
    }
}

/// Registry document-node kind, derived from the manifest item's [`ItemKind`].
fn document_node_kind(item: &ManifestItem) -> &'static str {
    match item.item_kind {
        ItemKind::WebPage => "web_page",
        ItemKind::RepoFile => "repo_file",
        ItemKind::LocalFile => "repo_file",
        ItemKind::PackageVersion => "package_version",
        ItemKind::FeedEntry => "feed_entry",
        ItemKind::Transcript => "youtube_video",
        ItemKind::SessionTurn => "session_turn",
        ItemKind::ToolCall => "tool_call",
        ItemKind::CliOutput => "artifact",
        ItemKind::McpToolOutput => "artifact",
        ItemKind::MemoryRecord => "memory",
        ItemKind::Artifact => "artifact",
    }
}

/// Registry containment edge kind (container → document) per family. Every
/// returned name is a closed [`axon_graph::edge::GraphEdgeKind`] variant.
fn containment_edge_kind(kind: SourceInputKind) -> &'static str {
    match kind {
        SourceInputKind::Web => "docs_site_contains_page",
        SourceInputKind::Git => "commit_contains_file",
        SourceInputKind::Local => "commit_contains_file",
        SourceInputKind::Feed => "feed_contains_entry",
        SourceInputKind::Reddit => "subreddit_has_thread",
        SourceInputKind::Youtube => "youtube_channel_has_video",
        SourceInputKind::Session => "session_has_turn",
        SourceInputKind::Registry => "package_has_version",
        SourceInputKind::CliTool | SourceInputKind::McpTool => "source_produced_artifact",
        SourceInputKind::Memory | SourceInputKind::Upload => "source_indexed_as",
        SourceInputKind::Unsupported => "source_produced_artifact",
    }
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
