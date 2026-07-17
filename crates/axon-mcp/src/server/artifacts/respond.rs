use super::super::common::internal_error;
use super::path::ensure_artifact_root;
use super::shape::{clip_inline_json, json_shape_preview, line_count, sha256_hex};
use crate::schema::{AxonToolResponse, ResponseMode};
use axon_api::source::{ArtifactKind, JobId, MetadataMap};
use axon_core::boundary::{ArtifactBytesWriteRequest, ArtifactStore, FileArtifactStore};
use axon_core::env::env_usize_clamped;
use rmcp::ErrorData;
use uuid::Uuid;

/// Controls which fields are always surfaced inline in the MCP response,
/// regardless of response_mode or payload size.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields + AlwaysPath wired in Task 5
pub enum InlineHint {
    /// Normal auto-inline behavior based on payload size.
    Default,
    /// Document reads default to inline mode and use a larger clip budget.
    Document,
    /// Extract these top-level fields into `key_fields` in the response.
    /// The full payload is still written to the artifact.
    Fields(&'static [&'static str]),
    /// Never inline. Force path mode regardless of the caller's explicit mode.
    /// Use for large document content (scrape, retrieve).
    AlwaysPath,
}

pub async fn write_json_artifact(
    stem: &str,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, ErrorData> {
    let text = serde_json::to_string_pretty(payload).map_err(|e| internal_error(e.to_string()))?;
    let job_id = payload
        .get("job_id")
        .and_then(|value| value.as_str())
        .or_else(|| {
            payload
                .get("job")
                .and_then(|job| job.get("id"))
                .and_then(|value| value.as_str())
        })
        .and_then(|value| match Uuid::parse_str(value) {
            Ok(id) => Some(id),
            Err(_) => {
                // A non-UUID job id would silently unlink this response
                // artifact from `artifacts list --job-id`; make that drift
                // diagnosable.
                tracing::debug!(job_id = %value, "response artifact payload job_id is not a UUID; dropping job linkage");
                None
            }
        })
        .map(JobId::new);
    let url = payload
        .get("url")
        .and_then(|value| value.as_str())
        .or_else(|| {
            payload
                .get("job")
                .and_then(|job| job.get("url"))
                .and_then(|value| value.as_str())
        })
        .map(ToString::to_string);
    let mut metadata = MetadataMap::new();
    metadata.insert("label".to_string(), format!("{stem}.json").into());
    metadata.insert("producer".to_string(), "mcp".into());
    metadata.insert("line_count".to_string(), line_count(&text).into());
    if let Some(url) = url {
        metadata.insert("source_url".to_string(), url.into());
    }
    let handle = FileArtifactStore::new(ensure_artifact_root().await?)
        .put_bytes(ArtifactBytesWriteRequest {
            kind: ArtifactKind::Report,
            content_type: "application/json".to_string(),
            bytes: text.as_bytes().to_vec(),
            source_id: None,
            job_id,
            metadata,
        })
        .await
        .map_err(|error| internal_error(error.to_string()))?;
    let artifact_id = handle.artifact_id.0;
    let artifact_handle = serde_json::json!({
        "artifact_id": artifact_id,
        "artifact_kind": "report",
    });

    Ok(serde_json::json!({
        "artifact_handle": artifact_handle,
        "artifact_id": artifact_id,
        "artifact_kind": "report",
        "bytes": text.len(),
        "line_count": line_count(&text),
        "sha256": sha256_hex(text.as_bytes()),
    }))
}

fn artifact_handle_value(artifact: &serde_json::Value) -> serde_json::Value {
    artifact
        .get("artifact_handle")
        .cloned()
        .unwrap_or(serde_json::Value::Null)
}

/// Best-effort job descriptor extraction from an already-built response
/// payload, for the MCP envelope's additive `job` field (U2-27). Mirrors the
/// `job_id`/`job.id` lookup `write_json_artifact` already does for artifact
/// bookkeeping — this does not invent job data, it only surfaces what the
/// handler already put in `payload`.
fn job_ref_from_payload(payload: &serde_json::Value) -> Option<serde_json::Value> {
    let job_id = payload
        .get("job_id")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            payload
                .get("job")
                .and_then(|job| job.get("id"))
                .and_then(serde_json::Value::as_str)
        })?;
    Some(serde_json::json!({ "id": job_id }))
}

/// Attach the additive envelope fields (U2-27) that are derivable from data
/// the handler already produced: the written artifact reference (when one
/// exists) and a `job` descriptor (when `payload` carries a job id).
fn with_envelope_extras(
    response: AxonToolResponse,
    artifact: Option<&serde_json::Value>,
    payload: &serde_json::Value,
) -> AxonToolResponse {
    let response = match artifact {
        Some(artifact) => response.with_artifact(artifact_handle_value(artifact)),
        None => response,
    };
    match job_ref_from_payload(payload) {
        Some(job) => response.with_job(job),
        None => response,
    }
}

/// Respond with the appropriate mode, respecting the caller's explicit choice.
///
/// When `mode` is `None` or `Some(AutoInline)`, small payloads are auto-inlined
/// to avoid unnecessary disk writes. Explicit `Path`, `Inline`, and `Both`
/// choices are honored regardless of payload size.
pub async fn respond_with_mode(
    action: &str,
    subaction: &str,
    mode: Option<ResponseMode>,
    artifact_stem: &str,
    payload: serde_json::Value,
    hint: InlineHint,
) -> Result<AxonToolResponse, ErrorData> {
    let is_document = matches!(hint, InlineHint::Document);
    // AlwaysPath overrides everything.
    if matches!(hint, InlineHint::AlwaysPath) {
        let artifact = write_json_artifact(artifact_stem, &payload).await?;
        let shape = json_shape_preview(&payload);
        let response = AxonToolResponse::ok(
            action,
            subaction,
            serde_json::json!({
                "response_mode": "path",
                "shape": shape,
                "artifact_handle": artifact_handle_value(&artifact),
                "artifact": artifact,
            }),
        );
        return Ok(with_envelope_extras(response, Some(&artifact), &payload));
    }

    // Fields hint: always write artifact, always extract named fields.
    if let InlineHint::Fields(fields) = &hint {
        let artifact = write_json_artifact(artifact_stem, &payload).await?;
        let key_fields = extract_key_fields(&payload, fields);
        let shape = json_shape_preview(&payload);
        let response = AxonToolResponse::ok(
            action,
            subaction,
            serde_json::json!({
                "response_mode": "path",
                "key_fields": key_fields,
                "shape": shape,
                "artifact_handle": artifact_handle_value(&artifact),
                "artifact": artifact,
            }),
        );
        return Ok(with_envelope_extras(response, Some(&artifact), &payload));
    }

    let effective_mode = match mode {
        None if is_document => ResponseMode::Inline,
        Some(ResponseMode::AutoInline) | None => {
            let payload_bytes = serde_json::to_string(&payload)
                .map(|s| s.len())
                .unwrap_or(usize::MAX);
            let threshold = inline_bytes_threshold();
            if threshold > 0 && payload_bytes <= threshold {
                let response = AxonToolResponse::ok(
                    action,
                    subaction,
                    serde_json::json!({
                        "response_mode": "auto-inline",
                        "data": payload,
                    }),
                );
                return Ok(with_envelope_extras(response, None, &payload));
            }
            // Large payload with auto mode — default to path.
            ResponseMode::Path
        }
        Some(explicit) => explicit,
    };

    let artifact = write_json_artifact(artifact_stem, &payload).await?;
    let shape = json_shape_preview(&payload);
    match effective_mode {
        ResponseMode::Path => {
            let response = AxonToolResponse::ok(
                action,
                subaction,
                serde_json::json!({
                    "response_mode": "path",
                    "shape": shape,
                    "artifact_handle": artifact_handle_value(&artifact),
                    "artifact": artifact,
                }),
            );
            Ok(with_envelope_extras(response, Some(&artifact), &payload))
        }
        ResponseMode::Inline => {
            let (inline, truncated) = clip_inline_json(&payload, inline_clip_chars(is_document));
            let response = AxonToolResponse::ok(
                action,
                subaction,
                serde_json::json!({
                    "response_mode": "inline",
                    "inline": inline,
                    "truncated": truncated,
                    "artifact_handle": artifact_handle_value(&artifact),
                    "artifact": artifact,
                }),
            );
            Ok(with_envelope_extras(response, Some(&artifact), &payload))
        }
        ResponseMode::Both => {
            let (inline, truncated) = clip_inline_json(&payload, inline_clip_chars(is_document));
            let response = AxonToolResponse::ok(
                action,
                subaction,
                serde_json::json!({
                    "response_mode": "both",
                    "inline": inline,
                    "truncated": truncated,
                    "shape": shape,
                    "artifact_handle": artifact_handle_value(&artifact),
                    "artifact": artifact,
                }),
            );
            Ok(with_envelope_extras(response, Some(&artifact), &payload))
        }
        ResponseMode::AutoInline => unreachable!("auto-inline is normalized before matching"),
    }
}

fn inline_bytes_threshold() -> usize {
    env_usize_clamped("AXON_INLINE_BYTES_THRESHOLD", 8_192, 0, usize::MAX)
}

fn inline_clip_chars(is_document: bool) -> usize {
    if is_document { 60_000 } else { 12_000 }
}

/// Extract named top-level fields from `payload` into a new object.
/// String values are capped at 32 000 chars to prevent abuse.
/// Missing keys are silently omitted.
fn extract_key_fields(payload: &serde_json::Value, fields: &[&'static str]) -> serde_json::Value {
    const STRING_CAP: usize = 32_000;
    let mut out = serde_json::Map::new();
    if let serde_json::Value::Object(map) = payload {
        for &field in fields {
            if let Some(v) = map.get(field) {
                let capped = match v {
                    serde_json::Value::String(s) if s.chars().count() > STRING_CAP => {
                        let head: String = s.chars().take(STRING_CAP).collect();
                        serde_json::Value::String(head)
                    }
                    other => other.clone(),
                };
                out.insert(field.to_string(), capped);
            }
        }
    }
    serde_json::Value::Object(out)
}

#[cfg(test)]
#[path = "respond_tests.rs"]
mod tests;
