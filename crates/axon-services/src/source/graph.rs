//! Baseline SourceGraph write for `index_source`.
//!
//! After a source is acquired and indexed, this module upserts the minimal
//! *source graph skeleton* into the durable [`SqliteGraphStore`]:
//!
//! 1. one **container node** for the source itself (kind chosen per family from
//!    the closed registry, keyed by the source's canonical URI);
//! 2. one **document node** per indexed manifest item (kind derived from the
//!    item's [`ItemKind`], keyed by the item's canonical URI);
//! 3. one **containment edge** (container → document) using the family's
//!    natural containment edge kind, each backed by a `text_mention` evidence
//!    record so the candidate validates.
//!
//! This is the baseline skeleton only — deep entity/dependency-manifest
//! candidate extraction (repo→package edges, compose topology, session tool
//! calls, …) is a later bead. The goal here is a genuinely non-empty, correct
//! graph at runtime, built from the real per-document manifest the ledger
//! already stored during indexing.
//!
//! Per the crate-ownership rule, `axon-graph` owns the store and the closed kind
//! registry; this module only assembles [`GraphCandidate`] values and calls
//! `upsert_candidates`. When no target pool is available (no unified SQLite
//! runtime), the write is skipped and a degraded [`GraphWriteSummary`] with zero
//! counts is returned — acquisition never crashes because of the graph write.

use std::sync::Arc;

use axon_api::source::{
    GraphCandidate, GraphCandidateProducer, GraphEdgeCandidate, GraphEvidence, GraphNodeCandidate,
    GraphWriteSummary, ItemKind, ManifestItem, MetadataMap, SourceId, SourceItemKey,
    SourceManifest,
};
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

/// Build and persist the baseline source graph for a completed index.
///
/// Reads the just-published manifest for `counts.source_id`/`counts.generation`
/// from the ledger, assembles one container node + one node/edge per document,
/// and upserts them into the durable graph on the unified pool. Returns the real
/// [`GraphWriteSummary`] from the store result.
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

    let candidate = build_candidate(kind, counts, canonical_uri, &manifest);
    let store = SqliteGraphStore::from_pool((*pool).clone());
    match store.upsert_candidates(vec![candidate]).await {
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
            properties: MetadataMap::new(),
        });
        evidence.push(containment_evidence(&source_id, item));
    }

    GraphCandidate {
        candidate_id: format!("source-baseline:{}:{}", source_id.0, counts.generation.0),
        job_id: counts.job_id.clone(),
        source_id: source_id.clone(),
        source_item_key: SourceItemKey::new(canonical_uri),
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
fn containment_evidence(source_id: &SourceId, item: &ManifestItem) -> GraphEvidence {
    GraphEvidence {
        evidence_id: format!("contains:{}", item.source_item_key.0),
        evidence_kind: "text_mention".to_string(),
        source_id: source_id.clone(),
        source_item_key: item.source_item_key.clone(),
        document_id: None,
        chunk_id: None,
        range: None,
        quote: None,
        confidence: BASELINE_CONFIDENCE,
        metadata: MetadataMap::new(),
    }
}

/// Stable key for the container node — the source's canonical URI, namespaced by
/// source id so distinct sources never collide on a shared URI shape.
fn container_stable_key(source_id: &SourceId, canonical_uri: &str) -> String {
    format!("source:{}:{}", source_id.0, canonical_uri)
}

/// Stable key for a document node — the item's own stable source key.
fn document_stable_key(item: &ManifestItem) -> String {
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
        SourceInputKind::Unsupported => "source_produced_artifact",
    }
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
