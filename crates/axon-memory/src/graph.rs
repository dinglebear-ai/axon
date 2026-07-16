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
use axon_graph::store::GraphStore;

use crate::store::{MemoryStore, Result};

mod batch;
mod candidates;
use candidates::*;

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

    /// Upsert one transaction-sized group of memory nodes. Implementations
    /// backed by a transactional graph store should override this so the
    /// whole slice commits atomically. The default preserves compatibility
    /// for lightweight mirrors while still bounding caller-side work.
    async fn upsert_memory_nodes(&self, records: &[MemoryRecord]) -> Result<()> {
        for record in records {
            self.upsert_memory_node(record).await?;
        }
        Ok(())
    }

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
    /// `target`'s memory node (contract "Graph Integration": "link | create
    /// evidence-backed edge"). Both endpoints are emitted as nodes and keyed by
    /// their canonical `memory:<id>` stable key, so the edge passes graph
    /// candidate validation (which requires both endpoints to resolve to a node
    /// present in the candidate). The service layer validates both ids exist as
    /// memories before this is called.
    async fn link(
        &self,
        record: &MemoryRecord,
        target: &MemoryRecord,
        link: &MemoryLink,
    ) -> Result<()>;

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

    async fn upsert_memory_nodes(&self, records: &[MemoryRecord]) -> Result<()> {
        let candidates = records
            .iter()
            .map(|record| {
                node_only_candidate(
                    format!("memory-upsert:{}", record.memory_id.0),
                    memory_node(record),
                )
            })
            .collect();
        self.graph.upsert_candidates(candidates).await?;
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

    async fn link(
        &self,
        record: &MemoryRecord,
        target: &MemoryRecord,
        link: &MemoryLink,
    ) -> Result<()> {
        let memory_key = memory_stable_key(&record.memory_id);
        let target_key = memory_stable_key(&target.memory_id);
        let candidate = edge_candidate(
            format!(
                "memory-link:{}:{}:{}",
                record.memory_id.0, link.link_type, target.memory_id.0
            ),
            vec![memory_node(record), memory_node(target)],
            link_type_to_edge_kind(&link.link_type),
            &memory_key,
            &target_key,
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

/// [`MemoryStore`] decorator that mirrors lifecycle events into the graph
/// via an injected [`MemoryGraphMirror`], delegating everything else to
/// `inner`. Composes the same way [`crate::vector::VectorBackedMemoryStore`]
/// does — wrap the innermost (possibly vector-backed) store with this one.
pub struct GraphBackedMemoryStore {
    inner: Arc<dyn MemoryStore>,
    mirror: Arc<dyn MemoryGraphMirror>,
    graph: Option<Arc<dyn GraphStore>>,
    graph_tx_batch_size: usize,
}

impl GraphBackedMemoryStore {
    pub fn new(inner: Arc<dyn MemoryStore>, mirror: Arc<dyn MemoryGraphMirror>) -> Self {
        Self {
            inner,
            mirror,
            graph: None,
            graph_tx_batch_size: crate::vector::MemoryBatchLimits::default().graph_tx_batch_size,
        }
    }

    pub fn with_graph_store(mut self, graph: Arc<dyn GraphStore>) -> Self {
        self.graph = Some(graph);
        self
    }

    pub fn with_graph_tx_batch_size(mut self, graph_tx_batch_size: usize) -> Self {
        self.graph_tx_batch_size = graph_tx_batch_size.max(1);
        self
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
        let mut inner_request = request.clone();
        inner_request.include_graph = false;
        let mut result = self.inner.search(inner_request).await?;
        if request.include_graph && result.graph.is_none() {
            result.graph = crate::graph_refs::graph_refs_for_memory_results(
                self.graph.as_deref(),
                &result.results,
                &mut result.warnings,
            )
            .await?;
        }
        Ok(result)
    }

    async fn context(&self, request: MemoryContextRequest) -> Result<MemoryContextResult> {
        self.inner.context(request).await
    }

    async fn link(&self, request: MemoryLinkRequest) -> Result<MemoryResult> {
        let memory_id = request.memory_id.clone();
        let link = request.link.clone();
        let target_id = MemoryId::new(link.target.clone());
        let result = self.inner.link(request).await?;
        let record = self.load_or_missing(&memory_id).await?;
        let target = self.load_or_missing(&target_id).await?;
        self.mirror.link(&record, &target, &link).await?;
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
        let dry_run = request.dry_run;
        let mut result = self.inner.import(request).await?;
        if dry_run || result.created_ids.is_empty() {
            return Ok(result);
        }

        let loaded = self.inner.load_many(result.created_ids.clone()).await?;
        let records = result
            .created_ids
            .iter()
            .zip(loaded)
            .map(|(memory_id, record)| record.ok_or_else(|| missing_memory(memory_id)))
            .collect::<Result<Vec<_>>>()?;
        for chunk in records.chunks(self.graph_tx_batch_size) {
            if let Err(error) = self.mirror.upsert_memory_nodes(chunk).await {
                self.mark_graph_recovery(chunk, &error, &mut result.warnings)
                    .await?;
            }
        }
        Ok(result)
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
