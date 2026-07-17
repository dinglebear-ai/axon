//! Redacted artifact capture for executable tool source output.

use axon_api::source::{
    ArtifactKind, ArtifactRef, ArtifactWriteRequest, ContentRef, MetadataMap, SourceAcquisition,
    SourcePlan, Timestamp,
};
use axon_core::redact::redact_secrets;
use sha2::{Digest, Sha256};

use crate::context::TargetLocalSourceRuntime;

pub(super) async fn capture_tool_output_artifacts(
    runtime: &TargetLocalSourceRuntime,
    plan: &SourcePlan,
    acquisition: &mut SourceAcquisition,
) -> anyhow::Result<Vec<ArtifactRef>> {
    let mut artifacts = Vec::new();
    for item in &mut acquisition.fetched_items {
        let tool_action = item
            .metadata
            .0
            .get("tool_action")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("metadata")
            .to_string();
        if !matches!(tool_action.as_str(), "execute" | "call") {
            continue;
        }
        let ContentRef::InlineText { text } = &item.content_ref else {
            continue;
        };
        let safe_text = redact_secrets(text);
        let redaction_status = if safe_text != *text {
            "redacted".to_string()
        } else {
            item.metadata
                .0
                .get("redaction_status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("clean")
                .to_string()
        };
        item.content_ref = ContentRef::InlineText {
            text: safe_text.clone(),
        };
        item.metadata.insert(
            "redaction_status".to_string(),
            serde_json::json!(redaction_status),
        );
        let content_hash = sha256_hex(safe_text.as_bytes());
        let mut metadata = MetadataMap::new();
        metadata.insert(
            "source_kind".to_string(),
            serde_json::json!(plan.route.source.source_kind),
        );
        metadata.insert("tool_action".to_string(), serde_json::json!(tool_action));
        metadata.insert(
            "redaction_status".to_string(),
            serde_json::json!(redaction_status),
        );
        let handle = runtime
            .artifact_store
            .put(ArtifactWriteRequest {
                kind: ArtifactKind::RawContent,
                content_type: "text/plain; charset=utf-8".to_string(),
                content: ContentRef::InlineText {
                    text: safe_text.clone(),
                },
                source_id: Some(plan.route.source.source_id.clone()),
                job_id: Some(plan.job_id),
                metadata,
            })
            .await?;
        item.raw_artifact_id = Some(handle.artifact_id.clone());
        let uri = handle
            .uri
            .clone()
            .unwrap_or_else(|| format!("artifact://{}", handle.artifact_id.0));
        artifacts.push(ArtifactRef {
            artifact_id: handle.artifact_id,
            artifact_kind: handle.artifact_kind,
            uri,
            size_bytes: Some(safe_text.len() as u64),
            content_hash: Some(content_hash),
            created_at: timestamp(),
        });
    }
    acquisition.artifacts.extend(artifacts.clone());
    Ok(artifacts)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}
