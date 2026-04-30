//! WebSocket-backed ACP completion runner for remote deployments.
//!
//! When `AXON_ACP_WS_URL` is configured, `complete_text` / `complete_streaming`
//! in `acp_llm` delegate here instead of spawning a local subprocess.
//!
//! Protocol: connects to `{acp_ws_url}/ws?token={token}`, sends a
//! `pulse_chat` execute message, reads ACP bridge events until `command.done`.
//!
//! Reliability:
//! - Exponential backoff with ±20% jitter on connect failure (1s → 60s max).
//! - Client-side ping every 30 s; closes and retries if pong not received in 10 s.

use std::error::Error as StdError;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::crates::core::config::Config;

use super::runner::compose_prompt;
use super::types::{AcpCompletionRequest, AcpCompletionRunner, AcpCompletionTurnResult};

const WS_COMPLETION_TIMEOUT_SECS: u64 = 300;
/// Ping interval — send a Ping frame every N seconds.
const WS_PING_INTERVAL_SECS: u64 = 30;
/// Pong deadline — if no Pong received within this window after a Ping, close.
const WS_PONG_TIMEOUT_SECS: u64 = 10;
/// Initial reconnect backoff in milliseconds.
const BACKOFF_INITIAL_MS: u64 = 1_000;
/// Maximum reconnect backoff in milliseconds.
const BACKOFF_MAX_MS: u64 = 60_000;
/// Maximum number of connect attempts before propagating the error.
const BACKOFF_MAX_ATTEMPTS: u32 = 5;

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
            .ok_or_else(|| {
                tracing::error!(
                    "acp_llm: AXON_ACP_WS_URL is not configured — WS-mode ACP completions will fail"
                );
                "AXON_ACP_WS_URL is required for WS-mode ACP completions"
            })?;
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
        run_ws_completion_with_retry(&self.ws_url, req, &mut |_| Ok(())).await
    }

    async fn complete_streaming<F>(
        &self,
        req: AcpCompletionRequest,
        on_delta: &mut F,
    ) -> Result<AcpCompletionTurnResult, Box<dyn StdError>>
    where
        F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
    {
        run_ws_completion_with_retry(&self.ws_url, req, on_delta).await
    }
}

// ── Exponential backoff retry wrapper ────────────────────────────────────────

/// Run `run_ws_completion` with exponential backoff + ±20% jitter on connect
/// failures.  Backs off from [`BACKOFF_INITIAL_MS`] up to [`BACKOFF_MAX_MS`],
/// giving up after [`BACKOFF_MAX_ATTEMPTS`] consecutive failures.
///
/// A successful completion after a previous failure is logged at INFO so
/// operators know the connection was re-established.
async fn run_ws_completion_with_retry<F>(
    ws_url: &str,
    req: AcpCompletionRequest,
    on_delta: &mut F,
) -> Result<AcpCompletionTurnResult, Box<dyn StdError>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
{
    let mut attempt: u32 = 0;
    let mut backoff_ms: u64 = BACKOFF_INITIAL_MS;

    loop {
        attempt += 1;
        match run_ws_completion(ws_url, req.clone(), on_delta).await {
            Ok(result) => {
                if attempt > 1 {
                    tracing::info!(
                        ws_url = %ws_url,
                        attempt,
                        "acp_llm: WS connection re-established after failure",
                    );
                }
                return Ok(result);
            }
            Err(e) if attempt >= BACKOFF_MAX_ATTEMPTS => {
                tracing::error!(
                    ws_url = %ws_url,
                    attempt,
                    max_attempts = BACKOFF_MAX_ATTEMPTS,
                    error = %e,
                    "acp_llm: WS connect failed — exhausted retries",
                );
                return Err(e);
            }
            Err(e) => {
                // ±20% jitter: multiply backoff by a factor in [0.8, 1.2).
                let jitter = 0.8 + (rand::random::<f64>() * 0.4);
                let delay_ms = ((backoff_ms as f64) * jitter) as u64;
                tracing::info!(
                    ws_url = %ws_url,
                    attempt,
                    backoff_ms = delay_ms,
                    error = %e,
                    "reconnecting to master",
                );
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                backoff_ms = (backoff_ms * 2).min(BACKOFF_MAX_MS);
            }
        }
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
    tracing::debug!(ws_url = %ws_url, model = ?req.model, "acp_llm: WS completion connecting");
    let (ws_stream, _) = connect_async(ws_url).await.map_err(|e| {
        tracing::error!(ws_url = %ws_url, error = %e, "acp_llm: WS connect failed");
        format!("ACP WS connect failed ({ws_url}): {e}")
    })?;
    let (mut write, mut read) = ws_stream.split();

    let req_id = Uuid::new_v4().to_string();
    let prompt = compose_prompt(&req);
    let execute_msg = compose_execute_msg(&prompt, req.model.as_deref(), &req_id);
    write
        .send(Message::Text(execute_msg.into()))
        .await
        .map_err(|e| format!("ACP WS send failed: {e}"))?;

    let mut result_text: Option<String> = None;

    // ── Heartbeat state ──────────────────────────────────────────────────────
    // `last_pong` tracks the last time we received a Pong (or connection start).
    // `ping_due` fires every WS_PING_INTERVAL_SECS via a tokio::time::interval.
    let last_pong: Arc<Mutex<Instant>> = Arc::new(Mutex::new(Instant::now()));
    let mut ping_interval = tokio::time::interval(Duration::from_secs(WS_PING_INTERVAL_SECS));
    ping_interval.tick().await; // consume the immediate first tick

    let loop_result: Result<(), String> = tokio::time::timeout(
        Duration::from_secs(WS_COMPLETION_TIMEOUT_SECS),
        async {
            loop {
                tokio::select! {
                    biased;

                    // ── Incoming WS frame ────────────────────────────────────
                    msg_opt = read.next() => {
                        let text = match msg_opt {
                            Some(Ok(Message::Text(t))) => t.to_string(),
                            Some(Ok(Message::Pong(_))) => {
                                tracing::debug!(action = "ws.pong.received");
                                *last_pong.lock().await = Instant::now();
                                continue;
                            }
                            Some(Ok(Message::Close(_))) => break,
                            Some(Ok(_)) => continue,
                            Some(Err(e)) => {
                                tracing::warn!(ws_url = %ws_url, error = %e, "acp_llm: WS read error");
                                return Err(format!("ACP WS read error: {e}"));
                            }
                            None => break,
                        };
                        match extract_event(&text) {
                            WsIncomingEvent::Delta(delta) => {
                                on_delta(&delta).map_err(|e| e.to_string())?;
                            }
                            WsIncomingEvent::Result(text) => {
                                result_text = Some(text);
                            }
                            WsIncomingEvent::Done => break,
                            WsIncomingEvent::Error(msg) => {
                                tracing::warn!(ws_url = %ws_url, server_error = %msg, "acp_llm: WS server returned error");
                                return Err(format!("ACP WS server error: {msg}"));
                            }
                            WsIncomingEvent::Ignore => {}
                        }
                    }

                    // ── Heartbeat tick: send Ping, check previous Pong ───────
                    _ = ping_interval.tick() => {
                        // Check if pong came back since the last ping.
                        let since_pong = last_pong.lock().await.elapsed();
                        if since_pong > Duration::from_secs(WS_PING_INTERVAL_SECS + WS_PONG_TIMEOUT_SECS) {
                            tracing::warn!(
                                ws_url = %ws_url,
                                action = "ws.pong.timeout",
                                since_pong_secs = since_pong.as_secs(),
                                "acp_llm: closing stale connection — pong timeout",
                            );
                            return Err("WS pong timeout — stale connection".to_string());
                        }
                        tracing::debug!(action = "ws.ping.sent");
                        if write.send(Message::Ping(Default::default())).await.is_err() {
                            return Err("ACP WS ping send failed".to_string());
                        }
                    }
                }
            }
            Ok(())
        },
    )
    .await
    .map_err(|_| {
        tracing::warn!(
            ws_url = %ws_url,
            timeout_secs = WS_COMPLETION_TIMEOUT_SECS,
            "acp_llm: WS completion timed out",
        );
        "timeout".to_string()
    })
    .and_then(|r| r);

    match loop_result {
        Ok(()) => {}
        Err(e) if e == "timeout" => {
            return Err(
                format!("ACP WS completion timed out after {WS_COMPLETION_TIMEOUT_SECS}s").into(),
            );
        }
        Err(e) => return Err(e.into()),
    }

    result_text
        .map(|text| AcpCompletionTurnResult { text, usage: None })
        .ok_or_else(|| {
            tracing::error!(ws_url = %ws_url, "acp_llm: WS server sent Done without Result — protocol violation");
            "ACP WS completion finished without a turn result".into()
        })
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
