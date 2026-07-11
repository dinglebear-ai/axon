//! Minimal retrieval engine for the boundary fake.

use std::sync::Arc;

use axon_api::source::{
    BatchId, ChunkId, ContentKind, EmbeddingBatch, EmbeddingInput, JobId, JobPriority, MetadataMap,
    ProviderId, VectorSearchMatch, VectorSearchRequest, Visibility,
};
use axon_embedding::provider::EmbeddingProvider;
use axon_error::{ApiError, ErrorStage};
use axon_vectors::store::VectorStore;
use uuid::Uuid;

use crate::citation::Citation;
use crate::context::ContextBundle;
use crate::plan::RetrievalPlan;
use crate::query::{RetrievalMatch, RetrievalRequest, RetrievalResult};

pub const MODULE_NAME: &str = "engine";

#[derive(Clone)]
pub struct RetrievalEngine<S, E> {
    store: Arc<S>,
    embedding_provider: Arc<E>,
    config: RetrievalEngineConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetrievalEngineConfig {
    pub(crate) embedding_provider_id: ProviderId,
    pub(crate) embedding_model: String,
    pub(crate) embedding_dimensions: u32,
    pub(crate) access: RetrievalAccess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetrievalAccess {
    pub(crate) allowed_visibility: Vec<Visibility>,
}

impl RetrievalAccess {
    pub(crate) fn standard() -> Self {
        Self {
            allowed_visibility: vec![
                Visibility::Public,
                Visibility::Internal,
                Visibility::Derived,
            ],
        }
    }
}

impl RetrievalEngineConfig {
    pub(crate) fn new(
        embedding_provider_id: ProviderId,
        embedding_model: impl Into<String>,
        embedding_dimensions: u32,
        access: RetrievalAccess,
    ) -> Self {
        Self {
            embedding_provider_id,
            embedding_model: embedding_model.into(),
            embedding_dimensions,
            access,
        }
    }
}

impl<S, E> RetrievalEngine<S, E>
where
    S: VectorStore + 'static,
    E: EmbeddingProvider + 'static,
{
    pub(crate) fn new(
        store: Arc<S>,
        embedding_provider: Arc<E>,
        config: RetrievalEngineConfig,
    ) -> Self {
        Self {
            store,
            embedding_provider,
            config,
        }
    }

    /// Inherent retrieval entry. Kept as the authoritative direct-call path —
    /// existing callers (e.g. `crate::service::run_query`) keep compiling
    /// unchanged. [`crate::boundary::RetrievalEngine::retrieve`] on this same
    /// type delegates straight back here: Rust resolves `self.retrieve(...)`
    /// to this inherent method rather than the trait method even from within
    /// the trait impl, because inherent methods always take priority over
    /// trait methods in dot-call resolution for a given receiver type.
    pub async fn retrieve(&self, request: RetrievalRequest) -> Result<RetrievalResult, ApiError> {
        let plan =
            RetrievalPlan::from_request(&request, self.config.access.allowed_visibility.clone());
        let dense_vector = self.embed_query(&request.query).await?;
        // bm42 sparse arm: compute the query's sparse vector locally (Qdrant
        // applies IDF server-side). An empty sparse vector (all-stopword / tiny
        // query) contributes nothing to RRF, so hybrid stays safe to request.
        let sparse_vector =
            axon_vectors::bm42::compute_bm42_sparse(ChunkId::new("query"), &request.query);
        let search = self
            .store
            .search(VectorSearchRequest {
                collection: plan.collection.clone(),
                query: request.query,
                limit: plan.limit,
                dense_vector: Some(dense_vector),
                sparse_vector: Some(sparse_vector),
                filters: search_filters(&plan)?,
                hybrid: Some(true),
                generation: plan.generation.clone(),
                graph_refs: Vec::new(),
                metadata: MetadataMap::new(),
            })
            .await?;

        let matches = search
            .results
            .iter()
            .filter(|item| !excluded_by_namespace(item, &plan))
            .map(match_from_vector)
            .collect::<Result<Vec<_>, _>>()?;
        let context = ContextBundle::from_matches(&matches, plan.byte_budget, plan.token_budget);
        if !matches.is_empty() && context.chunk_ids.is_empty() {
            return Err(ApiError::new(
                "retrieval.context_budget_too_small",
                ErrorStage::Retrieving,
                format!(
                    "retrieval context budget admitted no chunks; byte_budget={}, token_budget={}",
                    plan.byte_budget, plan.token_budget
                ),
            ));
        }
        let allowed_ids = context
            .chunk_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>();
        let citations = matches
            .iter()
            .filter(|item| allowed_ids.contains(&item.chunk_id))
            .map(|item| item.citation.clone())
            .collect();

        Ok(RetrievalResult {
            plan,
            matches,
            context,
            citations,
        })
    }

    async fn embed_query(&self, query: &str) -> Result<Vec<f32>, ApiError> {
        let result = self
            .embedding_provider
            .embed(EmbeddingBatch {
                batch_id: BatchId::new(Uuid::new_v4()),
                job_id: JobId::new(Uuid::new_v4()),
                provider_id: self.config.embedding_provider_id.clone(),
                model: self.config.embedding_model.clone(),
                items: vec![EmbeddingInput {
                    chunk_id: ChunkId::new("query"),
                    text: query.to_string(),
                    content_kind: ContentKind::PlainText,
                    metadata: MetadataMap::new(),
                }],
                instruction: None,
                priority: JobPriority::Interactive,
                metadata: MetadataMap::new(),
            })
            .await?;
        if result.provider_id != self.config.embedding_provider_id {
            return Err(ApiError::new(
                "retrieval.embedding_provider_mismatch",
                ErrorStage::Embedding,
                "embedding provider returned a vector for a different provider id",
            )
            .with_provider_id(&result.provider_id.0));
        }
        if result.model != self.config.embedding_model {
            return Err(ApiError::new(
                "retrieval.embedding_model_mismatch",
                ErrorStage::Embedding,
                "embedding provider returned a vector for a different model",
            ));
        }
        if result.dimensions != self.config.embedding_dimensions {
            return Err(ApiError::new(
                "retrieval.embedding_dimension_mismatch",
                ErrorStage::Embedding,
                format!(
                    "embedding provider returned {} dimensions, expected {}",
                    result.dimensions, self.config.embedding_dimensions
                ),
            ));
        }
        let vector = result
            .vectors
            .into_iter()
            .find(|vector| vector.chunk_id == ChunkId::new("query"))
            .ok_or_else(|| {
                ApiError::new(
                    "retrieval.missing_query_vector",
                    ErrorStage::Retrieving,
                    "embedding provider returned no query vector",
                )
            })?;
        if vector.values.len() as u32 != self.config.embedding_dimensions {
            return Err(ApiError::new(
                "retrieval.embedding_dimension_mismatch",
                ErrorStage::Embedding,
                format!(
                    "query vector has {} dimensions, expected {}",
                    vector.values.len(),
                    self.config.embedding_dimensions
                ),
            ));
        }
        Ok(vector.values)
    }
}

/// True when `item` must be dropped by [`RetrievalPlan::excluded_namespaces`].
/// Only applies when `namespace_filters` is empty — an explicit positive
/// namespace allow-list already governs which namespaces can appear, so the
/// default exclusion is only meaningful for unrestricted search.
fn excluded_by_namespace(item: &VectorSearchMatch, plan: &RetrievalPlan) -> bool {
    if !plan.namespace_filters.is_empty() || plan.excluded_namespaces.is_empty() {
        return false;
    }
    let Some(namespace) = item
        .payload
        .get("vector_namespace")
        .and_then(|v| v.as_str())
    else {
        return false;
    };
    plan.excluded_namespaces.iter().any(|n| n == namespace)
}

pub(crate) fn search_filters(plan: &RetrievalPlan) -> Result<MetadataMap, ApiError> {
    let mut filters = MetadataMap::new();
    filters.insert(
        "visibility".to_string(),
        serde_json::json!(
            plan.allowed_visibility
                .iter()
                .map(visibility_value)
                .collect::<Vec<_>>()
        ),
    );
    filters.insert("redaction_status".to_string(), serde_json::json!("clean"));
    filters.insert(
        "document_status".to_string(),
        serde_json::json!("published"),
    );
    if let Some(source_id) = &plan.source_id {
        filters.insert("source_id".to_string(), serde_json::json!(source_id.0));
    }
    if !plan.namespace_filters.is_empty() {
        filters.insert(
            "vector_namespace".to_string(),
            serde_json::json!(plan.namespace_filters),
        );
    }
    Ok(filters)
}

fn visibility_value(visibility: &Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public",
        Visibility::Internal => "internal",
        Visibility::Sensitive => "sensitive",
        Visibility::Redacted => "redacted",
        Visibility::Derived => "derived",
    }
}

pub(crate) fn match_from_vector(item: &VectorSearchMatch) -> Result<RetrievalMatch, ApiError> {
    require_clean_redaction_status(item)?;
    let citation = Citation::from_vector_match(item)?;
    Ok(RetrievalMatch {
        chunk_id: citation.chunk_id.clone(),
        document_id: citation.document_id.clone(),
        source_id: citation.source_id.clone(),
        score: item.score,
        canonical_uri: citation.canonical_uri.clone(),
        text: item
            .text
            .clone()
            .or_else(|| payload_string(&item.payload, "chunk_text"))
            .ok_or_else(|| {
                ApiError::new(
                    "retrieval.missing_chunk_text",
                    ErrorStage::Retrieving,
                    "vector search match did not include text or chunk_text payload",
                )
                .with_context("point_id", item.point_id.0.clone())
            })?,
        citation,
    })
}

fn require_clean_redaction_status(item: &VectorSearchMatch) -> Result<(), ApiError> {
    match item
        .payload
        .get("redaction_status")
        .and_then(serde_json::Value::as_str)
    {
        Some("clean") => Ok(()),
        Some(status) => Err(ApiError::new(
            "retrieval.redaction_status_not_clean",
            ErrorStage::Retrieving,
            format!(
                "vector match {} has redaction_status `{status}`",
                item.point_id.0
            ),
        )),
        None => Err(ApiError::new(
            "retrieval.missing_redaction_status",
            ErrorStage::Retrieving,
            format!(
                "vector match {} is missing redaction_status",
                item.point_id.0
            ),
        )),
    }
}

fn payload_string(payload: &MetadataMap, field: &str) -> Option<String> {
    payload.get(field)?.as_str().map(ToString::to_string)
}
