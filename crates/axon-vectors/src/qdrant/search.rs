//! Qdrant `/points/query` search: named-dense and dense+bm42 RRF hybrid.

use axon_api::source::*;
use serde::Deserialize;

use super::convert::search_filter_json;
use super::http::QdrantHttp;
use crate::filter::validate_search_filters;

/// Default per-arm prefetch window before RRF fusion.
const DEFAULT_HYBRID_CANDIDATES: usize = 100;
const HNSW_EF_SEARCH: usize = 128;

/// A single scored hit from Qdrant's query response.
#[derive(Debug, Deserialize)]
struct QdrantSearchHit {
    id: serde_json::Value,
    #[serde(default)]
    score: f64,
    #[serde(default)]
    payload: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct QdrantQueryPoints {
    #[serde(default)]
    points: Vec<QdrantSearchHit>,
}

#[derive(Debug, Deserialize)]
struct QdrantQueryResponse {
    result: QdrantQueryPoints,
}

/// Execute a search request against a live Qdrant collection.
///
/// Runs dense+bm42 RRF hybrid when a sparse vector is present (or `hybrid` is
/// requested and the collection is sparse-capable); otherwise a named-dense
/// `/points/query`. Filters — including the generation fence on
/// `committed_generation` — are converted to a Qdrant filter and applied.
pub async fn qdrant_search(
    http: &QdrantHttp,
    spec: &CollectionSpec,
    request: &VectorSearchRequest,
) -> Result<VectorSearchResult, ApiError> {
    let stage = axon_error::ErrorStage::Retrieving;
    let dense = request.dense_vector.as_deref().ok_or_else(|| {
        ApiError::new(
            "vector.missing_query_vector",
            stage,
            "qdrant vector store search requires a dense query vector",
        )
    })?;
    if dense.len() as u32 != spec.dense.dimensions {
        return Err(ApiError::new(
            "vector.dimension_mismatch",
            stage,
            format!(
                "query vector dimensions {} do not match collection dimensions {}",
                dense.len(),
                spec.dense.dimensions
            ),
        ));
    }
    let wants_sparse = request.sparse_vector.is_some() || request.hybrid == Some(true);
    if wants_sparse && spec.sparse.is_none() {
        return Err(ApiError::new(
            "vector.sparse_not_configured",
            stage,
            format!(
                "collection {} does not declare a sparse vector namespace",
                request.collection
            ),
        ));
    }
    validate_search_filters(request)?;
    let filter_json = search_filter_json(request)?;

    let limit = request.limit.max(1) as usize;
    let dense_name = &spec.dense.name;

    let body = match (&request.sparse_vector, spec.sparse.as_ref()) {
        (Some(sparse), Some(sparse_cfg)) => hybrid_body(
            dense,
            dense_name,
            sparse,
            &sparse_cfg.name,
            limit,
            filter_json.as_ref(),
        ),
        _ => named_dense_body(dense, dense_name, limit, filter_json.as_ref()),
    };

    let url = http
        .endpoint()
        .collection_path(&request.collection, "points/query");
    let parsed: QdrantQueryResponse = http.post_json(stage, &url, &body, "qdrant_search").await?;

    let results = parsed.result.points.into_iter().map(hit_to_match).collect();
    Ok(VectorSearchResult {
        collection: request.collection.clone(),
        results,
        limit: request.limit,
        next_cursor: None,
        warnings: Vec::new(),
        metadata: MetadataMap::new(),
    })
}

fn hybrid_body(
    dense: &[f32],
    dense_name: &str,
    sparse: &SparseVector,
    sparse_name: &str,
    limit: usize,
    filter: Option<&serde_json::Value>,
) -> serde_json::Value {
    let candidates = DEFAULT_HYBRID_CANDIDATES.max(limit);
    let mut body = serde_json::json!({
        "prefetch": [
            {
                "query": dense,
                "using": dense_name,
                "limit": candidates,
                "params": dense_params(),
            },
            {
                "query": { "indices": sparse.indices, "values": sparse.values },
                "using": sparse_name,
                "limit": candidates,
            }
        ],
        "query": { "fusion": "rrf" },
        "limit": limit,
        "with_payload": true,
        "with_vector": false,
    });
    if let Some(filter) = filter {
        body["filter"] = filter.clone();
    }
    body
}

fn named_dense_body(
    dense: &[f32],
    dense_name: &str,
    limit: usize,
    filter: Option<&serde_json::Value>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "query": dense,
        "using": dense_name,
        "limit": limit,
        "with_payload": true,
        "with_vector": false,
        "params": dense_params(),
    });
    if let Some(filter) = filter {
        body["filter"] = filter.clone();
    }
    body
}

fn dense_params() -> serde_json::Value {
    serde_json::json!({
        "hnsw_ef": HNSW_EF_SEARCH,
        "quantization": { "rescore": true, "oversampling": 1.5 },
    })
}

fn hit_to_match(hit: QdrantSearchHit) -> VectorSearchMatch {
    let payload = MetadataMap(hit.payload.into_iter().collect());
    let point_id = point_id_string(&hit.id);
    VectorSearchMatch {
        point_id: VectorPointId::new(point_id),
        score: hit.score,
        chunk_id: payload_str(&payload, "chunk_id").map(ChunkId::new),
        document_id: payload_str(&payload, "document_id").map(DocumentId::new),
        source_id: payload_str(&payload, "source_id").map(SourceId::new),
        source_item_key: payload_str(&payload, "source_item_key").map(SourceItemKey::new),
        text: payload_str(&payload, "chunk_text"),
        payload,
    }
}

fn point_id_string(id: &serde_json::Value) -> String {
    match id {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}

fn payload_str(payload: &MetadataMap, field: &str) -> Option<String> {
    payload.get(field)?.as_str().map(ToString::to_string)
}
