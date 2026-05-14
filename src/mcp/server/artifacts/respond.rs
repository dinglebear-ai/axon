use super::super::common::internal_error;
use super::path::{artifact_handle_for_path, build_artifact_path};
use super::shape::{clip_inline_json, json_shape_preview, line_count, sha256_hex};
use crate::mcp::schema::{AxonToolResponse, ResponseMode};
use crate::vector::ops::qdrant::env_usize_clamped;
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
    let path = build_artifact_path(stem, "json").await?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| internal_error(format!("failed to create artifact directory: {e}")))?;
    }

    // Write to a sibling temp file first, then rename atomically.
    let tmp_path = path.with_extension(format!("json.{}.tmp", Uuid::new_v4().simple()));
    tokio::fs::write(&tmp_path, text.as_bytes())
        .await
        .map_err(|e| internal_error(format!("failed to write artifact temp file: {e}")))?;
    tokio::fs::rename(&tmp_path, &path).await.map_err(|e| {
        let tmp = tmp_path.clone();
        tokio::spawn(async move {
            let _ = tokio::fs::remove_file(tmp).await;
        });
        internal_error(format!("failed to finalize artifact file: {e}"))
    })?;

    let job_id = payload
        .get("job_id")
        .and_then(|value| value.as_str())
        .or_else(|| {
            payload
                .get("job")
                .and_then(|job| job.get("id"))
                .and_then(|value| value.as_str())
        })
        .map(ToString::to_string);
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
    let handle = artifact_handle_for_path(
        "json",
        &path,
        text.len() as u64,
        Some(line_count(&text) as u64),
        job_id,
        url,
    )
    .await?;
    let relative_path = handle.relative_path.clone();
    let display_path = handle.display_path.clone();
    let kind = handle.kind.clone();

    Ok(serde_json::json!({
        "artifact_handle": handle,
        "path": relative_path,
        "relative_path": relative_path,
        "display_path": display_path,
        "kind": kind,
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
        return Ok(AxonToolResponse::ok(
            action,
            subaction,
            serde_json::json!({
                "response_mode": "path",
                "shape": shape,
                "artifact_handle": artifact_handle_value(&artifact),
                "artifact": artifact,
            }),
        ));
    }

    // Fields hint: always write artifact, always extract named fields.
    if let InlineHint::Fields(fields) = &hint {
        let artifact = write_json_artifact(artifact_stem, &payload).await?;
        let key_fields = extract_key_fields(&payload, fields);
        let shape = json_shape_preview(&payload);
        return Ok(AxonToolResponse::ok(
            action,
            subaction,
            serde_json::json!({
                "response_mode": "path",
                "key_fields": key_fields,
                "shape": shape,
                "artifact_handle": artifact_handle_value(&artifact),
                "artifact": artifact,
            }),
        ));
    }

    let effective_mode = match mode {
        None if is_document => ResponseMode::Inline,
        Some(ResponseMode::AutoInline) | None => {
            let payload_bytes = serde_json::to_string(&payload)
                .map(|s| s.len())
                .unwrap_or(usize::MAX);
            let threshold = inline_bytes_threshold();
            if threshold > 0 && payload_bytes <= threshold {
                return Ok(AxonToolResponse::ok(
                    action,
                    subaction,
                    serde_json::json!({
                        "response_mode": "auto-inline",
                        "data": payload,
                    }),
                ));
            }
            // Large payload with auto mode — default to path.
            ResponseMode::Path
        }
        Some(explicit) => explicit,
    };

    let artifact = write_json_artifact(artifact_stem, &payload).await?;
    let shape = json_shape_preview(&payload);
    match effective_mode {
        ResponseMode::Path => Ok(AxonToolResponse::ok(
            action,
            subaction,
            serde_json::json!({
                "response_mode": "path",
                "shape": shape,
                "artifact_handle": artifact_handle_value(&artifact),
                "artifact": artifact,
            }),
        )),
        ResponseMode::Inline => {
            let (inline, truncated) = clip_inline_json(&payload, inline_clip_chars(is_document));
            Ok(AxonToolResponse::ok(
                action,
                subaction,
                serde_json::json!({
                    "response_mode": "inline",
                    "inline": inline,
                    "truncated": truncated,
                    "artifact_handle": artifact_handle_value(&artifact),
                    "artifact": artifact,
                }),
            ))
        }
        ResponseMode::Both => {
            let (inline, truncated) = clip_inline_json(&payload, inline_clip_chars(is_document));
            Ok(AxonToolResponse::ok(
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
            ))
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
#[path = "respond/tests.rs"]
mod tests;
