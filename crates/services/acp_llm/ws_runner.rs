//! WebSocket-backed ACP completion runner for remote deployments.
//!
//! When `AXON_ACP_WS_URL` is configured, `complete_text` / `complete_streaming`
//! in `acp_llm` delegate here instead of spawning a local subprocess.
//!
//! Protocol: connects to `{acp_ws_url}/ws?token={token}`, sends a
//! `pulse_chat` execute message, reads ACP bridge events until `command.done`.

use std::error::Error as StdError;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::crates::core::config::Config;

use super::runner::compose_prompt;
use super::types::{AcpCompletionRequest, AcpCompletionRunner, AcpCompletionTurnResult};

const WS_COMPLETION_TIMEOUT_SECS: u64 = 300;

// ── Public runner ─────────────────────────────────────────────────────────────

pub(super) struct AcpWsCompletionRunner {
    ws_url: String,
}

impl AcpWsCompletionRunner {
    pub(super) fn from_config(cfg: &Config) -> Result<Self, Box<dyn StdError>> {
        let base = cfg
            .acp_ws_url
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or("AXON_ACP_WS_URL is required for WS-mode ACP completions")?;
        let token = cfg.acp_ws_token.as_deref();
        Ok(Self {
            ws_url: build_ws_endpoint(base, token),
        })
    }
}

#[async_trait::async_trait(?Send)]
impl AcpCompletionRunner for AcpWsCompletionRunner {
    async fn complete_text(
        &self,
        req: AcpCompletionRequest,
    ) -> Result<AcpCompletionTurnResult, Box<dyn StdError>> {
        run_ws_completion(&self.ws_url, req, &mut |_| Ok(())).await
    }

    async fn complete_streaming<F>(
        &self,
        req: AcpCompletionRequest,
        on_delta: &mut F,
    ) -> Result<AcpCompletionTurnResult, Box<dyn StdError>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
    {
        run_ws_completion(&self.ws_url, req, on_delta).await
    }
}

// ── Core WS execution ─────────────────────────────────────────────────────────

async fn run_ws_completion<F>(
    ws_url: &str,
    req: AcpCompletionRequest,
    on_delta: &mut F,
) -> Result<AcpCompletionTurnResult, Box<dyn StdError>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
{
    let (ws_stream, _) = connect_async(ws_url)
        .await
        .map_err(|e| format!("ACP WS connect failed ({ws_url}): {e}"))?;
    let (mut write, mut read) = ws_stream.split();

    let req_id = Uuid::new_v4().to_string();
    let prompt = compose_prompt(&req);
    let execute_msg = compose_execute_msg(&prompt, req.model.as_deref(), &req_id);
    write
        .send(Message::Text(execute_msg.into()))
        .await
        .map_err(|e| format!("ACP WS send failed: {e}"))?;

    let mut result_text: Option<String> = None;

    let loop_result = tokio::time::timeout(
        tokio::time::Duration::from_secs(WS_COMPLETION_TIMEOUT_SECS),
        async {
            while let Some(msg) = read.next().await {
                let text = match msg {
                    Ok(Message::Text(t)) => t.to_string(),
                    Ok(Message::Close(_)) => break,
                    Ok(_) => continue,
                    Err(e) => return Err(format!("ACP WS read error: {e}")),
                };
                match extract_event(&text) {
                    WsIncomingEvent::Delta(delta) => {
                        on_delta(&delta).map_err(|e| e.to_string())?;
                    }
                    WsIncomingEvent::Result(text) => {
                        result_text = Some(text);
                    }
                    // Server invariant: `result` is always emitted before `done`.
                    // `Done` without a prior `Result` will fall through to the
                    // `result_text.ok_or_else(...)` error below.
                    WsIncomingEvent::Done => break,
                    WsIncomingEvent::Error(msg) => {
                        return Err(format!("ACP WS server error: {msg}"));
                    }
                    WsIncomingEvent::Ignore => {}
                }
            }
            Ok(())
        },
    )
    .await;

    match loop_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e.into()),
        Err(_) => {
            return Err(
                format!("ACP WS completion timed out after {WS_COMPLETION_TIMEOUT_SECS}s").into(),
            );
        }
    }

    result_text
        .map(|text| AcpCompletionTurnResult { text, usage: None })
        .ok_or_else(|| "ACP WS completion finished without a turn result".into())
}

// ── Wire helpers ──────────────────────────────────────────────────────────────

/// Percent-encode a string for safe use as a URL query parameter value.
///
/// Encodes all bytes that are not unreserved URI characters (A-Z a-z 0-9 - _ . ~).
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b => {
                out.push('%');
                out.push(
                    char::from_digit((b >> 4) as u32, 16)
                        .unwrap()
                        .to_ascii_uppercase(),
                );
                out.push(
                    char::from_digit((b & 0xf) as u32, 16)
                        .unwrap()
                        .to_ascii_uppercase(),
                );
            }
        }
    }
    out
}

/// Build the full WebSocket endpoint URL from a base URL and optional token.
///
/// Normalises http→ws, https→wss. Appends `/ws`. Appends `?token=` when provided.
/// The token is percent-encoded so characters like `&`, `=`, `#` do not break the URL.
pub(super) fn build_ws_endpoint(base: &str, token: Option<&str>) -> String {
    let trimmed = base.trim_end_matches('/');
    let ws_base = if trimmed.starts_with("https://") {
        trimmed.replacen("https://", "wss://", 1)
    } else if trimmed.starts_with("http://") {
        trimmed.replacen("http://", "ws://", 1)
    } else {
        trimmed.to_string()
    };
    let endpoint = format!("{ws_base}/ws");
    match token.filter(|t| !t.is_empty()) {
        Some(tok) => {
            let encoded = percent_encode(tok);
            format!("{endpoint}?token={encoded}")
        }
        None => endpoint,
    }
}

/// Serialize the `execute` message for a `pulse_chat` turn.
pub(super) fn compose_execute_msg(input: &str, model: Option<&str>, id: &str) -> String {
    let flags: Value = match model.filter(|m| !m.is_empty()) {
        Some(m) => serde_json::json!({ "model": m }),
        None => serde_json::json!({}),
    };
    serde_json::json!({
        "type": "execute",
        "mode": "pulse_chat",
        "input": input,
        "id": id,
        "flags": flags,
    })
    .to_string()
}

/// Classify an incoming WS text frame.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum WsIncomingEvent {
    Delta(String),
    Result(String),
    Done,
    Error(String),
    Ignore,
}

/// Parse a raw WS text frame into a [`WsIncomingEvent`].
///
/// Uses `serde_json::Value` to avoid coupling to private server types.
pub(super) fn extract_event(raw: &str) -> WsIncomingEvent {
    let Ok(v) = serde_json::from_str::<Value>(raw) else {
        return WsIncomingEvent::Ignore;
    };
    match v["type"].as_str() {
        Some("command.output.json") => {
            let inner = &v["data"]["data"];
            match inner["type"].as_str() {
                Some("assistant_delta") => inner["delta"]
                    .as_str()
                    .map(|s| WsIncomingEvent::Delta(s.to_string()))
                    .unwrap_or(WsIncomingEvent::Ignore),
                Some("result") => inner["result"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(|s| WsIncomingEvent::Result(s.to_string()))
                    .unwrap_or(WsIncomingEvent::Ignore),
                _ => WsIncomingEvent::Ignore,
            }
        }
        Some("command.done") => WsIncomingEvent::Done,
        Some("command.error") => {
            let msg = v["data"]["payload"]["message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string();
            WsIncomingEvent::Error(msg)
        }
        _ => WsIncomingEvent::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_http_url_to_ws_endpoint() {
        assert_eq!(
            build_ws_endpoint("http://server:49000", None),
            "ws://server:49000/ws"
        );
    }

    #[test]
    fn normalise_https_url_to_wss_endpoint() {
        assert_eq!(
            build_ws_endpoint("https://server:49000", None),
            "wss://server:49000/ws"
        );
    }

    #[test]
    fn normalise_ws_url_passthrough() {
        assert_eq!(
            build_ws_endpoint("ws://server:49000", None),
            "ws://server:49000/ws"
        );
    }

    #[test]
    fn trailing_slash_stripped() {
        assert_eq!(
            build_ws_endpoint("http://server:49000/", None),
            "ws://server:49000/ws"
        );
    }

    #[test]
    fn token_appended_as_query_param() {
        assert_eq!(
            build_ws_endpoint("http://server:49000", Some("tok123")),
            "ws://server:49000/ws?token=tok123"
        );
    }

    #[test]
    fn token_with_special_chars_is_percent_encoded() {
        let result = build_ws_endpoint("http://server:49000", Some("tok&evil=1"));
        assert!(
            result.contains("tok%26evil%3D1"),
            "special chars must be encoded: {result}"
        );
        assert!(
            !result.contains("tok&evil"),
            "raw & must not appear in URL: {result}"
        );
    }

    #[test]
    fn compose_execute_message_includes_prompt_and_id() {
        let msg = compose_execute_msg("hello world", None, "req-1");
        let v: Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(v["type"], "execute");
        assert_eq!(v["mode"], "pulse_chat");
        assert_eq!(v["input"], "hello world");
        assert_eq!(v["id"], "req-1");
    }

    #[test]
    fn compose_execute_message_includes_model_in_flags() {
        let msg = compose_execute_msg("hello", Some("claude-opus-4-5"), "req-2");
        let v: Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(v["flags"]["model"], "claude-opus-4-5");
    }

    #[test]
    fn extract_delta_from_output_json_event() {
        let raw = r#"{"type":"command.output.json","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"data":{"type":"assistant_delta","session_id":"s1","delta":"Hello","tool_call_id":null}}}"#;
        assert_eq!(
            extract_event(raw),
            WsIncomingEvent::Delta("Hello".to_string())
        );
    }

    #[test]
    fn extract_result_from_output_json_event() {
        let raw = r#"{"type":"command.output.json","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"data":{"type":"result","session_id":"s1","stop_reason":"end_turn","result":"Final answer"}}}"#;
        assert_eq!(
            extract_event(raw),
            WsIncomingEvent::Result("Final answer".to_string())
        );
    }

    #[test]
    fn extract_done_from_command_done_event() {
        let raw = r#"{"type":"command.done","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"payload":{"exit_code":0}}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Done);
    }

    #[test]
    fn extract_error_from_command_error_event() {
        let raw = r#"{"type":"command.error","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"payload":{"message":"oops"}}}"#;
        assert_eq!(
            extract_event(raw),
            WsIncomingEvent::Error("oops".to_string())
        );
    }

    #[test]
    fn empty_delta_is_forwarded_as_delta_event() {
        let raw = r#"{"type":"command.output.json","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"data":{"type":"assistant_delta","session_id":"s1","delta":"","tool_call_id":null}}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Delta(String::new()));
    }

    #[test]
    fn empty_result_is_ignored() {
        let raw = r#"{"type":"command.output.json","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"data":{"type":"result","session_id":"s1","stop_reason":"end_turn","result":""}}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Ignore);
    }

    #[test]
    fn unknown_event_type_is_ignored() {
        let raw = r#"{"type":"stats","data":{}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Ignore);
    }
}
