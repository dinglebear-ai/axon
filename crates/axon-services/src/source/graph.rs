//! SourceGraph write for `index_source` — the `graphing` stage.
//!
//! After a source is acquired and indexed, this module upserts two kinds of
//! [`GraphCandidate`] into the durable `axon-graph` SQLite store:
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
//! The graph store fails the *whole* transaction on the first invalid
//! candidate, so a single malformed candidate from a parser must
//! not be allowed to also block a source's valid baseline skeleton from
//! landing. Invalid candidates are dropped with a warning (fail-closed at the
//! candidate level), not published.
//!
//! Per the crate-ownership rule, `axon-graph` owns the store, the closed kind
//! registry, and candidate/merge-key/authority validation; this module only
//! assembles and filters [`GraphCandidate`] values and sends one candidate
//! batch through the scheduler-owned reserved-call facade per index. When no target pool is available (no
//! unified SQLite runtime), the write is skipped and a degraded
//! [`GraphWriteSummary`] with zero counts is returned — acquisition never
//! crashes because of the graph write.

use std::collections::HashSet;
use std::sync::Arc;

use axon_api::source::{
    EnrichmentKind, EnrichmentStatus, GraphCandidate, GraphCandidateProducer, GraphEdgeCandidate,
    GraphEvidence, GraphNodeCandidate, GraphWriteSummary, ItemKind, ManifestItem, MetadataMap,
    SourceId, SourceItemKey, SourceKind, SourceManifest, SourceScope,
};
use axon_graph::candidate::validate_candidate;
use axon_ledger::store::LedgerStore;
use sqlx::SqlitePool;
use tokio::sync::Semaphore;

use super::result_map::IndexCounts;
use crate::context::TargetLocalSourceRuntime;
use crate::reserved_call::{self, ProviderCallContext};

/// Confidence stamped on baseline skeleton nodes/edges. These are structural
/// containment facts derived directly from the acquired manifest, not inferred
/// text mentions, so they carry high confidence.
const BASELINE_CONFIDENCE: f32 = 0.95;
const BASELINE_GRAPH_BATCH_SIZE: usize = 512;

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
#[cfg(test)]
pub async fn write_baseline_graph(
    kind: SourceKind,
    pool: Option<Arc<SqlitePool>>,
    ledger: &dyn LedgerStore,
    counts: &IndexCounts,
    canonical_uri: &str,
    extra_candidates: Vec<GraphCandidate>,
) -> GraphWriteSummary {
    write_baseline_graph_with_db_gate(
        None,
        None,
        kind,
        pool,
        ledger,
        counts,
        canonical_uri,
        None,
        extra_candidates,
        None,
    )
    .await
}

pub(crate) async fn write_baseline_graph_with_db_gate(
    runtime: Option<&TargetLocalSourceRuntime>,
    graph_context: Option<ProviderCallContext>,
    kind: SourceKind,
    pool: Option<Arc<SqlitePool>>,
    ledger: &dyn LedgerStore,
    counts: &IndexCounts,
    canonical_uri: &str,
    published_manifest: Option<SourceManifest>,
    extra_candidates: Vec<GraphCandidate>,
    db_stage_slots: Option<Arc<Semaphore>>,
) -> GraphWriteSummary {
    let Some(pool) = pool else {
        tracing::debug!("no unified sqlite pool; skipping baseline graph write");
        return degraded_summary();
    };

    let manifest = if let Some(manifest) = published_manifest {
        manifest
    } else {
        match ledger
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
        }
    };

    // Enriching-stage observability is derived directly from the borrowed
    // candidates. Do not clone the generation's graph payload into a temporary
    // SourceEnrichment merely to count it before the real graph write.
    let valid_extras = filter_valid_candidates(extra_candidates, &counts.source_id);
    let parse_hint_count = valid_extras
        .iter()
        .filter_map(|candidate| candidate.producer.parser.as_deref())
        .collect::<HashSet<_>>()
        .len();
    let (enrichment_kind, enrichment_status) = if valid_extras.is_empty() {
        (EnrichmentKind::None, EnrichmentStatus::NotNeeded)
    } else {
        (EnrichmentKind::Extraction, EnrichmentStatus::Completed)
    };
    tracing::info!(
        source_id = %counts.source_id.0,
        enrichment_kind = ?enrichment_kind,
        enrichment_status = ?enrichment_status,
        parse_hints = parse_hint_count,
        graph_candidates = valid_extras.len(),
        "enriching stage validated parser-produced graph candidates"
    );

    let baseline_batch_count = manifest
        .items
        .len()
        .max(1)
        .div_ceil(BASELINE_GRAPH_BATCH_SIZE);
    let baseline = baseline_candidates(kind, counts, canonical_uri, &manifest);
    let candidates = baseline.chain(valid_extras).collect::<Vec<_>>();

    let _db_permit = match db_stage_slots {
        Some(slots) => match slots.acquire_owned().await {
            Ok(permit) => Some(permit),
            Err(_) => {
                tracing::warn!(
                    source_id = %counts.source_id.0,
                    "source database-stage admission gate closed before graph upsert"
                );
                return degraded_summary();
            }
        },
        None => None,
    };
    let write = match (runtime, graph_context) {
        (Some(runtime), Some(context)) => {
            reserved_call::upsert_graph_candidates(runtime, context, (*pool).clone(), candidates)
                .await
        }
        #[cfg(test)]
        (None, None) => {
            reserved_call::upsert_graph_candidates_for_test((*pool).clone(), candidates).await
        }
        _ => {
            tracing::warn!(
                source_id = %counts.source_id.0,
                "graph write is missing scheduler runtime/context; returning degraded summary"
            );
            return degraded_summary();
        }
    };
    match write {
        Ok(result) => GraphWriteSummary {
            // Every baseline chunk repeats the container node so its edges can
            // resolve locally. Hide those implementation-only repeat upserts
            // from the public summary.
            nodes_upserted: result
                .nodes_upserted
                .saturating_sub(baseline_batch_count.saturating_sub(1) as u64),
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

/// Lazily assemble bounded baseline graph candidates. Every chunk repeats the
/// source container node so containment edges resolve locally, while the graph
/// store consumes the iterator inside one transaction for atomic publication.
fn baseline_candidates<'a>(
    kind: SourceKind,
    counts: &'a IndexCounts,
    canonical_uri: &'a str,
    manifest: &'a SourceManifest,
) -> impl Iterator<Item = GraphCandidate> + 'a {
    let empty = manifest.items.is_empty().then_some(&manifest.items[..]);
    empty
        .into_iter()
        .chain(manifest.items.chunks(BASELINE_GRAPH_BATCH_SIZE))
        .enumerate()
        .map(move |(batch_index, items)| {
            build_candidate_batch(
                kind.clone(),
                manifest.scope,
                counts,
                canonical_uri,
                items,
                batch_index,
            )
        })
}

/// Assemble the full baseline candidate for tests and small direct callers.
#[cfg(test)]
fn build_candidate(
    kind: SourceKind,
    counts: &IndexCounts,
    canonical_uri: &str,
    manifest: &SourceManifest,
) -> GraphCandidate {
    build_candidate_batch(
        kind,
        manifest.scope,
        counts,
        canonical_uri,
        &manifest.items,
        0,
    )
}

fn build_candidate_batch(
    kind: SourceKind,
    scope: SourceScope,
    counts: &IndexCounts,
    canonical_uri: &str,
    items: &[ManifestItem],
    batch_index: usize,
) -> GraphCandidate {
    let source_id = counts.source_id.clone();
    let source_item_key = SourceItemKey::new(canonical_uri);
    let container_key = container_stable_key(&source_id, canonical_uri);
    // Carry the real canonical URI as a node property: without it,
    // `canonical_uri_for` falls back to the composite stable key, the
    // canonical_uri alias stores that composite, and `graph resolve <uri>` /
    // `graph query <uri>` can never match a plain URI (seen live on the
    // reset 7.0 stores).
    let container = GraphNodeCandidate {
        node_kind: container_node_kind(kind, scope).to_string(),
        stable_key: container_key.clone(),
        label: canonical_uri.to_string(),
        properties: uri_properties(canonical_uri),
    };

    let mut nodes = vec![container];
    let mut edges = Vec::new();
    let mut evidence = Vec::new();
    let edge_kind = containment_edge_kind(kind, scope);

    for item in items {
        let doc_key = document_stable_key(item);
        let item_evidence = containment_evidence(&source_id, &source_item_key, item);
        nodes.push(GraphNodeCandidate {
            node_kind: document_node_kind(item).to_string(),
            stable_key: doc_key.clone(),
            label: item.canonical_uri.clone(),
            properties: uri_properties(&item.canonical_uri),
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

    let candidate_id = if batch_index == 0 {
        format!("source-baseline:{}:{}", source_id.0, counts.generation.0)
    } else {
        format!(
            "source-baseline:{}:{}:{batch_index}",
            source_id.0, counts.generation.0
        )
    };
    GraphCandidate {
        candidate_id,
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
/// Node properties carrying the plain canonical URI so graph merge stores it
/// (and its alias) as the node's canonical identity instead of falling back
/// to the composite stable key.
fn uri_properties(canonical_uri: &str) -> MetadataMap {
    let mut properties = MetadataMap::new();
    properties.insert(
        "canonical_uri".to_string(),
        serde_json::json!(canonical_uri),
    );
    properties
}

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
fn container_node_kind(kind: SourceKind, scope: SourceScope) -> &'static str {
    match (kind, scope) {
        (SourceKind::Registry, SourceScope::Api) => "source",
        (SourceKind::Web, _) => "web_origin",
        (SourceKind::Git, _) => "repo",
        (SourceKind::Local, _) => "local_checkout",
        (SourceKind::Feed, _) => "feed",
        (SourceKind::Reddit, _) => "reddit_subreddit",
        (SourceKind::Youtube, _) => "youtube_channel",
        (SourceKind::Session, _) => "session",
        (SourceKind::Registry, _) => "package",
        (SourceKind::CliTool | SourceKind::McpTool, _) => "artifact",
        (SourceKind::Memory, _) => "source",
        (SourceKind::Upload, _) => "derived_source",
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
fn containment_edge_kind(kind: SourceKind, scope: SourceScope) -> &'static str {
    if kind == SourceKind::Registry && scope == SourceScope::Api {
        return "contains_artifact";
    }
    match kind {
        SourceKind::Web => "docs_site_contains_page",
        SourceKind::Git => "commit_contains_file",
        SourceKind::Local => "commit_contains_file",
        SourceKind::Feed => "feed_contains_entry",
        SourceKind::Reddit => "subreddit_has_thread",
        SourceKind::Youtube => "youtube_channel_has_video",
        SourceKind::Session => "session_has_turn",
        SourceKind::Registry => "package_has_version",
        SourceKind::CliTool | SourceKind::McpTool => "source_produced_artifact",
        SourceKind::Memory | SourceKind::Upload => "source_indexed_as",
    }
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
