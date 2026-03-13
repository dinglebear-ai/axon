//! Cancel handler for async jobs via direct service dispatch.
//!
//! Previously this module spawned an `axon <mode> cancel <job_id> --json`
//! subprocess. Now it calls the jobs layer directly — no subprocess, no
//! binary discovery required.

use super::events::{self, JobCancelResponsePayload, WsEventV2, serialize_v2_event};
use super::ws_send::send_error_dual;
use crate::crates::core::config::Config;
use crate::crates::jobs;
use std::string::ToString;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn cancel_ok_from_output(
    parsed: Option<&serde_json::Value>,
    status_success: bool,
) -> bool {
    parsed
        .and_then(|v| v.get("ok"))
        .and_then(|v| v.as_bool())
        .or_else(|| {
            parsed
                .and_then(|v| v.get("canceled"))
                .and_then(|v| v.as_bool())
        })
        .unwrap_or(status_success)
}

pub(super) fn is_valid_cancel_job_id(job_id: &str) -> bool {
    Uuid::parse_str(job_id).is_ok()
}

/// Cancel a job via direct service call. No subprocess is spawned.
///
/// The `mode` argument selects which job table to cancel from
/// (`crawl`, `extract`, `embed`). Unknown or unsupported modes fall back to
/// `crawl` for backward compatibility with older browser clients that may not
/// set the mode field.
pub(super) async fn handle_cancel(
    mode: &str,
    job_id: &str,
    tx: mpsc::Sender<String>,
    cfg: Arc<Config>,
) {
    let cancel_mode = if mode.is_empty() { "crawl" } else { mode };

    // Validate mode against the allowlist before doing any work.
    if !super::constants::ALLOWED_MODES.contains(&cancel_mode) {
        let ws_ctx = events::CommandContext {
            exec_id: format!("exec-{}", Uuid::new_v4()),
            mode: cancel_mode.to_string(),
            input: job_id.to_string(),
        };
        send_error_dual(
            &tx,
            &ws_ctx,
            format!("cancel failed: unknown mode '{cancel_mode}'"),
            None,
        )
        .await;
        return;
    }

    let ws_ctx = events::CommandContext {
        exec_id: format!("exec-{}", Uuid::new_v4()),
        mode: cancel_mode.to_string(),
        input: job_id.to_string(),
    };

    // Validate job_id is a UUID before hitting the DB.
    let uuid = match Uuid::parse_str(job_id) {
        Ok(u) => u,
        Err(_) => {
            if let Some(v2) = serialize_v2_event(WsEventV2::JobCancelResponse {
                ctx: ws_ctx.clone(),
                payload: JobCancelResponsePayload {
                    ok: false,
                    mode: Some(cancel_mode.to_string()),
                    job_id: Some(job_id.to_string()),
                    message: Some("invalid job_id format".to_string()),
                },
            }) {
                let _ = tx.send(v2).await;
            }
            send_error_dual(
                &tx,
                &ws_ctx,
                "cancel failed: invalid job_id format".to_string(),
                None,
            )
            .await;
            return;
        }
    };

    // Dispatch cancel to the appropriate job table.
    // Map the error to String immediately so no non-Send Box<dyn Error> is held
    // across the subsequent .await points in the match arms below.
    let cancel_result: Result<bool, String> = match cancel_mode {
        "crawl" => jobs::crawl::cancel_job(&cfg, uuid)
            .await
            .map_err(|e| e.to_string()),
        "extract" => jobs::extract::cancel_extract_job(&cfg, uuid)
            .await
            .map_err(|e| e.to_string()),
        "embed" => jobs::embed::cancel_embed_job(&cfg, uuid)
            .await
            .map_err(|e| e.to_string()),
        other => Err(format!("cancel not supported for mode '{other}'")),
    };

    match cancel_result {
        Ok(ok) => {
            let message = if ok {
                Some("cancel requested".to_string())
            } else {
                Some("job not found or already terminal".to_string())
            };

            if let Some(v2) = serialize_v2_event(WsEventV2::JobCancelResponse {
                ctx: ws_ctx.clone(),
                payload: JobCancelResponsePayload {
                    ok,
                    mode: Some(cancel_mode.to_string()),
                    job_id: Some(job_id.to_string()),
                    message,
                },
            }) {
                let _ = tx.send(v2).await;
            }

            if ok {
                // L-3: job.cancel.response is the sole terminal event for
                // successful cancels — do NOT also emit command.done.
            } else {
                send_error_dual(
                    &tx,
                    &ws_ctx,
                    "cancel failed: job not found or already terminal".to_string(),
                    None,
                )
                .await;
            }
        }
        Err(err_msg) => {
            if let Some(v2) = serialize_v2_event(WsEventV2::JobCancelResponse {
                ctx: ws_ctx.clone(),
                payload: JobCancelResponsePayload {
                    ok: false,
                    mode: Some(cancel_mode.to_string()),
                    job_id: Some(job_id.to_string()),
                    message: Some(err_msg.clone()),
                },
            }) {
                let _ = tx.send(v2).await;
            }
            send_error_dual(&tx, &ws_ctx, format!("cancel failed: {err_msg}"), None).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::Config;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    // ---- cancel_ok_from_output (pure, no I/O) --------------------------------

    #[test]
    fn cancel_ok_from_output_ok_true() {
        let v = serde_json::json!({ "ok": true });
        assert!(cancel_ok_from_output(Some(&v), false));
    }

    #[test]
    fn cancel_ok_from_output_ok_false() {
        let v = serde_json::json!({ "ok": false });
        assert!(!cancel_ok_from_output(Some(&v), true));
    }

    #[test]
    fn cancel_ok_from_output_canceled_key() {
        // Falls back to "canceled" bool when "ok" is absent
        let v = serde_json::json!({ "canceled": true });
        assert!(cancel_ok_from_output(Some(&v), false));
    }

    #[test]
    fn cancel_ok_from_output_falls_back_to_status() {
        // Neither "ok" nor "canceled" key — uses status_success fallback
        let v = serde_json::json!({ "something_else": 1 });
        assert!(cancel_ok_from_output(Some(&v), true));
        assert!(!cancel_ok_from_output(Some(&v), false));
    }

    #[test]
    fn cancel_ok_from_output_none_parsed() {
        assert!(cancel_ok_from_output(None, true));
        assert!(!cancel_ok_from_output(None, false));
    }

    // ---- is_valid_cancel_job_id (pure, no I/O) --------------------------------

    #[test]
    fn valid_uuid_accepted() {
        assert!(is_valid_cancel_job_id(
            "550e8400-e29b-41d4-a716-446655440000"
        ));
    }

    #[test]
    fn invalid_uuid_rejected() {
        assert!(!is_valid_cancel_job_id("not-a-uuid"));
        assert!(!is_valid_cancel_job_id(""));
        assert!(!is_valid_cancel_job_id("12345"));
    }

    // ---- handle_cancel early-exit paths (no DB needed) -----------------------

    /// Invalid UUID: emits a JobCancelResponse with ok=false before touching the DB.
    #[tokio::test]
    async fn cancel_invalid_uuid_emits_error_event() {
        let (tx, mut rx) = mpsc::channel::<String>(16);
        let cfg = Arc::new(Config::default());

        handle_cancel("crawl", "not-a-uuid", tx, cfg).await;

        let mut messages = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }

        assert!(!messages.is_empty(), "expected at least one WS event");

        let has_cancel_error = messages.iter().any(|msg| {
            let v: serde_json::Value = serde_json::from_str(msg).unwrap_or_default();
            let is_cancel_response = v["type"] == "job.cancel.response";
            let is_error = v["type"] == "command.error";
            (is_cancel_response && v["data"]["payload"]["ok"] == false) || is_error
        });
        assert!(
            has_cancel_error,
            "expected cancel error event, got: {messages:?}"
        );
    }

    /// Empty mode falls back to "crawl". UUID validation still fires before DB.
    #[tokio::test]
    async fn cancel_empty_mode_falls_back_to_crawl_validates_uuid() {
        let (tx, mut rx) = mpsc::channel::<String>(16);
        let cfg = Arc::new(Config::default());

        // Empty mode → treated as "crawl"; still rejects invalid UUID before DB
        handle_cancel("", "not-a-uuid", tx, cfg).await;

        let mut messages = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }

        assert!(!messages.is_empty(), "expected at least one WS event");

        // The cancel response or error event should reference "crawl" as the mode
        let references_crawl = messages.iter().any(|msg| msg.contains("crawl"));
        assert!(
            references_crawl,
            "expected mode 'crawl' in output, got: {messages:?}"
        );
    }

    /// Unknown mode must emit a command.error immediately without touching the DB.
    #[tokio::test]
    async fn cancel_unknown_mode_emits_error() {
        let (tx, mut rx) = mpsc::channel::<String>(16);
        let cfg = Arc::new(Config::default());

        // "nonexistent_mode" is not in ALLOWED_MODES
        handle_cancel(
            "nonexistent_mode",
            "550e8400-e29b-41d4-a716-446655440000",
            tx,
            cfg,
        )
        .await;

        let mut messages = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }

        assert!(!messages.is_empty(), "expected at least one WS event");

        let has_mode_error = messages.iter().any(|msg| {
            let v: serde_json::Value = serde_json::from_str(msg).unwrap_or_default();
            v["type"] == "command.error"
                && msg.contains("unknown mode")
                && msg.contains("nonexistent_mode")
        });
        assert!(
            has_mode_error,
            "expected 'unknown mode' error, got: {messages:?}"
        );
    }
}
