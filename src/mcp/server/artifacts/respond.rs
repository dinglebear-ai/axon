use super::super::common::internal_error;
use super::path::{build_artifact_path, ensure_artifact_root};
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

    let relative_path = ensure_artifact_root()
        .await
        .ok()
        .as_ref()
        .and_then(|root| path.strip_prefix(root).ok())
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    Ok(serde_json::json!({
        "path": path,
        "relative_path": relative_path,
        "bytes": text.len(),
        "line_count": line_count(&text),
        "sha256": sha256_hex(text.as_bytes()),
    }))
}

/// Respond with the appropriate mode, respecting the caller's explicit choice.
///
/// When `mode` is `None` (caller didn't specify), small payloads are auto-inlined
/// to avoid unnecessary disk writes. When the caller explicitly requests a mode
/// (`Some(Path)`, `Some(Inline)`, `Some(Both)`), that choice is honored regardless
/// of payload size.
pub async fn respond_with_mode(
    action: &str,
    subaction: &str,
    mode: Option<ResponseMode>,
    artifact_stem: &str,
    payload: serde_json::Value,
    hint: InlineHint,
) -> Result<AxonToolResponse, ErrorData> {
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
                "artifact": artifact,
            }),
        ));
    }

    let effective_mode = match mode {
        Some(explicit) => explicit,
        None => {
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
            // Large payload with no explicit mode — default to path.
            ResponseMode::Path
        }
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
                "artifact": artifact,
            }),
        )),
        ResponseMode::Inline | ResponseMode::AutoInline => {
            let (inline, truncated) = clip_inline_json(&payload, 12_000);
            Ok(AxonToolResponse::ok(
                action,
                subaction,
                serde_json::json!({
                    "response_mode": "inline",
                    "inline": inline,
                    "truncated": truncated,
                    "artifact": artifact,
                }),
            ))
        }
        ResponseMode::Both => {
            let (inline, truncated) = clip_inline_json(&payload, 12_000);
            Ok(AxonToolResponse::ok(
                action,
                subaction,
                serde_json::json!({
                    "response_mode": "both",
                    "inline": inline,
                    "truncated": truncated,
                    "shape": shape,
                    "artifact": artifact,
                }),
            ))
        }
    }
}

fn inline_bytes_threshold() -> usize {
    env_usize_clamped("AXON_INLINE_BYTES_THRESHOLD", 8_192, 0, usize::MAX)
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
mod tests {
    use super::super::path::{ARTIFACT_ENV_TEST_LOCK, MCP_ARTIFACT_DIR_ENV};
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[allow(unsafe_code)]
    fn scoped_artifact_root() -> (TempDir, Option<String>) {
        let tmp = TempDir::new().expect("tempdir");
        let prev = env::var(MCP_ARTIFACT_DIR_ENV).ok();
        unsafe {
            env::set_var(MCP_ARTIFACT_DIR_ENV, tmp.path());
        }
        (tmp, prev)
    }

    #[allow(unsafe_code)]
    fn restore_artifact_env(prev: Option<String>) {
        unsafe {
            match prev {
                Some(val) => env::set_var(MCP_ARTIFACT_DIR_ENV, val),
                None => env::remove_var(MCP_ARTIFACT_DIR_ENV),
            }
        }
    }

    #[tokio::test]
    #[allow(unsafe_code)]
    #[allow(clippy::await_holding_lock)]
    async fn auto_inline_when_mode_is_none_and_payload_small() {
        let _guard = ARTIFACT_ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (_tmp, prev) = scoped_artifact_root();
        let payload = serde_json::json!({"key": "value"});
        let resp = respond_with_mode(
            "test",
            "sub",
            None,
            "test-artifact",
            payload.clone(),
            InlineHint::Default,
        )
        .await
        .unwrap();
        assert!(resp.ok);
        assert_eq!(resp.data["response_mode"], "auto-inline");
        assert_eq!(resp.data["data"], payload);
        restore_artifact_env(prev);
    }

    #[tokio::test]
    #[allow(unsafe_code)]
    #[allow(clippy::await_holding_lock)]
    async fn explicit_path_mode_respected_even_for_small_payload() {
        let _guard = ARTIFACT_ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (_tmp, prev) = scoped_artifact_root();
        let payload = serde_json::json!({"key": "value"});
        let resp = respond_with_mode(
            "test",
            "sub",
            Some(ResponseMode::Path),
            "test-path-mode",
            payload,
            InlineHint::Default,
        )
        .await
        .unwrap();
        assert!(resp.ok);
        assert_eq!(resp.data["response_mode"], "path");
        assert!(resp.data["artifact"].is_object());
        assert!(resp.data["shape"].is_object());
        restore_artifact_env(prev);
    }

    #[tokio::test]
    #[allow(unsafe_code)]
    #[allow(clippy::await_holding_lock)]
    async fn explicit_inline_mode_returns_inline_data() {
        let _guard = ARTIFACT_ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (_tmp, prev) = scoped_artifact_root();
        let payload = serde_json::json!({"items": [1, 2, 3]});
        let resp = respond_with_mode(
            "test",
            "sub",
            Some(ResponseMode::Inline),
            "test-inline-mode",
            payload,
            InlineHint::Default,
        )
        .await
        .unwrap();
        assert!(resp.ok);
        assert_eq!(resp.data["response_mode"], "inline");
        assert!(resp.data["inline"].is_object() || resp.data["inline"].is_array());
        assert!(resp.data.get("truncated").is_some());
        assert!(resp.data["artifact"].is_object());
        restore_artifact_env(prev);
    }

    #[tokio::test]
    #[allow(unsafe_code)]
    #[allow(clippy::await_holding_lock)]
    async fn both_mode_returns_inline_and_shape_and_artifact() {
        let _guard = ARTIFACT_ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (_tmp, prev) = scoped_artifact_root();
        let payload = serde_json::json!({"name": "axon", "count": 42});
        let resp = respond_with_mode(
            "test",
            "sub",
            Some(ResponseMode::Both),
            "test-both-mode",
            payload,
            InlineHint::Default,
        )
        .await
        .unwrap();
        assert!(resp.ok);
        assert_eq!(resp.data["response_mode"], "both");
        assert!(resp.data["inline"].is_object() || resp.data["inline"].is_array());
        assert!(resp.data.get("truncated").is_some());
        assert!(resp.data["shape"].is_object());
        assert!(resp.data["artifact"].is_object());
        restore_artifact_env(prev);
    }

    #[tokio::test]
    #[allow(unsafe_code)]
    #[allow(clippy::await_holding_lock)]
    async fn inline_hint_fields_included_in_path_mode_response() {
        let _guard = ARTIFACT_ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (_tmp, prev) = scoped_artifact_root();
        let payload = serde_json::json!({
            "query": "test question",
            "answer": "This is a detailed answer that explains everything.",
            "timing_ms": {"total": 1234},
        });
        let resp = respond_with_mode(
            "ask",
            "ask",
            None,
            "ask-test",
            payload,
            InlineHint::Fields(&["answer"]),
        )
        .await
        .unwrap();
        assert!(resp.ok);
        assert!(resp.data.get("key_fields").is_some(), "key_fields missing");
        assert!(
            resp.data["key_fields"].get("answer").is_some(),
            "answer not extracted"
        );
        assert!(resp.data.get("artifact").is_some());
        restore_artifact_env(prev);
    }

    #[tokio::test]
    #[allow(unsafe_code)]
    #[allow(clippy::await_holding_lock)]
    async fn inline_hint_always_path_overrides_explicit_inline_mode() {
        let _guard = ARTIFACT_ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (_tmp, prev) = scoped_artifact_root();
        let payload = serde_json::json!({"content": "scraped page content here"});
        let resp = respond_with_mode(
            "scrape",
            "scrape",
            Some(ResponseMode::Inline),
            "scrape-test",
            payload,
            InlineHint::AlwaysPath,
        )
        .await
        .unwrap();
        assert!(resp.ok);
        assert_eq!(resp.data["response_mode"], "path");
        assert!(
            resp.data.get("inline").is_none(),
            "inline must not be present"
        );
        restore_artifact_env(prev);
    }
}
