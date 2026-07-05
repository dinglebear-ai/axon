use std::collections::BTreeSet;

use axon_api::source::*;
use serde_json::json;

use crate::payload::generation_payload_i64;
use crate::store_helpers::{payload_string, stage_header};

use super::{FakeVectorMode, FakeVectorStore, Result};

impl FakeVectorStore {
    pub(super) async fn mark_unchanged_items_committed_inner(
        &self,
        collection: String,
        source_id: SourceId,
        previous_generation: SourceGenerationId,
        committed_generation: SourceGenerationId,
        source_item_keys: Vec<SourceItemKey>,
    ) -> Result<VectorStoreWriteResult> {
        let mut state = self.state.lock().await;
        state.calls.push("mark_unchanged_items_committed");
        if self.mode == FakeVectorMode::CommitFailure {
            return Err(ApiError::new(
                "provider.commit_failed",
                axon_error::ErrorStage::Publishing,
                "vector store failed to mark unchanged items committed",
            )
            .with_provider_id(&self.provider_id.0));
        }
        if let Some(err) = self.mode_error_for(axon_error::ErrorStage::Publishing) {
            return Err(err);
        }
        state.collection_spec(&collection, axon_error::ErrorStage::Publishing)?;
        let live_keys = source_item_keys
            .into_iter()
            .map(|key| key.0)
            .collect::<BTreeSet<_>>();
        let points = state.points.entry(collection.clone()).or_default();
        let mut carried_points = Vec::new();
        for point in points.values() {
            let point_source = payload_string(&point.payload, "source_id");
            let point_item = payload_string(&point.payload, "source_item_key");
            if point_source.as_deref() == Some(source_id.0.as_str())
                && payload_generation_matches(
                    &point.payload,
                    "committed_generation",
                    &previous_generation,
                )
                && point_item
                    .as_deref()
                    .is_some_and(|item| live_keys.contains(item))
            {
                let mut carried = point.clone();
                carried.point_id =
                    VectorPointId::new(format!("{}::{}", point.point_id.0, committed_generation.0));
                carried.payload.insert(
                    "source_generation".to_string(),
                    json!(generation_payload_i64(
                        &committed_generation,
                        "source_generation"
                    )?),
                );
                carried.payload.insert(
                    "committed_generation".to_string(),
                    json!(generation_payload_i64(
                        &committed_generation,
                        "committed_generation"
                    )?),
                );
                carried
                    .payload
                    .insert("document_status".to_string(), json!("published"));
                carried_points.push(carried);
            }
        }
        let partial_failure = self.mode == FakeVectorMode::PartialCommitFailure;
        let points_attempted = carried_points.len() as u64;
        let mut points_written = 0;
        for point in carried_points {
            points.insert(point.point_id.clone(), point);
            points_written += 1;
            if partial_failure {
                break;
            }
        }
        if partial_failure {
            return Err(ApiError::new(
                "provider.partial_commit_failure",
                axon_error::ErrorStage::Publishing,
                format!(
                    "fake vector store copied {points_written} of {points_attempted} unchanged points"
                ),
            )
            .with_provider_id(&self.provider_id.0));
        }
        Ok(VectorStoreWriteResult {
            header: stage_header(PipelinePhase::Publishing),
            collection,
            points_attempted,
            points_written,
            payload_indexes_created: Vec::new(),
            usage: ProviderUsage {
                input_tokens: None,
                output_tokens: None,
                requests: 1,
                duration_ms: 0,
            },
        })
    }
}

fn payload_generation_matches(
    payload: &MetadataMap,
    field: &str,
    generation: &SourceGenerationId,
) -> bool {
    let Ok(expected) = generation_payload_i64(generation, field) else {
        return false;
    };
    payload.get(field).and_then(serde_json::Value::as_i64) == Some(expected)
}
