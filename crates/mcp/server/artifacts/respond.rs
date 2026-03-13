use super::super::common::internal_error;
use super::path::build_artifact_path;
use super::shape::{clip_inline_json, json_shape_preview, line_count, sha256_hex};
use crate::crates::mcp::schema::{AxonToolResponse, ResponseMode};
use rmcp::ErrorData;
use uuid::Uuid;

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
    // This ensures that if the write fails, the original file (if any) is preserved.
    let tmp_path = path.with_extension(format!("json.{}.tmp", Uuid::new_v4().simple()));
    tokio::fs::write(&tmp_path, text.as_bytes())
        .await
        .map_err(|e| internal_error(format!("failed to write artifact temp file: {e}")))?;
    tokio::fs::rename(&tmp_path, &path).await.map_err(|e| {
        // Best-effort cleanup of the temp file on rename failure.
        let tmp = tmp_path.clone();
        tokio::spawn(async move {
            let _ = tokio::fs::remove_file(tmp).await;
        });
        internal_error(format!("failed to finalize artifact file: {e}"))
    })?;

    Ok(serde_json::json!({
        "path": path,
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
) -> Result<AxonToolResponse, ErrorData> {
    // Resolve the effective mode. Auto-inline only applies when the caller
    // hasn't explicitly requested a specific response mode.
    let effective_mode = match mode {
        Some(explicit) => explicit,
        None => {
            // Auto-inline: if payload serializes small, skip the artifact disk write.
            // Threshold configurable via AXON_INLINE_BYTES_THRESHOLD (default: 8192).
            // Set to 0 to disable auto-inline and force path mode for all payloads.
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
            // Large payload with no explicit mode — fall back to path.
            ResponseMode::Path
        }
    };

    // Write artifact to disk and respond according to the effective mode.
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
        ResponseMode::Inline => {
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
    std::env::var("AXON_INLINE_BYTES_THRESHOLD")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(8_192)
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

    /// Small payload with no explicit mode should auto-inline.
    #[tokio::test]
    #[allow(unsafe_code)]
    #[allow(clippy::await_holding_lock)]
    async fn auto_inline_when_mode_is_none_and_payload_small() {
        let _guard = ARTIFACT_ENV_TEST_LOCK.lock().expect("lock poisoned");
        let (_tmp, prev) = scoped_artifact_root();
        let payload = serde_json::json!({"key": "value"});
        let resp = respond_with_mode("test", "sub", None, "test-artifact", payload.clone())
            .await
            .unwrap();
        assert!(resp.ok);
        assert_eq!(resp.data["response_mode"], "auto-inline");
        assert_eq!(resp.data["data"], payload);
        restore_artifact_env(prev);
    }

    /// Explicit Path mode on a small payload should NOT auto-inline — it should
    /// write to disk and return a path response.
    #[tokio::test]
    #[allow(unsafe_code)]
    #[allow(clippy::await_holding_lock)]
    async fn explicit_path_mode_respected_even_for_small_payload() {
        let _guard = ARTIFACT_ENV_TEST_LOCK.lock().expect("lock poisoned");
        let (_tmp, prev) = scoped_artifact_root();
        let payload = serde_json::json!({"key": "value"});
        let resp = respond_with_mode(
            "test",
            "sub",
            Some(ResponseMode::Path),
            "test-path-mode",
            payload,
        )
        .await
        .unwrap();
        assert!(resp.ok);
        assert_eq!(resp.data["response_mode"], "path");
        assert!(resp.data["artifact"].is_object());
        assert!(resp.data["shape"].is_object());
        restore_artifact_env(prev);
    }

    /// Explicit Inline mode should return inline data with the artifact on disk.
    #[tokio::test]
    #[allow(unsafe_code)]
    #[allow(clippy::await_holding_lock)]
    async fn explicit_inline_mode_returns_inline_data() {
        let _guard = ARTIFACT_ENV_TEST_LOCK.lock().expect("lock poisoned");
        let (_tmp, prev) = scoped_artifact_root();
        let payload = serde_json::json!({"items": [1, 2, 3]});
        let resp = respond_with_mode(
            "test",
            "sub",
            Some(ResponseMode::Inline),
            "test-inline-mode",
            payload,
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

    /// Both mode should return inline data, shape preview, and the artifact.
    #[tokio::test]
    #[allow(unsafe_code)]
    #[allow(clippy::await_holding_lock)]
    async fn both_mode_returns_inline_and_shape_and_artifact() {
        let _guard = ARTIFACT_ENV_TEST_LOCK.lock().expect("lock poisoned");
        let (_tmp, prev) = scoped_artifact_root();
        let payload = serde_json::json!({"name": "axon", "count": 42});
        let resp = respond_with_mode(
            "test",
            "sub",
            Some(ResponseMode::Both),
            "test-both-mode",
            payload,
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
}
