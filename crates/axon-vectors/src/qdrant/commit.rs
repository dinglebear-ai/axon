//! Generation-aware publish operations over the Qdrant REST API.
//!
//! Upserted points land with `committed_generation = null` (stamped by the
//! point builder) so in-flight generations stay invisible to
//! committed-generation searches until publish. `mark_generation_committed`
//! flips the matching points' `committed_generation`/`document_status` in place;
//! `mark_unchanged_items_committed` copies carried-forward points into the new
//! committed generation without mutating the previous generation's points.

use axon_api::source::*;
use serde::Deserialize;

use super::QdrantVectorStore;
use super::http::QdrantHttp;
use super::store_impl::request_usage;
use crate::payload::generation_payload_i64;
use crate::store::Result;
use crate::store_helpers::stage_header;

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
    let stage = axon_error::ErrorStage::Publishing;
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
    let stage = axon_error::ErrorStage::Publishing;
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
    let points = scroll_points(http, &collection, &filter, stage).await?;

    let mut carried = Vec::new();
    for point in points {
        let payload = point.payload;
        let item = payload.get("source_item_key").and_then(|v| v.as_str());
        if item.is_none_or(|item| !live_keys.contains(item)) {
            continue;
        }
        let mut payload = payload;
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
        let new_id = format!("{}::{}", point_id_string(&point.id), committed_generation.0);
        carried.push(serde_json::json!({
            "id": new_id,
            "vector": point.vector,
            "payload": payload,
        }));
    }

    let attempted = carried.len() as u64;
    if attempted > 0 {
        let url = http
            .endpoint()
            .collection_path(&collection, "points?wait=true");
        let body = serde_json::json!({ "points": carried });
        http.put_json(stage, &url, &body, "qdrant_mark_unchanged_items_committed")
            .await?;
    }

    Ok(VectorStoreWriteResult {
        header: stage_header(PipelinePhase::Publishing),
        collection,
        points_attempted: attempted,
        points_written: attempted,
        payload_indexes_created: Vec::new(),
        usage: request_usage(2),
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
    stage: axon_error::ErrorStage,
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

async fn scroll_points(
    http: &QdrantHttp,
    collection: &str,
    filter: &serde_json::Value,
    stage: axon_error::ErrorStage,
) -> Result<Vec<ScrollPoint>> {
    let url = http.endpoint().collection_path(collection, "points/scroll");
    let mut offset: Option<serde_json::Value> = None;
    let mut all = Vec::new();
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
        let response: ScrollResponse = http.post_json(stage, &url, &body, "qdrant_scroll").await?;
        all.extend(response.result.points);
        match response.result.next_page_offset {
            Some(next) if !next.is_null() => offset = Some(next),
            _ => break,
        }
    }
    Ok(all)
}

fn point_id_string(id: &serde_json::Value) -> String {
    match id {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}
