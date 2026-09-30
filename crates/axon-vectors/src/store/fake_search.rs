//! Search implementation for the deterministic vector store.

use axon_api::source::*;

use crate::filter::{matches_search_filters, validate_search_filters};
use crate::store_helpers::{dot_score, payload_string, sparse_dot_score};

use super::{FakeVectorStore, Result};

impl FakeVectorStore {
    pub(super) async fn search_inner(
        &self,
        request: VectorSearchRequest,
    ) -> Result<VectorSearchResult> {
        let mut state = self.state.lock().await;
        state.calls.push("search");
        if let Some(err) = self.mode_error_for(ErrorStage::Retrieving) {
            return Err(err);
        }
        let query_vector = request.dense_vector.as_deref().ok_or_else(|| {
            ApiError::new(
                "vector.missing_query_vector",
                ErrorStage::Retrieving,
                "fake vector store search requires a dense query vector",
            )
        })?;
        let query_sparse = request.sparse_vector.as_ref();
        let spec = state.collection_spec(&request.collection, ErrorStage::Retrieving)?;
        if query_vector.len() as u32 != spec.dense.dimensions {
            return Err(ApiError::new(
                "vector.dimension_mismatch",
                ErrorStage::Retrieving,
                format!(
                    "query vector dimensions {} do not match collection dimensions {}",
                    query_vector.len(),
                    spec.dense.dimensions
                ),
            ));
        }
        if (query_sparse.is_some() || request.hybrid == Some(true)) && spec.sparse.is_none() {
            return Err(ApiError::new(
                "vector.sparse_not_configured",
                ErrorStage::Retrieving,
                format!(
                    "collection {} does not declare a sparse vector namespace",
                    request.collection
                ),
            ));
        }
        validate_search_filters(&request)?;
        let limit = request.limit as usize;
        let mut scored = state
            .points
            .get(&request.collection)
            .into_iter()
            .flat_map(|points| points.values())
            .filter(|point| matches_search_filters(point, &request))
            .map(|point| {
                (
                    point,
                    dot_score(query_vector, &point.vector)
                        + sparse_dot_score(query_sparse, point.sparse_vector.as_ref()),
                )
            })
            .collect::<Vec<_>>();
        scored.sort_by(|(left_point, left_score), (right_point, right_score)| {
            right_score
                .total_cmp(left_score)
                .then(left_point.point_id.0.cmp(&right_point.point_id.0))
        });
        scored.truncate(limit);
        let results = scored
            .into_iter()
            .map(|(point, score)| VectorSearchMatch {
                point_id: point.point_id.clone(),
                score,
                chunk_id: Some(point.chunk_id.clone()),
                document_id: payload_string(&point.payload, "document_id").map(DocumentId::new),
                source_id: payload_string(&point.payload, "source_id").map(SourceId::new),
                source_item_key: payload_string(&point.payload, "source_item_key")
                    .map(SourceItemKey::new),
                text: payload_string(&point.payload, "chunk_text"),
                payload: point.payload.clone(),
            })
            .collect();
        Ok(VectorSearchResult {
            collection: request.collection,
            results,
            limit: request.limit,
            next_cursor: None,
            warnings: Vec::new(),
            metadata: MetadataMap::new(),
        })
    }
}
