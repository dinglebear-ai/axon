//! Generation-aware publish operations over the Qdrant REST API.
//!
//! Upserted points land with `committed_generation = null` (stamped by the
//! point builder) so in-flight generations stay invisible to
//! committed-generation searches until publish. `mark_generation_committed`
//! flips the matching points' `committed_generation`/`document_status` in place;
//! `mark_unchanged_items_committed` copies carried-forward points into the new
//! committed generation without mutating the previous generation's points.

use std::sync::Arc;

use axon_api::source::*;
use futures_util::{StreamExt, TryStreamExt, stream};
use serde::Deserialize;

use super::QdrantVectorStore;
use super::http::QdrantHttp;
use super::store_impl::request_usage;
use crate::payload::generation_payload_i64;
use crate::store::Result;
use crate::store_helpers::{carried_point_id, stage_header};

const SCROLL_PAGE_LIMIT: u64 = 256;

/// Set `committed_generation`/`document_status` = published on every point whose
/// `source_id` + `source_generation` match, via a filtered set-payload.
pub async fn mark_generation_committed_rest(
    store: &QdrantVectorStore,
    http: &QdrantHttp,
    collection: String,
    source_id: SourceId,
    generation: SourceGenerationId,
) -> Result<VectorStoreWriteResult> {
    let stage = ErrorStage::Publishing;
    store
        .require_collection_spec(http, &collection, stage)
        .await?;

    let generation_value = generation_payload_i64(&generation, "source_generation")?;
    let filter = super::convert::eq2_filter_json(
        "source_id",
        &source_id.0,
        "source_generation",
        generation_value,
    );
    let matched = count_points(http, &collection, &filter, stage).await?;

    let body = serde_json::json!({
        "payload": {
            "committed_generation": generation_value,
            "document_status": "published",
        },
        "filter": filter,
    });
    let url = http
        .endpoint()
        .collection_path(&collection, "points/payload?wait=true");
    let _ack: SimpleAck = http
        .post_json(stage, &url, &body, "qdrant_mark_generation_committed")
        .await?;

    Ok(VectorStoreWriteResult {
        header: stage_header(PipelinePhase::Publishing),
        collection,
        points_attempted: matched,
        points_written: matched,
        payload_indexes_created: Vec::new(),
        usage: request_usage(2),
    })
}

pub async fn retire_generation_rest(
    store: &QdrantVectorStore,
    http: &QdrantHttp,
    collection: String,
    source_id: SourceId,
    generation: SourceGenerationId,
    retired_epoch: SourceGenerationId,
) -> Result<VectorStoreWriteResult> {
    let stage = ErrorStage::Publishing;
    store
        .require_collection_spec(http, &collection, stage)
        .await?;
    let generation_value = generation_payload_i64(&generation, "committed_generation")?;
    let retired_value = generation_payload_i64(&retired_epoch, "retired_epoch")?;
    let filter = super::convert::eq2_filter_json(
        "source_id",
        &source_id.0,
        "committed_generation",
        generation_value,
    );
    let matched = count_points(http, &collection, &filter, stage).await?;
    let body =
        serde_json::json!({ "payload": { "retired_epoch": retired_value }, "filter": filter });
    let url = http
        .endpoint()
        .collection_path(&collection, "points/payload?wait=true");
    let _ack: SimpleAck = http
        .post_json(stage, &url, &body, "qdrant_retire_generation")
        .await?;
    Ok(VectorStoreWriteResult {
        header: stage_header(PipelinePhase::Publishing),
        collection,
        points_attempted: matched,
        points_written: matched,
        payload_indexes_created: Vec::new(),
        usage: request_usage(2),
    })
}

/// Copy unchanged carried-forward points into the newly committed generation.
///
/// Scrolls every point whose `source_id` + `committed_generation` match the
/// previous generation and whose `source_item_key` is in `source_item_keys`,
/// then re-upserts a copy with a generation-suffixed id and the new
/// generation/status stamped — leaving the previous generation intact.
pub async fn mark_unchanged_items_committed_rest(
    store: &QdrantVectorStore,
    http: &QdrantHttp,
    collection: String,
    source_id: SourceId,
    previous_generation: SourceGenerationId,
    committed_generation: SourceGenerationId,
    source_item_keys: Vec<SourceItemKey>,
) -> Result<VectorStoreWriteResult> {
    let stage = ErrorStage::Publishing;
    store
        .require_collection_spec(http, &collection, stage)
        .await?;

    let live_keys: std::collections::BTreeSet<String> =
        source_item_keys.into_iter().map(|key| key.0).collect();
    if live_keys.is_empty() {
        return Ok(empty_commit(collection));
    }

    let previous_generation_value =
        generation_payload_i64(&previous_generation, "committed_generation")?;
    let committed_generation_value =
        generation_payload_i64(&committed_generation, "committed_generation")?;
    let filter = super::convert::eq2_filter_json(
        "source_id",
        &source_id.0,
        "committed_generation",
        previous_generation_value,
    );
    let scroll_url = http
        .endpoint()
        .collection_path(&collection, "points/scroll");
    let upsert_url = http
        .endpoint()
        .collection_path(&collection, "points?wait=true");
    let mut offset: Option<serde_json::Value> = None;
    let mut attempted = 0u64;
    let mut requests = 0u64;
    loop {
        let mut body = serde_json::json!({
            "filter": filter,
            "limit": SCROLL_PAGE_LIMIT,
            "with_payload": true,
            "with_vector": true,
        });
        if let Some(offset) = &offset {
            body["offset"] = offset.clone();
        }
        let response: ScrollResponse = http
            .post_json(stage, &scroll_url, &body, "qdrant_scroll")
            .await?;
        requests = requests.saturating_add(1);

        let next_page_offset = response.result.next_page_offset;
        let mut carried = Vec::with_capacity(response.result.points.len());
        for point in response.result.points {
            let mut payload = point.payload;
            let item = payload.get("source_item_key").and_then(|v| v.as_str());
            if item.is_none_or(|item| !live_keys.contains(item)) {
                continue;
            }
            payload.insert(
                "source_generation".to_string(),
                serde_json::Value::from(committed_generation_value),
            );
            payload.insert(
                "committed_generation".to_string(),
                serde_json::Value::from(committed_generation_value),
            );
            payload.insert(
                "document_status".to_string(),
                serde_json::Value::from("published"),
            );
            let new_id = carried_point_id(&point_id_string(&point.id), &committed_generation);
            payload.insert(
                "vector_point_id".to_string(),
                serde_json::Value::from(new_id.0.clone()),
            );
            carried.push(serde_json::json!({
                "id": new_id.0,
                "vector": point.vector,
                "payload": payload,
            }));
        }

        attempted = attempted.saturating_add(carried.len() as u64);
        if !carried.is_empty() {
            let bodies = carried_upsert_chunks(carried, store.point_buffer())
                .map(|points| serde_json::json!({ "points": points }))
                .collect::<Vec<_>>();
            requests = requests.saturating_add(bodies.len() as u64);
            let write_slots = store.write_slots();
            let provider_id = store.provider_id().0.clone();
            stream::iter(bodies)
                .map(|body| {
                    let write_slots = Arc::clone(&write_slots);
                    let provider_id = provider_id.clone();
                    let upsert_url = &upsert_url;
                    async move {
                        let _permit = write_slots.acquire_owned().await.map_err(|_| {
                            ApiError::new(
                                "vector.qdrant.write_admission_closed",
                                stage,
                                "Qdrant write admission gate is closed",
                            )
                            .with_provider_id(provider_id)
                        })?;
                        http.put_json(
                            stage,
                            upsert_url,
                            &body,
                            "qdrant_mark_unchanged_items_committed",
                        )
                        .await
                    }
                })
                .buffer_unordered(store.write_parallelism())
                .try_collect::<Vec<_>>()
                .await?;
        }

        match next_page_offset {
            Some(next) if !next.is_null() => offset = Some(next),
            _ => break,
        }
    }

    Ok(VectorStoreWriteResult {
        header: stage_header(PipelinePhase::Publishing),
        collection,
        points_attempted: attempted,
        points_written: attempted,
        payload_indexes_created: Vec::new(),
        usage: request_usage(requests),
    })
}

fn empty_commit(collection: String) -> VectorStoreWriteResult {
    VectorStoreWriteResult {
        header: stage_header(PipelinePhase::Publishing),
        collection,
        points_attempted: 0,
        points_written: 0,
        payload_indexes_created: Vec::new(),
        usage: request_usage(1),
    }
}

#[derive(Deserialize)]
struct SimpleAck {
    #[serde(default, rename = "result")]
    _result: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct CountResponse {
    result: CountResult,
}

#[derive(Deserialize)]
struct CountResult {
    #[serde(default)]
    count: u64,
}

async fn count_points(
    http: &QdrantHttp,
    collection: &str,
    filter: &serde_json::Value,
    stage: ErrorStage,
) -> Result<u64> {
    let url = http.endpoint().collection_path(collection, "points/count");
    let body = serde_json::json!({ "filter": filter, "exact": true });
    let response: CountResponse = http.post_json(stage, &url, &body, "qdrant_count").await?;
    Ok(response.result.count)
}

#[derive(Deserialize)]
struct ScrollPoint {
    id: serde_json::Value,
    #[serde(default)]
    vector: serde_json::Value,
    #[serde(default)]
    payload: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct ScrollResult {
    #[serde(default)]
    points: Vec<ScrollPoint>,
    #[serde(default)]
    next_page_offset: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ScrollResponse {
    result: ScrollResult,
}

fn point_id_string(id: &serde_json::Value) -> String {
    match id {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}

fn carried_upsert_chunks(
    points: Vec<serde_json::Value>,
    point_buffer: usize,
) -> impl Iterator<Item = Vec<serde_json::Value>> {
    let mut points = points.into_iter();
    let chunk_size = point_buffer.max(1);
    std::iter::from_fn(move || {
        let chunk = points.by_ref().take(chunk_size).collect::<Vec<_>>();
        (!chunk.is_empty()).then_some(chunk)
    })
}
