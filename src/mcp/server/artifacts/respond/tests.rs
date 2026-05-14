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
    assert_eq!(
        resp.data["artifact_handle"]["relative_path"],
        resp.data["artifact"]["relative_path"]
    );
    assert!(
        !resp.data["artifact_handle"]["relative_path"]
            .as_str()
            .unwrap()
            .starts_with('/')
    );
    assert!(resp.data["artifact_handle"]["display_path"].is_string());
    assert!(resp.data["shape"].is_object());
    restore_artifact_env(prev);
}

#[tokio::test]
#[allow(unsafe_code)]
#[allow(clippy::await_holding_lock)]
async fn document_hint_defaults_to_inline_mode() {
    let _guard = ARTIFACT_ENV_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (_tmp, prev) = scoped_artifact_root();
    let payload = serde_json::json!({"content": "hello", "truncated": false});
    let resp = respond_with_mode(
        "retrieve",
        "retrieve",
        None,
        "retrieve-doc",
        payload.clone(),
        InlineHint::Document,
    )
    .await
    .unwrap();
    assert_eq!(resp.data["response_mode"], "inline");
    assert_eq!(resp.data["inline"], payload);
    restore_artifact_env(prev);
}

#[tokio::test]
#[allow(unsafe_code)]
#[allow(clippy::await_holding_lock)]
async fn explicit_auto_inline_mode_uses_threshold_auto_response() {
    let _guard = ARTIFACT_ENV_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (_tmp, prev) = scoped_artifact_root();
    let payload = serde_json::json!({"key": "value"});
    let resp = respond_with_mode(
        "test",
        "sub",
        Some(ResponseMode::AutoInline),
        "test-auto-inline-mode",
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
