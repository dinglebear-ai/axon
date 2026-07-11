//! Memory lifecycle mirror into the SourceGraph, plus the shared-pipeline
//! fact-kind contract markers consumed by `axon-adapters`' source family
//! matrix.
//!
//! Every memory is a graph node (`GraphNodeKind::Memory`); lifecycle events
//! that relate two memories become edges in the closed registry
//! (`memory_supersedes`, `memory_contradicts`, `memory_compacts`). This is a
//! mirror, not the source of truth — SQLite (`crate::sqlite`) stays
//! authoritative for status/decay/recall; the graph only records the
//! relationships so other domains (sessions, tools, repos) can traverse
//! through them.

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use axon_graph::GraphEdgeKind;
use axon_graph::store::GraphStore;

use crate::store::{MemoryStore, Result};

pub const MODULE_NAME: &str = "graph";
pub const MEMORY_GRAPH_REQUIRED_FACT: &str = "memory_document";
pub const MEMORY_GRAPH_OPTIONAL_FACTS: &[&str] = &["memory_link", "supersedes"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryGraphCandidate {
    pub memory_id: String,
    pub fact_kind: &'static str,
}

pub fn memory_graph_candidates(memory_id: impl Into<String>) -> Vec<MemoryGraphCandidate> {
    vec![MemoryGraphCandidate {
        memory_id: memory_id.into(),
        fact_kind: MEMORY_GRAPH_REQUIRED_FACT,
    }]
}

/// A memory-authored graph action is a deliberate, explicit claim — not an
/// inferred one — so it gets the highest evidence authority in the registry.
const MEMORY_EVIDENCE_KIND: &str = "user_pinned";
const MEMORY_SOURCE_ID: &str = "axon-memory";
const MEMORY_NODE_KIND: &str = "memory";

/// Mirrors memory lifecycle events into the SourceGraph.
#[async_trait]
pub trait MemoryGraphMirror: Send + Sync {
    /// Upsert (or refresh) a memory's graph node. Called on `remember` and on
    /// any status transition so the node's `memory_status` property stays
    /// current for graph queries.
    async fn upsert_memory_node(&self, record: &MemoryRecord) -> Result<()>;

    /// Record that `replacement` supersedes `old`.
    async fn supersedes(
        &self,
        replacement: &MemoryRecord,
        old: &MemoryRecord,
        reason: Option<&str>,
    ) -> Result<()>;

    /// Record that two memories contradict each other.
    async fn contradicts(
        &self,
        left: &MemoryRecord,
        right: &MemoryRecord,
        reason: Option<&str>,
    ) -> Result<()>;

    /// Record that `compacted` was derived from `sources`.
    async fn derived_from(&self, compacted: &MemoryRecord, sources: &[MemoryRecord]) -> Result<()>;

    /// Record an evidence-backed edge from `record`'s memory node to
    /// `link.target` (contract "Graph Integration": "link | create
    /// evidence-backed edge to source/repo/file/issue/pr/entity"). The
    /// target is referenced by stable key only — this crate does not own
    /// the target node's shape/kind, which belongs to whichever domain
    /// produced it.
    async fn link(&self, record: &MemoryRecord, link: &MemoryLink) -> Result<()>;

    /// Mark a memory's node as no longer recallable (forgotten). The node
    /// stays (history is never lost per the crate's decay invariant) but its
    /// `memory_status` property flips so graph queries can filter it out.
    async fn hide_recall_edges(&self, memory_id: &MemoryId, reason: &str) -> Result<()>;
}

/// Real [`MemoryGraphMirror`] backed by an injected [`GraphStore`].
pub struct GraphBackedMemoryMirror {
    graph: Arc<dyn GraphStore>,
}

impl GraphBackedMemoryMirror {
    pub fn new(graph: Arc<dyn GraphStore>) -> Self {
        Self { graph }
    }
}

#[async_trait]
impl MemoryGraphMirror for GraphBackedMemoryMirror {
    async fn upsert_memory_node(&self, record: &MemoryRecord) -> Result<()> {
        let candidate = node_only_candidate(
            format!("memory-upsert:{}", record.memory_id.0),
            memory_node(record),
        );
        self.graph.upsert_candidates(vec![candidate]).await?;
        Ok(())
    }

    async fn supersedes(
        &self,
        replacement: &MemoryRecord,
        old: &MemoryRecord,
        reason: Option<&str>,
    ) -> Result<()> {
        let candidate = edge_candidate(
            format!(
                "memory-supersedes:{}:{}",
                replacement.memory_id.0, old.memory_id.0
            ),
            vec![memory_node(replacement), memory_node(old)],
            GraphEdgeKind::MemorySupersedes,
            &memory_stable_key(&replacement.memory_id),
            &memory_stable_key(&old.memory_id),
            reason,
        );
        self.graph.upsert_candidates(vec![candidate]).await?;
        Ok(())
    }

    async fn contradicts(
        &self,
        left: &MemoryRecord,
        right: &MemoryRecord,
        reason: Option<&str>,
    ) -> Result<()> {
        let candidate = edge_candidate(
            format!(
                "memory-contradicts:{}:{}",
                left.memory_id.0, right.memory_id.0
            ),
            vec![memory_node(left), memory_node(right)],
            GraphEdgeKind::MemoryContradicts,
            &memory_stable_key(&left.memory_id),
            &memory_stable_key(&right.memory_id),
            reason,
        );
        self.graph.upsert_candidates(vec![candidate]).await?;
        Ok(())
    }

    async fn derived_from(&self, compacted: &MemoryRecord, sources: &[MemoryRecord]) -> Result<()> {
        let mut nodes = vec![memory_node(compacted)];
        nodes.extend(sources.iter().map(memory_node));
        let compacted_key = memory_stable_key(&compacted.memory_id);
        let candidates = sources
            .iter()
            .map(|source| {
                edge_candidate(
                    format!(
                        "memory-compacts:{}:{}",
                        compacted.memory_id.0, source.memory_id.0
                    ),
                    nodes.clone(),
                    GraphEdgeKind::MemoryCompacts,
                    &compacted_key,
                    &memory_stable_key(&source.memory_id),
                    Some("compacted from source memories"),
                )
            })
            .collect();
        self.graph.upsert_candidates(candidates).await?;
        Ok(())
    }

    async fn link(&self, record: &MemoryRecord, link: &MemoryLink) -> Result<()> {
        let memory_key = memory_stable_key(&record.memory_id);
        let candidate = edge_candidate(
            format!(
                "memory-link:{}:{}:{}",
                record.memory_id.0, link.link_type, link.target
            ),
            vec![memory_node(record)],
            link_type_to_edge_kind(&link.link_type),
            &memory_key,
            &link.target,
            Some(link.link_type.as_str()),
        );
        self.graph.upsert_candidates(vec![candidate]).await?;
        Ok(())
    }

    async fn hide_recall_edges(&self, memory_id: &MemoryId, reason: &str) -> Result<()> {
        let mut properties = MetadataMap::new();
        properties.insert(
            "memory_status".to_string(),
            serde_json::Value::String("forgotten".to_string()),
        );
        properties.insert(
            "memory_forgotten_reason".to_string(),
            serde_json::Value::String(reason.to_string()),
        );
        let candidate = node_only_candidate(
            format!("memory-hide:{}", memory_id.0),
            GraphNodeCandidate {
                node_kind: MEMORY_NODE_KIND.to_string(),
                stable_key: memory_stable_key(memory_id),
                label: memory_id.0.clone(),
                properties,
            },
        );
        self.graph.upsert_candidates(vec![candidate]).await?;
        Ok(())
    }
}

fn memory_stable_key(memory_id: &MemoryId) -> String {
    format!("memory:{}", memory_id.0)
}

/// Map a [`MemoryLink::link_type`] to the closed [`GraphEdgeKind`] registry
/// (contract "Graph Integration": "link | create evidence-backed edge to
/// source/repo/file/issue/pr/entity"). Unrecognized/free-text link types
/// fall back to the generic `memory_relates_to` edge rather than failing the
/// link write — the link type is caller-supplied free text, not a closed
/// enum on the wire (see [`MemoryLink::link_type`]).
fn link_type_to_edge_kind(link_type: &str) -> GraphEdgeKind {
    match link_type.to_ascii_lowercase().as_str() {
        "source" | "repo" | "repository" => GraphEdgeKind::MemoryAboutSource,
        "file" => GraphEdgeKind::MemoryAboutFile,
        "issue" | "pr" | "pull_request" | "ticket" => GraphEdgeKind::MemoryAboutIssue,
        _ => GraphEdgeKind::MemoryRelatesTo,
    }
}

fn memory_node(record: &MemoryRecord) -> GraphNodeCandidate {
    let mut properties = MetadataMap::new();
    properties.insert(
        "memory_status".to_string(),
        serde_json::Value::String(format!("{:?}", record.status).to_lowercase()),
    );
    properties.insert(
        "memory_type".to_string(),
        serde_json::Value::String(format!("{:?}", record.memory_type).to_lowercase()),
    );
    GraphNodeCandidate {
        node_kind: MEMORY_NODE_KIND.to_string(),
        stable_key: memory_stable_key(&record.memory_id),
        label: record
            .title
            .clone()
            .unwrap_or_else(|| record.memory_id.0.clone()),
        properties,
    }
}

fn node_only_candidate(candidate_id: String, node: GraphNodeCandidate) -> GraphCandidate {
    GraphCandidate {
        candidate_id: candidate_id.clone(),
        job_id: JobId::new(uuid::Uuid::new_v4()),
        source_id: SourceId::new(MEMORY_SOURCE_ID),
        source_item_key: SourceItemKey::new(candidate_id),
        item_canonical_uri: format!("memory://{}", node.stable_key),
        document_id: None,
        kind: "memory_lifecycle".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: MEMORY_SOURCE_ID.to_string(),
            parser: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        nodes: vec![node],
        edges: Vec::new(),
        evidence: Vec::new(),
        confidence: 1.0,
        metadata: MetadataMap::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn edge_candidate(
    candidate_id: String,
    nodes: Vec<GraphNodeCandidate>,
    edge_kind: GraphEdgeKind,
    from_stable_key: &str,
    to_stable_key: &str,
    reason: Option<&str>,
) -> GraphCandidate {
    let evidence_id = format!("{candidate_id}:evidence");
    GraphCandidate {
        candidate_id: candidate_id.clone(),
        job_id: JobId::new(uuid::Uuid::new_v4()),
        source_id: SourceId::new(MEMORY_SOURCE_ID),
        source_item_key: SourceItemKey::new(candidate_id.clone()),
        item_canonical_uri: format!("memory://{from_stable_key}"),
        document_id: None,
        kind: "memory_lifecycle".to_string(),
        merge_key: None,
        producer: GraphCandidateProducer {
            adapter: MEMORY_SOURCE_ID.to_string(),
            parser: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        nodes,
        edges: vec![GraphEdgeCandidate {
            edge_kind: edge_kind.as_str().to_string(),
            from_stable_key: from_stable_key.to_string(),
            to_stable_key: to_stable_key.to_string(),
            properties: MetadataMap::new(),
        }],
        evidence: vec![GraphEvidence {
            evidence_id,
            evidence_kind: MEMORY_EVIDENCE_KIND.to_string(),
            source_id: SourceId::new(MEMORY_SOURCE_ID),
            source_item_key: SourceItemKey::new(candidate_id),
            document_id: None,
            chunk_id: None,
            range: None,
            quote: reason.map(ToOwned::to_owned),
            confidence: 1.0,
            metadata: MetadataMap::new(),
        }],
        confidence: 1.0,
        metadata: MetadataMap::new(),
    }
}

/// [`MemoryStore`] decorator that mirrors lifecycle events into the graph
/// via an injected [`MemoryGraphMirror`], delegating everything else to
/// `inner`. Composes the same way [`crate::vector::VectorBackedMemoryStore`]
/// does — wrap the innermost (possibly vector-backed) store with this one.
pub struct GraphBackedMemoryStore {
    inner: Arc<dyn MemoryStore>,
    mirror: Arc<dyn MemoryGraphMirror>,
}

impl GraphBackedMemoryStore {
    pub fn new(inner: Arc<dyn MemoryStore>, mirror: Arc<dyn MemoryGraphMirror>) -> Self {
        Self { inner, mirror }
    }

    async fn load_or_missing(&self, memory_id: &MemoryId) -> Result<MemoryRecord> {
        self.inner
            .get(memory_id.clone())
            .await?
            .ok_or_else(|| missing_memory(memory_id))
    }
}

fn missing_memory(memory_id: &MemoryId) -> ApiError {
    ApiError::new(
        "memory.not_found",
        axon_error::ErrorStage::Retrieving,
        format!("memory not found after mutation: {}", memory_id.0),
    )
}

#[async_trait]
impl MemoryStore for GraphBackedMemoryStore {
    async fn remember(&self, request: MemoryRequest) -> Result<MemoryResult> {
        let result = self.inner.remember(request).await?;
        let record = self.load_or_missing(&result.memory_id).await?;
        self.mirror.upsert_memory_node(&record).await?;
        Ok(result)
    }

    async fn get(&self, memory_id: MemoryId) -> Result<Option<MemoryRecord>> {
        self.inner.get(memory_id).await
    }

    async fn load_many(&self, memory_ids: Vec<MemoryId>) -> Result<Vec<Option<MemoryRecord>>> {
        self.inner.load_many(memory_ids).await
    }

    async fn search(&self, request: MemorySearchRequest) -> Result<MemorySearchResult> {
        self.inner.search(request).await
    }

    async fn context(&self, request: MemoryContextRequest) -> Result<MemoryContextResult> {
        self.inner.context(request).await
    }

    async fn link(&self, request: MemoryLinkRequest) -> Result<MemoryResult> {
        let memory_id = request.memory_id.clone();
        let link = request.link.clone();
        let result = self.inner.link(request).await?;
        let record = self.load_or_missing(&memory_id).await?;
        self.mirror.link(&record, &link).await?;
        Ok(result)
    }

    async fn reinforce(
        &self,
        memory_id: MemoryId,
        signal: MemoryReinforcement,
    ) -> Result<MemoryResult> {
        self.inner.reinforce(memory_id, signal).await
    }

    async fn supersede(&self, request: MemorySupersedeRequest) -> Result<MemoryResult> {
        let replacement_id = request.replacement_id.clone();
        let old_id = request.memory_id.clone();
        let reason = request.reason.clone();
        let result = self.inner.supersede(request).await?;
        let replacement = self.load_or_missing(&replacement_id).await?;
        let old = self.load_or_missing(&old_id).await?;
        self.mirror
            .supersedes(&replacement, &old, reason.as_deref())
            .await?;
        Ok(result)
    }

    async fn contradict(&self, request: MemoryContradictRequest) -> Result<MemoryResult> {
        let left_id = request.memory_id.clone();
        let right_id = request.conflicting_id.clone();
        let reason = request.reason.clone();
        let result = self.inner.contradict(request).await?;
        let left = self.load_or_missing(&left_id).await?;
        let right = self.load_or_missing(&right_id).await?;
        self.mirror
            .contradicts(&left, &right, reason.as_deref())
            .await?;
        Ok(result)
    }

    async fn set_status(&self, request: MemoryStatusRequest) -> Result<MemoryResult> {
        let memory_id = request.memory_id.clone();
        let status = request.status;
        let reason = request.reason.clone();
        let result = self.inner.set_status(request).await?;
        if status == MemoryStatus::Forgotten {
            self.mirror
                .hide_recall_edges(
                    &memory_id,
                    reason.as_deref().unwrap_or("status -> forgotten"),
                )
                .await?;
        } else {
            let record = self.load_or_missing(&memory_id).await?;
            self.mirror.upsert_memory_node(&record).await?;
        }
        Ok(result)
    }

    async fn review(&self, request: MemoryReviewRequest) -> Result<MemoryReviewResult> {
        self.inner.review(request).await
    }

    async fn update(&self, request: MemoryUpdateRequest) -> Result<MemoryResult> {
        let memory_id = request.memory_id.clone();
        let result = self.inner.update(request).await?;
        let record = self.load_or_missing(&memory_id).await?;
        self.mirror.upsert_memory_node(&record).await?;
        Ok(result)
    }

    async fn pin(&self, request: MemoryPinRequest) -> Result<MemoryResult> {
        let memory_id = request.memory_id.clone();
        let result = self.inner.pin(request).await?;
        let record = self.load_or_missing(&memory_id).await?;
        self.mirror.upsert_memory_node(&record).await?;
        Ok(result)
    }

    async fn archive(&self, request: MemoryArchiveRequest) -> Result<MemoryResult> {
        let memory_id = request.memory_id.clone();
        let result = self.inner.archive(request).await?;
        let record = self.load_or_missing(&memory_id).await?;
        self.mirror.upsert_memory_node(&record).await?;
        Ok(result)
    }

    async fn forget(&self, request: MemoryForgetRequest) -> Result<MemoryResult> {
        let memory_id = request.memory_id.clone();
        let reason = request
            .reason
            .clone()
            .unwrap_or_else(|| "status -> forgotten".to_string());
        let result = self.inner.forget(request).await?;
        self.mirror.hide_recall_edges(&memory_id, &reason).await?;
        Ok(result)
    }

    async fn compact(&self, request: MemoryCompactRequest) -> Result<MemoryResult> {
        let source_ids = request.memory_ids.clone();
        let mut sources = Vec::with_capacity(source_ids.len());
        for source_id in &source_ids {
            sources.push(self.load_or_missing(source_id).await?);
        }
        let result = self.inner.compact(request).await?;
        let compacted = self.load_or_missing(&result.memory_id).await?;
        self.mirror.derived_from(&compacted, &sources).await?;
        Ok(result)
    }

    async fn import(&self, request: MemoryImportRequest) -> Result<MemoryImportResult> {
        self.inner.import(request).await
    }

    async fn export(&self, request: MemoryExportRequest) -> Result<MemoryExportResult> {
        self.inner.export(request).await
    }

    async fn reset(&self) -> Result<()> {
        self.inner.reset().await
    }

    async fn capabilities(&self) -> Result<MemoryStoreCapability> {
        self.inner.capabilities().await
    }
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
