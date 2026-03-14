use super::events::{ArtifactEntry, CommandContext, WsEventV2};
use crate::crates::core::config::Config;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Instant;
use tokio::process::Command;
use tokio::sync::mpsc;
use uuid::Uuid;

fn sample_ctx() -> CommandContext {
    CommandContext {
        exec_id: "exec-123".to_string(),
        mode: "crawl".to_string(),
        input: "https://example.com".to_string(),
    }
}

#[test]
fn command_start_serializes_v2_schema_with_ctx() {
    let event = WsEventV2::CommandStart { ctx: sample_ctx() };
    let serialized = serde_json::to_value(event).expect("event should serialize");

    assert_eq!(
        serialized.get("type").and_then(Value::as_str),
        Some("command.start")
    );
    assert_eq!(
        serialized
            .get("data")
            .and_then(|data| data.get("ctx"))
            .and_then(|ctx| ctx.get("exec_id"))
            .and_then(Value::as_str),
        Some("exec-123")
    );
    assert_eq!(
        serialized
            .get("data")
            .and_then(|data| data.get("ctx"))
            .and_then(|ctx| ctx.get("mode"))
            .and_then(Value::as_str),
        Some("crawl")
    );
    assert_eq!(
        serialized
            .get("data")
            .and_then(|data| data.get("ctx"))
            .and_then(|ctx| ctx.get("input"))
            .and_then(Value::as_str),
        Some("https://example.com")
    );
}

#[test]
fn artifact_list_serializes_v2_schema_with_artifacts_array() {
    let event = WsEventV2::ArtifactList {
        ctx: sample_ctx(),
        artifacts: vec![ArtifactEntry {
            kind: Some("screenshot".to_string()),
            path: Some("output/report.png".to_string()),
            download_url: Some("/download/job-1/file/output/report.png".to_string()),
            mime: Some("image/png".to_string()),
            size_bytes: Some(1024),
        }],
    };
    let serialized = serde_json::to_value(event).expect("event should serialize");

    assert_eq!(
        serialized.get("type").and_then(Value::as_str),
        Some("artifact.list")
    );
    assert_eq!(
        serialized
            .get("data")
            .and_then(|data| data.get("artifacts"))
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        serialized
            .get("data")
            .and_then(|data| data.get("artifacts"))
            .and_then(Value::as_array)
            .and_then(|artifacts| artifacts.first())
            .and_then(|artifact| artifact.get("kind"))
            .and_then(Value::as_str),
        Some("screenshot")
    );
    assert_eq!(
        serialized
            .get("data")
            .and_then(|data| data.get("artifacts"))
            .and_then(Value::as_array)
            .and_then(|artifacts| artifacts.first())
            .and_then(|artifact| artifact.get("size_bytes"))
            .and_then(Value::as_u64),
        Some(1024)
    );
}

#[test]
fn cancel_response_serializes_v2_schema() {
    let event = WsEventV2::JobCancelResponse {
        ctx: sample_ctx(),
        payload: super::events::JobCancelResponsePayload {
            ok: true,
            mode: Some("crawl".to_string()),
            job_id: Some("job-123".to_string()),
            message: Some("cancellation requested".to_string()),
        },
    };
    let serialized = serde_json::to_value(event).expect("event should serialize");

    assert_eq!(
        serialized.get("type").and_then(Value::as_str),
        Some("job.cancel.response")
    );
    assert_eq!(
        serialized
            .get("data")
            .and_then(|data| data.get("payload"))
            .and_then(|payload| payload.get("ok"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        serialized
            .get("data")
            .and_then(|data| data.get("payload"))
            .and_then(|payload| payload.get("job_id"))
            .and_then(Value::as_str),
        Some("job-123")
    );
}

#[tokio::test]
async fn sync_output_line_emits_v2_only() {
    let (tx, mut rx) = mpsc::channel::<String>(8);
    let ctx = sample_ctx();

    super::send_command_output_line(&tx, &ctx, "hello world".to_string());

    let first =
        serde_json::from_str::<Value>(&rx.recv().await.expect("v2 message")).expect("valid json");
    assert_eq!(
        first.get("type").and_then(Value::as_str),
        Some("command.output.line")
    );
    assert_eq!(
        first
            .get("data")
            .and_then(|data| data.get("ctx"))
            .and_then(|ctx| ctx.get("mode"))
            .and_then(Value::as_str),
        Some("crawl")
    );
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn sync_single_line_json_emits_one_structured_event() {
    let (tx, mut rx) = mpsc::channel::<String>(8);
    let context = super::ExecCommandContext {
        exec_id: "exec-dup-check".to_string(),
        mode: "query".to_string(),
        input: "test".to_string(),
        flags: Value::Null,
        cfg: Arc::new(Config::default()),
    };

    let child = Command::new("sh")
        .args(["-c", "printf '%s\n' '{\"result\":\"ok\"}'"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("subprocess should spawn");

    super::handle_sync_command(child, &context, &tx, Instant::now()).await;

    let mut json_output_events = 0usize;
    while let Ok(raw) = rx.try_recv() {
        let parsed = serde_json::from_str::<Value>(&raw).expect("valid ws event json");
        if parsed.get("type").and_then(Value::as_str) == Some("command.output.json") {
            json_output_events += 1;
        }
    }

    assert_eq!(json_output_events, 1);
}

#[tokio::test]
async fn done_emits_v2_only() {
    let (tx, mut rx) = mpsc::channel::<String>(8);
    let ctx = sample_ctx();

    super::send_done_dual(&tx, &ctx, 0, Some(123)).await;

    let first =
        serde_json::from_str::<Value>(&rx.recv().await.expect("v2 message")).expect("valid json");
    assert_eq!(
        first.get("type").and_then(Value::as_str),
        Some("command.done")
    );
    assert_eq!(
        first
            .get("data")
            .and_then(|data| data.get("payload"))
            .and_then(|payload| payload.get("exit_code"))
            .and_then(Value::as_i64),
        Some(0)
    );
    assert_eq!(
        first
            .get("data")
            .and_then(|data| data.get("payload"))
            .and_then(|payload| payload.get("elapsed_ms"))
            .and_then(Value::as_u64),
        Some(123)
    );
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn error_emits_v2_only() {
    let (tx, mut rx) = mpsc::channel::<String>(8);
    let ctx = sample_ctx();

    super::send_error_dual(&tx, &ctx, "boom".to_string(), Some(88)).await;

    let first =
        serde_json::from_str::<Value>(&rx.recv().await.expect("v2 message")).expect("valid json");
    assert_eq!(
        first.get("type").and_then(Value::as_str),
        Some("command.error")
    );
    assert_eq!(
        first
            .get("data")
            .and_then(|data| data.get("payload"))
            .and_then(|payload| payload.get("message"))
            .and_then(Value::as_str),
        Some("boom")
    );
    assert_eq!(
        first
            .get("data")
            .and_then(|data| data.get("payload"))
            .and_then(|payload| payload.get("elapsed_ms"))
            .and_then(Value::as_u64),
        Some(88)
    );
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn artifact_content_emits_v2_only_via_read_file_handler() {
    let base = std::env::temp_dir().join(format!("axon-ws-v2-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&base)
        .await
        .expect("temp dir should be created");
    tokio::fs::write(base.join("test.md"), "# hello")
        .await
        .expect("test file should be written");

    let (tx, mut rx) = mpsc::channel::<String>(8);
    super::files::handle_read_file("test.md", &base, tx).await;

    let first =
        serde_json::from_str::<Value>(&rx.recv().await.expect("v2 message")).expect("valid json");
    assert_eq!(
        first.get("type").and_then(Value::as_str),
        Some("artifact.content")
    );
    assert_eq!(
        first
            .get("data")
            .and_then(|data| data.get("content"))
            .and_then(Value::as_str),
        Some("# hello")
    );
    assert!(rx.try_recv().is_err());

    let _ = tokio::fs::remove_dir_all(&base).await;
}

#[tokio::test]
async fn artifact_list_emits_v2_only_via_screenshot_json_helper() {
    let (tx, mut rx) = mpsc::channel::<String>(8);
    let ctx = sample_ctx();
    let screenshot_jsons = vec![json!({
        "path": ".cache/axon-rust/output/screenshots/example.png",
        "size_bytes": 42,
        "url": "https://example.com"
    })];

    super::files::send_screenshot_files_from_json(&screenshot_jsons, &tx, &ctx).await;

    let first =
        serde_json::from_str::<Value>(&rx.recv().await.expect("v2 message")).expect("valid json");
    assert_eq!(
        first.get("type").and_then(Value::as_str),
        Some("artifact.list")
    );
    assert_eq!(
        first
            .get("data")
            .and_then(|data| data.get("artifacts"))
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );
    assert!(rx.try_recv().is_err());
}

// async_polling_dual_emits_legacy_and_v2_status_progress removed:
// polling module was deleted in the fire-and-forget refactor (Task 5.3).

#[test]
fn cancel_ok_from_output_accepts_legacy_canceled_field() {
    let parsed = json!({"id": "job-123", "canceled": false, "source": "rust"});
    assert!(!super::cancel_ok_from_output(Some(&parsed), true));

    let parsed = json!({"id": "job-123", "canceled": true, "source": "rust"});
    assert!(super::cancel_ok_from_output(Some(&parsed), false));
}

#[test]
fn cancel_ok_from_output_prefers_ok_field_and_falls_back_to_status() {
    let parsed = json!({"ok": false, "canceled": true});
    assert!(!super::cancel_ok_from_output(Some(&parsed), true));

    assert!(super::cancel_ok_from_output(None, true));
    assert!(!super::cancel_ok_from_output(None, false));
}

#[test]
fn cancel_job_id_validation_accepts_only_uuid() {
    assert!(super::is_valid_cancel_job_id(
        "550e8400-e29b-41d4-a716-446655440000"
    ));
    assert!(!super::is_valid_cancel_job_id("not-a-uuid"));
    assert!(!super::is_valid_cancel_job_id(""));
}

// ACP-specific WS event tests have been split into acp_ws_event_tests.rs
// to keep this file under the monolith line limit.
