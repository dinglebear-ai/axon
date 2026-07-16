//! Vector-path semantic search, split out of `vector.rs` to keep that file
//! under the monolith line cap.

use axon_api::source::*;
use serde_json::json;

use super::VectorBackedMemoryStore;
use crate::graph_refs::graph_refs_for_memory_results;
use crate::record::age_days;
use crate::store::Result;

impl VectorBackedMemoryStore {
    /// Embed a memory query string into a single dense vector via the
    /// injected embedding provider. Split out of `search_vectors` to keep it
    /// under the monolith function-length cap.
    async fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let query_chunk = ChunkId::new("memory-query");
        let embedding = self
            .embeddings
            .embed(EmbeddingBatch {
                batch_id: BatchId::new(uuid::Uuid::new_v4()),
                job_id: JobId::new(uuid::Uuid::new_v4()),
                provider_id: self.config.embedding_provider_id.clone(),
                model: self.config.embedding_model.clone(),
                items: vec![EmbeddingInput {
                    chunk_id: query_chunk.clone(),
                    text: query.to_string(),
                    content_kind: ContentKind::PlainText,
                    metadata: MetadataMap::new(),
                }],
                instruction: None,
                priority: JobPriority::Interactive,
                metadata: MetadataMap::new(),
            })
            .await?;
        embedding
            .vectors
            .into_iter()
            .find(|vector| vector.chunk_id == query_chunk)
            .map(|vector| vector.values)
            .ok_or_else(|| {
                ApiError::new(
                    "memory.query_embedding_missing",
                    axon_error::ErrorStage::Retrieving,
                    "embedding provider did not return a memory query vector",
                )
            })
    }

    pub(super) async fn search_vectors(
        &self,
        request: &MemorySearchRequest,
    ) -> Result<MemorySearchResult> {
        let dense_vector = self.embed_query(&request.query).await?;
        let mut filters = request.filters.clone();
        filters.insert("source_kind".to_string(), json!("memory"));
        if !request.include_archived {
            filters.insert("memory_recallable".to_string(), json!(true));
        }
        let search = self
            .vectors
            .search(VectorSearchRequest {
                collection: self.config.collection.clone(),
                query: request.query.clone(),
                limit: request.limit,
                dense_vector: Some(dense_vector),
                sparse_vector: None,
                filters,
                hybrid: Some(false),
                generation: None,
                graph_refs: Vec::new(),
                metadata: MetadataMap::new(),
            })
            .await?;
        let mut warnings = search.warnings;
        // Pair each hit with its `memory_id`, dropping (with a warning) any
        // hit whose payload is missing the field — done here, before
        // `load_many`, so `memory_ids` and `hits_with_id` stay the same
        // length and in the same order. Previously `memory_ids` was built by
        // `filter_map`-ing `search.results` but then zipped against the
        // *original, unfiltered* `search.results`: a payload gap partway
        // through the response would silently pair every later hit with the
        // wrong `MemoryRecord`/score.
        let mut memory_ids = Vec::with_capacity(search.results.len());
        let mut hits_with_id = Vec::with_capacity(search.results.len());
        for hit in search.results {
            match hit
                .payload
                .get("memory_id")
                .and_then(serde_json::Value::as_str)
            {
                Some(id) => {
                    memory_ids.push(MemoryId::new(id));
                    hits_with_id.push(hit);
                }
                None => {
                    warnings.push(SourceWarning {
                        code: "memory.payload_missing_id".to_string(),
                        severity: Severity::Warning,
                        message: "memory vector hit had no memory_id in its payload".to_string(),
                        source_item_key: None,
                        retryable: true,
                    });
                }
            }
        }
        let records = self.inner.load_many(memory_ids).await?;
        let mut results = Vec::new();
        // Contract "Score formula": vector recall must return the blended
        // `memory_score` (semantic x confidence x salience x scope x
        // reinforcement, then decay/contradiction/status-penalized), not the
        // raw Qdrant similarity `hit.score`. Reuse the same
        // `crate::decay::score_record` + scope-match input the keyword recall
        // path (`sqlite::recall::search`) uses, so both recall paths rank
        // identically for the same record.
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0);
        let scope_filter = request
            .filters
            .get("scope")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        for (hit, record) in hits_with_id.into_iter().zip(records) {
            let Some(record) = record else {
                warnings.push(SourceWarning {
                    code: "memory.metadata_missing".to_string(),
                    severity: Severity::Warning,
                    message: "memory vector hit had no SQLite metadata row".to_string(),
                    source_item_key: None,
                    retryable: true,
                });
                continue;
            };
            if !request.include_archived && record.status != MemoryStatus::Active {
                continue;
            }
            let semantic_score = crate::decay::clamp01(hit.score as f32);
            let age = age_days(&record, now_secs);
            let scope_match =
                crate::sqlite::recall::scope_match_score(&record, scope_filter.as_deref());
            let score = crate::decay::score_record(
                &record,
                age,
                semantic_score,
                scope_match,
                request.include_archived,
            );
            results.push(MemorySearchMatch { record, score });
        }
        let graph = if request.include_graph {
            graph_refs_for_memory_results(self.graph.as_deref(), &results, &mut warnings).await?
        } else {
            None
        };
        Ok(MemorySearchResult {
            results,
            query_embedding_model: Some(self.config.embedding_model.clone()),
            graph,
            warnings,
        })
    }
}
