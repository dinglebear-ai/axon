//! Live `VectorStore` implementation over the Qdrant REST API.

use std::sync::Arc;

use axon_api::source::*;

use super::QdrantVectorStore;
use super::convert::{
    canonical_uri_filter_json, collection_create_json, eq_filter_json, eq2_filter_json,
    payload_index_json,
};
use super::http::QdrantHttp;
use super::search::qdrant_search;
use super::upsert::upsert_batches_rest;
use crate::collection::{
    check_collection_drift, normalize_collection_spec, validate_collection_spec,
};
use crate::filter::{selector_collection, validate_delete_selector};
use crate::payload::generation_payload_i64;
use crate::store::Result;
use crate::store_helpers::delete_result;

impl QdrantVectorStore {
    pub(super) async fn ensure_collection_inner(&self, spec: CollectionSpec) -> Result<()> {
        let stage = ErrorStage::Upserting;
        let http = self.http()?;
        let spec = normalize_collection_spec(spec);
        validate_collection_spec(&spec)?;

        if let Some(existing) = self
            .fetch_collection_spec(&http, &spec.collection, stage)
            .await?
        {
            check_collection_drift(&existing, &spec)?;
            // Existing collection: still (idempotently) ensure payload indexes.
            self.ensure_payload_indexes(&http, &spec, stage).await?;
            self.cache_collection_spec(spec).await;
            return Ok(());
        }

        let url = http.endpoint().collection_path(&spec.collection, "");
        {
            let _permit = self.write_permit(stage).await?;
            http.put_json(
                stage,
                &url,
                &collection_create_json(&spec),
                "qdrant_create_collection",
            )
            .await?;
        }
        self.ensure_payload_indexes(&http, &spec, stage).await?;
        self.cache_collection_spec(spec).await;
        Ok(())
    }

    pub(super) async fn upsert_inner(
        &self,
        batch: VectorPointBatch,
    ) -> Result<VectorStoreWriteResult> {
        let stage = ErrorStage::Upserting;
        let http = self.http()?;
        let spec = self
            .require_collection_spec(&http, &batch.collection, stage)
            .await?;
        upsert_batches_rest(self, &http, &spec, batch, stage).await
    }

    pub(super) async fn delete_inner(
        &self,
        selector: VectorDeleteSelector,
    ) -> Result<VectorStoreDeleteResult> {
        let stage = ErrorStage::Cleaning;
        let http = self.http()?;
        validate_delete_selector(&selector)?;
        let collection = selector_collection(&selector).to_string();
        let _spec = self
            .require_collection_spec(&http, &collection, stage)
            .await?;
        if let VectorDeleteSelector::Collection { .. } = &selector {
            return delete_collection_points_by_scroll(self, &http, &collection, stage).await;
        }
        let body = delete_body(&selector)?;
        let matched = count_delete_matches(&http, &collection, &body, stage).await?;
        let url = http
            .endpoint()
            .collection_path(&collection, "points/delete?wait=true");
        let _permit = self.write_permit(stage).await?;
        let _ack: DeleteResponse = http.post_json(stage, &url, &body, "qdrant_delete").await?;
        Ok(qdrant_delete_result(
            collection,
            matched,
            "pre_delete_exact_match_count",
        ))
    }

    pub(super) async fn search_inner(
        &self,
        request: VectorSearchRequest,
    ) -> Result<VectorSearchResult> {
        let stage = ErrorStage::Retrieving;
        let http = self.http()?;
        let spec = self
            .require_collection_spec(&http, &request.collection, stage)
            .await?;
        qdrant_search(&http, &spec, &request).await
    }
}

impl QdrantVectorStore {
    /// Count every point currently stored in `collection` (exact server-side
    /// count, no filter). Used by `axon-prune`'s `Collection` selector to size
    /// a real, non-fabricated `estimated_deletes` without duplicating HTTP/JSON
    /// wiring outside this crate.
    pub async fn count_collection_points(
        &self,
        collection: &str,
        stage: ErrorStage,
    ) -> Result<u64> {
        let http = self.http()?;
        count_all_points(&http, collection, stage).await
    }
}

impl QdrantVectorStore {
    pub(super) async fn ensure_payload_indexes(
        &self,
        http: &QdrantHttp,
        spec: &CollectionSpec,
        stage: ErrorStage,
    ) -> Result<()> {
        let url = http
            .endpoint()
            .collection_path(&spec.collection, "index?wait=true");
        use futures_util::stream::{self, StreamExt, TryStreamExt};
        let bodies = spec
            .payload_indexes
            .iter()
            .map(payload_index_json)
            .collect::<Vec<_>>();
        let payload_index_slots = self.payload_index_slots();
        let provider_id = self.provider_id().0.clone();
        stream::iter(bodies)
            .map(|body| {
                let url = &url;
                let payload_index_slots = Arc::clone(&payload_index_slots);
                let provider_id = provider_id.clone();
                async move {
                    let _permit = payload_index_slots.acquire_owned().await.map_err(|_| {
                        ApiError::new(
                            "vector.qdrant.payload_index_admission_closed",
                            stage,
                            "Qdrant payload-index admission gate is closed",
                        )
                        .with_provider_id(provider_id)
                    })?;
                    http.put_json(stage, url, &body, "qdrant_payload_index")
                        .await
                }
            })
            .buffer_unordered(self.payload_index_parallelism)
            .try_collect::<Vec<_>>()
            .await?;
        Ok(())
    }
}

pub(super) fn request_usage(requests: u64) -> ProviderUsage {
    ProviderUsage {
        input_tokens: None,
        output_tokens: None,
        requests,
        duration_ms: 0,
    }
}

#[derive(serde::Deserialize)]
struct DeleteResponse {
    #[serde(default, rename = "result")]
    _result: Option<serde_json::Value>,
}

#[derive(serde::Deserialize)]
struct CountResult {
    #[serde(default)]
    count: u64,
}

#[derive(serde::Deserialize)]
struct CountResponse {
    result: CountResult,
}

#[derive(serde::Deserialize)]
struct RetrieveResponse {
    #[serde(default)]
    result: Vec<serde_json::Value>,
}

/// Count the exact points targeted by a Qdrant delete before issuing it.
///
/// Qdrant's synchronous delete acknowledgement does not contain a deletion
/// count. Filtered selectors use `/points/count` and explicit point-id
/// selectors use `/points` retrieval against the same request body. The count
/// is exact at observation time, but it is not transactionally coupled to the
/// subsequent delete; the receipt therefore marks it as an estimate.
async fn count_delete_matches(
    http: &QdrantHttp,
    collection: &str,
    delete_body: &serde_json::Value,
    stage: ErrorStage,
) -> Result<u64> {
    if let Some(filter) = delete_body.get("filter") {
        let url = http.endpoint().collection_path(collection, "points/count");
        let body = serde_json::json!({ "filter": filter, "exact": true });
        let count: CountResponse = http
            .post_json(stage, &url, &body, "qdrant_delete_count")
            .await?;
        return Ok(count.result.count);
    }

    if let Some(points) = delete_body.get("points") {
        let url = http.endpoint().collection_path(collection, "points");
        let body = serde_json::json!({
            "ids": points,
            "with_payload": false,
            "with_vector": false,
        });
        let retrieved: RetrieveResponse = http
            .post_json(stage, &url, &body, "qdrant_delete_retrieve")
            .await?;
        return Ok(retrieved.result.len() as u64);
    }

    Ok(0)
}

/// Count every point in `collection`, no filter (exact server-side count).
async fn count_all_points(http: &QdrantHttp, collection: &str, stage: ErrorStage) -> Result<u64> {
    let url = http.endpoint().collection_path(collection, "points/count");
    let body = serde_json::json!({ "exact": true });
    let response: CountResponse = http
        .post_json(stage, &url, &body, "qdrant_count_collection")
        .await?;
    Ok(response.result.count)
}

const COLLECTION_DELETE_SCROLL_PAGE_LIMIT: usize = 256;
const COLLECTION_DELETE_BATCH_SIZE: usize = 1000;

/// Delete every point in `collection`, keeping the collection itself.
///
/// Normal prune/delete must not recreate a collection; destructive collection
/// recreation belongs to reset receipts only. Qdrant does not expose a stable
/// match-all filter contract, so collection prune pages point ids and deletes
/// those ids in bounded batches.
async fn delete_collection_points_by_scroll(
    store: &QdrantVectorStore,
    http: &QdrantHttp,
    collection: &str,
    stage: ErrorStage,
) -> Result<VectorStoreDeleteResult> {
    let url = http
        .endpoint()
        .collection_path(collection, "points/delete?wait=true");
    let mut offset = None;
    let mut deleted = 0u64;

    loop {
        let page = store
            .scroll_page(
                collection,
                None,
                serde_json::json!(false),
                COLLECTION_DELETE_SCROLL_PAGE_LIMIT,
                offset,
            )
            .await?;
        if page.points.is_empty() {
            break;
        }

        let mut ids = Vec::with_capacity(page.points.len());
        for point in &page.points {
            if point.id.is_null() {
                return Err(ApiError::new(
                    "vector.collection_prune_missing_point_id",
                    stage,
                    "qdrant scroll returned a point without an id during collection prune",
                ));
            }
            ids.push(point.id.clone());
        }

        for batch in ids.chunks(COLLECTION_DELETE_BATCH_SIZE) {
            let body = serde_json::json!({ "points": batch });
            let _permit = store.write_permit(stage).await?;
            let _ack: DeleteResponse = http
                .post_json(stage, &url, &body, "qdrant_delete_collection_points")
                .await?;
        }
        deleted += ids.len() as u64;

        let Some(next) = page.next_offset else {
            break;
        };
        offset = Some(next);
    }

    Ok(qdrant_delete_result(
        collection.to_string(),
        deleted,
        "scrolled_point_id_count",
    ))
}

fn qdrant_delete_result(
    collection: String,
    observed_matches: u64,
    count_basis: &str,
) -> VectorStoreDeleteResult {
    let mut result = delete_result(collection, observed_matches);
    result.warnings.push(SourceWarning {
        code: "vector.qdrant_delete_count_estimated".to_string(),
        severity: Severity::Warning,
        message: "Qdrant acknowledged the delete but does not report an actual deletion count; \
                  points_deleted is the observed match count and may differ under concurrent mutation"
            .to_string(),
        source_item_key: None,
        retryable: false,
    });
    result.metadata.insert(
        "points_deleted_count_basis".to_string(),
        serde_json::Value::String(count_basis.to_string()),
    );
    result.metadata.insert(
        "points_deleted_is_estimate".to_string(),
        serde_json::Value::Bool(true),
    );
    result
}

fn generation_delete_filter(
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> Result<serde_json::Value> {
    Ok(eq2_filter_json(
        "source_id",
        &source_id.0,
        "source_generation",
        generation_payload_i64(generation, "source_generation")?,
    ))
}

fn delete_body(selector: &VectorDeleteSelector) -> Result<serde_json::Value> {
    match selector {
        VectorDeleteSelector::Points { point_ids, .. } => Ok(serde_json::json!({
            "points": point_ids.iter().map(|id| id.0.clone()).collect::<Vec<_>>()
        })),
        VectorDeleteSelector::Chunks { chunk_ids, .. } => {
            let ids = chunk_ids.iter().map(|id| id.0.clone()).collect::<Vec<_>>();
            Ok(serde_json::json!({
                "filter": {
                    "must": [{ "key": "chunk_id", "match": { "any": ids } }]
                }
            }))
        }
        VectorDeleteSelector::Source {
            source_id,
            generation,
            ..
        } => {
            let filter = match generation {
                Some(generation) => eq2_filter_json(
                    "source_id",
                    &source_id.0,
                    "source_generation",
                    generation_payload_i64(generation, "source_generation")?,
                ),
                None => eq_filter_json("source_id", &source_id.0),
            };
            Ok(serde_json::json!({ "filter": filter }))
        }
        VectorDeleteSelector::Generation {
            source_id,
            generation,
            ..
        } => Ok(serde_json::json!({
            "filter": generation_delete_filter(source_id, generation)?
        })),
        // Never reached: `delete_inner` intercepts `Collection` before calling
        // `delete_body` and routes it through bounded point-id batch deletes.
        // Kept exhaustive defensively.
        VectorDeleteSelector::Collection { .. } => Ok(serde_json::json!({})),
        VectorDeleteSelector::Document {
            document_id,
            generation,
            ..
        } => {
            let filter = match generation {
                Some(generation) => eq2_filter_json(
                    "document_id",
                    &document_id.0,
                    "source_generation",
                    generation_payload_i64(generation, "source_generation")?,
                ),
                None => eq_filter_json("document_id", &document_id.0),
            };
            Ok(serde_json::json!({ "filter": filter }))
        }
        VectorDeleteSelector::CanonicalUri {
            canonical_uri,
            match_prefix,
            ..
        } => Ok(serde_json::json!({
            "filter": canonical_uri_filter_json(canonical_uri, *match_prefix)
        })),
        VectorDeleteSelector::Filter { filter, .. } => {
            let must = filter
                .as_object()
                .map(|object| {
                    object
                        .iter()
                        .map(|(field, value)| {
                            serde_json::json!({ "key": field, "match": { "value": value } })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Ok(serde_json::json!({ "filter": { "must": must } }))
        }
    }
}

/// Interpret a Qdrant collection GET body into a [`CollectionSpec`].
///
/// Returns `None` when the body lacks a usable dense-vector config (e.g. an
/// error envelope), so callers treat it as "collection absent".
pub(super) fn detect_collection_spec(
    collection: &str,
    body: &serde_json::Value,
) -> Option<CollectionSpec> {
    let params = body.pointer("/result/config/params")?;
    let vectors = params.get("vectors")?;

    // Named-mode: {"vectors": {"<name>": {"size": N, "distance": "Cosine"}}}
    let (dense_name, dense_cfg) = if vectors.get("size").is_some() {
        ("dense".to_string(), vectors.clone())
    } else {
        let object = vectors.as_object()?;
        let (name, cfg) = object.iter().next()?;
        (name.clone(), cfg.clone())
    };
    let dimensions = dense_cfg.get("size").and_then(|v| v.as_u64())? as u32;
    let distance = dense_cfg
        .get("distance")
        .and_then(|v| v.as_str())
        .and_then(parse_distance)
        .unwrap_or(VectorDistance::Cosine);

    let sparse = params
        .get("sparse_vectors")
        .and_then(|v| v.as_object())
        .and_then(|map| map.iter().next())
        .map(|(name, cfg)| SparseVectorConfig {
            name: name.clone(),
            modifier: match cfg.get("modifier").and_then(|v| v.as_str()) {
                Some("idf") => SparseVectorModifier::Idf,
                _ => SparseVectorModifier::None,
            },
        });

    let payload_indexes = body
        .pointer("/result/payload_schema")
        .and_then(|schema| schema.as_object())
        .map(|schema| {
            schema
                .iter()
                .filter_map(|(field, cfg)| {
                    let data_type = cfg.get("data_type").and_then(|v| v.as_str())?;
                    Some(PayloadIndexSpec {
                        field_name: field.clone(),
                        field_schema: parse_field_schema(data_type),
                        required_for_filters: true,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Some(CollectionSpec {
        collection: collection.to_string(),
        dense: VectorConfig {
            name: dense_name,
            dimensions,
            distance,
        },
        payload_indexes,
        sparse,
        aliases: Vec::new(),
        distance: None,
        metadata: MetadataMap::new(),
    })
}

fn parse_field_schema(data_type: &str) -> PayloadFieldSchema {
    match data_type {
        "integer" => PayloadFieldSchema::Integer,
        "float" => PayloadFieldSchema::Float,
        "bool" => PayloadFieldSchema::Boolean,
        "datetime" => PayloadFieldSchema::Datetime,
        "text" => PayloadFieldSchema::Text,
        _ => PayloadFieldSchema::Keyword,
    }
}

fn parse_distance(value: &str) -> Option<VectorDistance> {
    match value {
        "Cosine" => Some(VectorDistance::Cosine),
        "Dot" => Some(VectorDistance::Dot),
        "Euclid" => Some(VectorDistance::Euclid),
        "Manhattan" => Some(VectorDistance::Manhattan),
        _ => None,
    }
}

#[cfg(test)]
#[path = "store_impl_tests.rs"]
mod tests;
