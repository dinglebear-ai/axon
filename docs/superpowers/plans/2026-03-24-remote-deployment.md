# Remote Deployment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable `axon` CLI and MCP server to run on a machine that does not host any of the backing services, by fixing silent config footguns and routing ACP completions through the server's existing WebSocket protocol.

**Architecture:** Three independent changes: (1) make `qdrant_url` and `tei_url` required at config-parse time like the other service URLs; (2) add `AXON_ACP_WS_URL` / `AXON_ACP_WS_TOKEN` config fields; (3) implement `AcpWsCompletionRunner` that connects to the existing `/ws` endpoint with `mode: "pulse_chat"` and plugs into the `AcpCompletionRunner` trait — zero server-side changes needed.

**Tech Stack:** Rust, `tokio-tungstenite` (already in `Cargo.toml` with `rustls-tls-native-roots`), existing `WsClientMsg` / `WsEventV2` wire protocol.

---

## File Map

| Action | File | Why |
|--------|------|-----|
| Modify | `crates/core/config/parse/build_config.rs` | Make `qdrant_url` + `tei_url` required; read `AXON_ACP_WS_URL` + `AXON_ACP_WS_TOKEN` |
| Modify | `crates/core/config/types/config.rs` | Add `acp_ws_url: Option<String>` + `acp_ws_token: Option<String>` |
| Modify | `crates/core/config/types/config_impls.rs` | Add defaults + redact `acp_ws_token` in Debug |
| Create | `crates/services/acp_llm/ws_runner.rs` | `AcpWsCompletionRunner` — WS client that speaks pulse_chat protocol |
| Modify | `crates/services/acp_llm.rs` | Route `complete_text` / `complete_streaming` through WS runner when `acp_ws_url` is set |
| Modify | `.env.example` | Document the two new env vars |

---

## Task 1: Make `qdrant_url` Required

**Files:**
- Modify: `crates/core/config/parse/build_config.rs` (~line 384)

### Background

Currently `qdrant_url` silently falls back to `http://127.0.0.1:53333` when `QDRANT_URL` is not set. On a remote machine this means all vector operations silently try `127.0.0.1` and produce confusing connection errors instead of a clear config message. `pg_url`, `redis_url`, and `amqp_url` are already required — `qdrant_url` should match.

- [ ] **Step 1: Write the failing test**

Add inside the existing `#[cfg(test)] mod tests` block in `build_config.rs`:

```rust
#[test]
fn into_config_errors_when_qdrant_url_missing() {
    let _guard = ENV_LOCK.lock().unwrap();
    // Unset QDRANT_URL so the default branch is exercised.
    // Safety: test-only, guarded by ENV_LOCK.
    unsafe { env::remove_var("QDRANT_URL"); }

    let cli = Cli::parse_from([
        "axon",
        "--pg-url", "postgresql://axon:postgres@127.0.0.1:53432/axon", <!-- gitleaks:allow -->
        "--redis-url", "redis://127.0.0.1:53379",
        "--amqp-url", "amqp://axon:axonrabbit@127.0.0.1:45535/%2f", <!-- gitleaks:allow -->
        "status",
    ]);
    let err = into_config(cli).unwrap_err();
    assert!(
        err.contains("QDRANT_URL"),
        "expected QDRANT_URL error, got: {err}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test into_config_errors_when_qdrant_url_missing -- --nocapture
```

Expected: FAIL — `into_config` currently returns `Ok` with a localhost default.

- [ ] **Step 3: Replace the `qdrant_url` assignment in `build_config.rs`**

Find (around line 384):
```rust
        qdrant_url: global
            .qdrant_url
            .or_else(|| env::var("QDRANT_URL").ok())
            .map(normalize_local_service_url)
            .unwrap_or_else(|| "http://127.0.0.1:53333".to_string()),
```

Replace with:
```rust
        qdrant_url: normalize_local_service_url(
            global
                .qdrant_url
                .or_else(|| env::var("QDRANT_URL").ok())
                .ok_or_else(|| {
                    "QDRANT_URL environment variable is required (or pass --qdrant-url). \
                     Copy .env.example to .env and fill in credentials."
                        .to_string()
                })?,
        ),
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test into_config_errors_when_qdrant_url_missing -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Run the full config test suite**

```bash
cargo test build_config -- --nocapture
```

Expected: All tests pass. If any test was relying on the localhost default without setting `QDRANT_URL`, it will fail — fix it by adding `--qdrant-url http://127.0.0.1:53333` to the `Cli::parse_from` call in that test.

- [ ] **Step 6: Verify `cargo check` is clean**

```bash
cargo check --bin axon
```

- [ ] **Step 7: Commit**

```bash
git add crates/core/config/parse/build_config.rs
git commit -m "fix(config): make QDRANT_URL required — removes silent localhost default"
```

---

## Task 2: Make `tei_url` Required

**Files:**
- Modify: `crates/core/config/parse/build_config.rs` (~line 379)

### Background

`tei_url` defaults to empty string (`unwrap_or_default()`). Any command that calls `tei_embed()` with an empty URL fails with a cryptic HTTP error rather than a clear configuration message. TEI is needed for all vector operations (query, ask, embed). Making it required at config-parse time matches the pattern of every other service URL.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `build_config.rs`:

```rust
#[test]
fn into_config_errors_when_tei_url_missing() {
    let _guard = ENV_LOCK.lock().unwrap();
    unsafe { env::remove_var("TEI_URL"); }

    let cli = Cli::parse_from([
        "axon",
        "--pg-url", "postgresql://axon:postgres@127.0.0.1:53432/axon", <!-- gitleaks:allow -->
        "--redis-url", "redis://127.0.0.1:53379",
        "--amqp-url", "amqp://axon:axonrabbit@127.0.0.1:45535/%2f", <!-- gitleaks:allow -->
        "--qdrant-url", "http://127.0.0.1:53333",
        "status",
    ]);
    let err = into_config(cli).unwrap_err();
    assert!(
        err.contains("TEI_URL"),
        "expected TEI_URL error, got: {err}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test into_config_errors_when_tei_url_missing -- --nocapture
```

Expected: FAIL.

- [ ] **Step 3: Replace the `tei_url` assignment in `build_config.rs`**

Find (around line 379):
```rust
        tei_url: global
            .tei_url
            .or_else(|| env::var("TEI_URL").ok())
            .map(normalize_local_service_url)
            .unwrap_or_default(),
```

Replace with:
```rust
        tei_url: normalize_local_service_url(
            global
                .tei_url
                .or_else(|| env::var("TEI_URL").ok())
                .ok_or_else(|| {
                    "TEI_URL environment variable is required (or pass --tei-url). \
                     Copy .env.example to .env and fill in credentials."
                        .to_string()
                })?,
        ),
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test into_config_errors_when_tei_url_missing -- --nocapture
```

- [ ] **Step 5: Fix any tests that pass no `TEI_URL`**

```bash
cargo test build_config -- --nocapture
```

Add `--tei-url http://127.0.0.1:52000` to any `Cli::parse_from` calls that now error. Also check the existing `into_config_normalizes_tei_url_like_other_services` test — it already passes `--tei-url`, so it should still pass.

- [ ] **Step 6: Run full test suite and check**

```bash
cargo check --bin axon && cargo test build_config -- --nocapture
```

- [ ] **Step 7: Commit**

```bash
git add crates/core/config/parse/build_config.rs
git commit -m "fix(config): make TEI_URL required — removes silent empty-string default"
```

---

## Task 3: Add `acp_ws_url` and `acp_ws_token` to Config

**Files:**
- Modify: `crates/core/config/types/config.rs`
- Modify: `crates/core/config/types/config_impls.rs`
- Modify: `crates/core/config/parse/build_config.rs`

### Background

`AXON_ACP_WS_URL` points to a running `axon serve` instance (e.g. `http://server:49000` or `ws://server:49000`). When set, `acp_llm` will route completions through that server's WebSocket instead of spawning a local subprocess. `AXON_ACP_WS_TOKEN` is the bearer token that matches `AXON_WEB_API_TOKEN` on the server side.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `build_config.rs`:

```rust
#[allow(unsafe_code)]
#[test]
fn into_config_reads_acp_ws_url_from_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    const VAR: &str = "AXON_ACP_WS_URL";
    unsafe { env::set_var(VAR, "ws://axon-server:49000"); }

    let cli = Cli::parse_from([
        "axon",
        "--pg-url", "postgresql://axon:postgres@127.0.0.1:53432/axon", <!-- gitleaks:allow -->
        "--redis-url", "redis://127.0.0.1:53379",
        "--amqp-url", "amqp://axon:axonrabbit@127.0.0.1:45535/%2f", <!-- gitleaks:allow -->
        "--qdrant-url", "http://127.0.0.1:53333",
        "--tei-url", "http://127.0.0.1:52000",
        "status",
    ]);
    let cfg = into_config(cli).expect("config should parse");
    assert_eq!(cfg.acp_ws_url.as_deref(), Some("ws://axon-server:49000"));

    unsafe { env::remove_var(VAR); }
}

#[allow(unsafe_code)]
#[test]
fn into_config_reads_acp_ws_token_from_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    const VAR: &str = "AXON_ACP_WS_TOKEN";
    unsafe { env::set_var(VAR, "super-secret-token"); }

    let cli = Cli::parse_from([
        "axon",
        "--pg-url", "postgresql://axon:postgres@127.0.0.1:53432/axon", <!-- gitleaks:allow -->
        "--redis-url", "redis://127.0.0.1:53379",
        "--amqp-url", "amqp://axon:axonrabbit@127.0.0.1:45535/%2f", <!-- gitleaks:allow -->
        "--qdrant-url", "http://127.0.0.1:53333",
        "--tei-url", "http://127.0.0.1:52000",
        "status",
    ]);
    let cfg = into_config(cli).expect("config should parse");
    assert_eq!(cfg.acp_ws_token.as_deref(), Some("super-secret-token"));

    unsafe { env::remove_var(VAR); }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test into_config_reads_acp_ws -- --nocapture
```

Expected: compile error — `cfg.acp_ws_url` does not exist yet.

- [ ] **Step 3: Add fields to `Config` struct**

In `crates/core/config/types/config.rs`, find the block containing `acp_adapter_cmd` and add after `acp_prewarm`:

```rust
    /// Remote axon serve WebSocket URL for ACP completions.
    /// When set, acp_llm routes through the remote WS instead of a local subprocess.
    /// Format: ws://host:port  or  http://host:port  (scheme is normalised to ws://).
    /// Env: AXON_ACP_WS_URL
    pub acp_ws_url: Option<String>,
    /// Bearer token for the remote WS ACP endpoint (matches AXON_WEB_API_TOKEN on server).
    /// Env: AXON_ACP_WS_TOKEN
    pub acp_ws_token: Option<String>,
```

- [ ] **Step 4: Add defaults to `Config::default()` in `config_impls.rs`**

In `Config::default()`, after the `acp_prewarm` line:

```rust
            acp_ws_url: None,
            acp_ws_token: None,
```

Also add `acp_ws_token` to the `fmt::Debug` redaction block (it is a secret):

```rust
            .field("acp_ws_url", &self.acp_ws_url)
            .field("acp_ws_token", &"[REDACTED]")
```

- [ ] **Step 5: Read from env in `build_config.rs`**

In `into_config()`, after the `acp_prewarm` line:

```rust
        acp_ws_url: env::var("AXON_ACP_WS_URL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        acp_ws_token: env::var("AXON_ACP_WS_TOKEN")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
```

- [ ] **Step 6: Run tests to verify they pass**

```bash
cargo test into_config_reads_acp_ws -- --nocapture
```

- [ ] **Step 7: Verify full test suite**

```bash
cargo check --bin axon && cargo test -- --nocapture 2>&1 | tail -5
```

- [ ] **Step 8: Commit**

```bash
git add crates/core/config/types/config.rs \
        crates/core/config/types/config_impls.rs \
        crates/core/config/parse/build_config.rs
git commit -m "feat(config): add AXON_ACP_WS_URL + AXON_ACP_WS_TOKEN for remote ACP routing"
```

---

## Task 4: Implement `AcpWsCompletionRunner`

**Files:**
- Create: `crates/services/acp_llm/ws_runner.rs`
- Modify: `crates/services/acp_llm.rs` (add `mod ws_runner;`)

### Background

This module implements `AcpCompletionRunner` by opening a WebSocket connection to a remote `axon serve` instance and submitting a `pulse_chat` execute request. It speaks the existing WS wire protocol — no server-side changes needed.

**Wire protocol recap:**

Client sends:
```json
{"type":"execute","mode":"pulse_chat","input":"<prompt>","id":"<uuid>","flags":{}}
```

Server emits (until `command.done`):
```json
{"type":"command.output.json","data":{"ctx":{...},"data":{"type":"assistant_delta","session_id":"...","delta":"..."}}}
{"type":"command.output.json","data":{"ctx":{...},"data":{"type":"result","session_id":"...","stop_reason":"end_turn","result":"..."}}}
{"type":"command.done","data":{"ctx":{...},"payload":{"exit_code":0}}}
```

On failure:
```json
{"type":"command.error","data":{"ctx":{...},"payload":{"message":"..."}}}
```

The `input` field is built with `compose_prompt()` (from `runner.rs`) to combine `system_prompt` + `user_prompt`. If `req.model` is set, pass it via `flags: {"model": "<model>"}`.

**URL normalisation:** Accept both `http://` and `ws://` prefixes. Strip trailing slashes. Append `/ws`. Convert to `ws://` or `wss://` as appropriate.

- [ ] **Step 0: Verify `futures-util` and `tokio-tungstenite` are in the services crate**

```bash
grep -E 'futures-util|tokio-tungstenite' Cargo.toml crates/services/Cargo.toml
```

Expected output shows both crates present in **`crates/services/Cargo.toml`** (workspace root entry alone is not sufficient — `ws_runner.rs` uses `use futures_util::{SinkExt, StreamExt}` directly):
```
Cargo.toml:futures-util = "0.3"
Cargo.toml:tokio-tungstenite = { version = "0.29", features = ["rustls-tls-native-roots"] }
crates/services/Cargo.toml:futures-util.workspace = true
crates/services/Cargo.toml:tokio-tungstenite.workspace = true
```

If either is missing from `crates/services/Cargo.toml`, add it under `[dependencies]` before proceeding:
```toml
futures-util.workspace = true
tokio-tungstenite.workspace = true
```

- [ ] **Step 1: Write the failing tests** (pure logic, no live server)

Create `crates/services/acp_llm/ws_runner.rs` with tests only:

```rust
//! WebSocket-backed ACP completion runner for remote deployments.

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
        // Tokens containing '&', '=', '#' must not break the URL structure.
        let result = build_ws_endpoint("http://server:49000", Some("tok&evil=1"));
        assert!(result.contains("tok%26evil%3D1"), "special chars must be encoded: {result}");
        assert!(!result.contains("tok&evil"), "raw & must not appear in URL: {result}");
    }

    #[test]
    fn compose_execute_message_includes_prompt_and_id() {
        let msg = compose_execute_msg("hello world", None, "req-1");
        let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(v["type"], "execute");
        assert_eq!(v["mode"], "pulse_chat");
        assert_eq!(v["input"], "hello world");
        assert_eq!(v["id"], "req-1");
    }

    #[test]
    fn compose_execute_message_includes_model_in_flags() {
        let msg = compose_execute_msg("hello", Some("claude-opus-4-5"), "req-2");
        let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(v["flags"]["model"], "claude-opus-4-5");
    }

    #[test]
    fn extract_delta_from_output_json_event() {
        let raw = r#"{"type":"command.output.json","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"data":{"type":"assistant_delta","session_id":"s1","delta":"Hello","tool_call_id":null}}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Delta("Hello".to_string()));
    }

    #[test]
    fn extract_result_from_output_json_event() {
        let raw = r#"{"type":"command.output.json","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"data":{"type":"result","session_id":"s1","stop_reason":"end_turn","result":"Final answer"}}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Result("Final answer".to_string()));
    }

    #[test]
    fn extract_done_from_command_done_event() {
        let raw = r#"{"type":"command.done","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"payload":{"exit_code":0}}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Done);
    }

    #[test]
    fn extract_error_from_command_error_event() {
        let raw = r#"{"type":"command.error","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"payload":{"message":"oops"}}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Error("oops".to_string()));
    }

    #[test]
    fn unknown_event_type_is_ignored() {
        let raw = r#"{"type":"stats","data":{}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Ignore);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test acp_llm::ws_runner -- --nocapture
```

Expected: compile errors — none of the functions or types exist yet.

- [ ] **Step 3: Implement the module**

Replace the content of `crates/services/acp_llm/ws_runner.rs` with the full implementation:

```rust
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
            return Err(format!(
                "ACP WS completion timed out after {WS_COMPLETION_TIMEOUT_SECS}s"
            )
            .into());
        }
    }

    result_text
        .map(|text| AcpCompletionTurnResult { text, usage: None })
        .ok_or_else(|| "ACP WS completion finished without a turn result".into())
}

// ── Wire helpers ──────────────────────────────────────────────────────────────

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
            let encoded: String =
                url::form_urlencoded::byte_serialize(tok.as_bytes()).collect();
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
        // Tokens containing '&', '=', '#' must not break the URL structure.
        let result = build_ws_endpoint("http://server:49000", Some("tok&evil=1"));
        assert!(result.contains("tok%26evil%3D1"), "special chars must be encoded: {result}");
        assert!(!result.contains("tok&evil"), "raw & must not appear in URL: {result}");
    }

    #[test]
    fn compose_execute_message_includes_prompt_and_id() {
        let msg = compose_execute_msg("hello world", None, "req-1");
        let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(v["type"], "execute");
        assert_eq!(v["mode"], "pulse_chat");
        assert_eq!(v["input"], "hello world");
        assert_eq!(v["id"], "req-1");
    }

    #[test]
    fn compose_execute_message_includes_model_in_flags() {
        let msg = compose_execute_msg("hello", Some("claude-opus-4-5"), "req-2");
        let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(v["flags"]["model"], "claude-opus-4-5");
    }

    #[test]
    fn extract_delta_from_output_json_event() {
        let raw = r#"{"type":"command.output.json","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"data":{"type":"assistant_delta","session_id":"s1","delta":"Hello","tool_call_id":null}}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Delta("Hello".to_string()));
    }

    #[test]
    fn extract_result_from_output_json_event() {
        let raw = r#"{"type":"command.output.json","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"data":{"type":"result","session_id":"s1","stop_reason":"end_turn","result":"Final answer"}}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Result("Final answer".to_string()));
    }

    #[test]
    fn extract_done_from_command_done_event() {
        let raw = r#"{"type":"command.done","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"payload":{"exit_code":0}}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Done);
    }

    #[test]
    fn extract_error_from_command_error_event() {
        let raw = r#"{"type":"command.error","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"payload":{"message":"oops"}}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Error("oops".to_string()));
    }

    #[test]
    fn empty_delta_is_forwarded_as_delta_event() {
        // ACP adapters emit empty-string deltas as keep-alives; they must not be dropped.
        let raw = r#"{"type":"command.output.json","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"data":{"type":"assistant_delta","session_id":"s1","delta":"","tool_call_id":null}}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Delta("".to_string()));
    }

    #[test]
    fn empty_result_is_ignored() {
        // An empty result string is not a valid completion — treat as Ignore.
        let raw = r#"{"type":"command.output.json","data":{"ctx":{"exec_id":"r1","mode":"pulse_chat","input":"hi"},"data":{"type":"result","session_id":"s1","stop_reason":"end_turn","result":""}}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Ignore);
    }

    #[test]
    fn unknown_event_type_is_ignored() {
        let raw = r#"{"type":"stats","data":{}}"#;
        assert_eq!(extract_event(raw), WsIncomingEvent::Ignore);
    }
}
```

- [ ] **Step 4: Register the module in `acp_llm.rs`**

Add to the top of `crates/services/acp_llm.rs`:

```rust
mod ws_runner;
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test acp_llm::ws_runner -- --nocapture
```

Expected: all 13 tests pass (pure logic, no network).

- [ ] **Step 6: Check monolith policy**

```bash
wc -l crates/services/acp_llm/ws_runner.rs
```

Must be ≤ 500 lines.

- [ ] **Step 7: Commit**

```bash
git add crates/services/acp_llm/ws_runner.rs crates/services/acp_llm.rs
git commit -m "feat(acp_llm): add AcpWsCompletionRunner — speaks pulse_chat WS protocol"
```

---

## Task 5: Route `acp_llm` Through WS Runner When Configured

**Files:**
- Modify: `crates/services/acp_llm.rs`

### Background

The public `complete_text` and `complete_streaming` functions currently always construct an `AcpRuntimeCompletionRunner` (local subprocess). After this task, they check `cfg.acp_ws_url` first. When set, they construct an `AcpWsCompletionRunner` instead. The behaviour from callers' perspective is identical — same trait, same return types.

`warm_session` is subprocess-specific (pre-warming a remote WS connection doesn't make sense) so it continues to require the local adapter. Callers that need pre-warming (`ask` synthesis hot path) will keep using `warm_session` and will get a clear error if the local adapter is absent and no WS URL is configured.

- [ ] **Step 1: Write the failing test**

Add to `crates/services/acp_llm.rs` tests (or a new inline `#[cfg(test)]` block):

```rust
#[cfg(test)]
mod routing_tests {
    use super::*;
    use crate::crates::core::config::Config;

    #[tokio::test]
    async fn complete_text_uses_ws_runner_when_acp_ws_url_is_set() {
        // Uses a non-existent server so the WS connect will fail — but the
        // important thing is the error message mentions the WS URL, not the
        // local subprocess adapter.
        let cfg = Config {
            acp_ws_url: Some("ws://127.0.0.1:1".to_string()),
            acp_ws_token: None,
            ..Config::test_default()
        };
        let req = AcpCompletionRequest::new("hello");
        let err = complete_text(&cfg, req).await.unwrap_err();
        // Error should come from the WS connect path, not subprocess spawn.
        let msg = err.to_string();
        assert!(
            msg.contains("ACP WS connect failed") || msg.contains("Connection refused"),
            "expected WS connect error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn complete_text_uses_subprocess_runner_when_acp_ws_url_is_not_set() {
        let cfg = Config {
            acp_ws_url: None,
            acp_adapter_cmd: None, // no adapter → subprocess path fails with adapter error
            ..Config::test_default()
        };
        let req = AcpCompletionRequest::new("hello");
        let err = complete_text(&cfg, req).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("AXON_ACP_ADAPTER_CMD"),
            "expected adapter cmd error, got: {msg}"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test acp_llm::routing_tests -- --nocapture
```

Expected: FAIL — `complete_text` does not yet check `acp_ws_url`.

- [ ] **Step 3: Update `complete_text` and `complete_streaming` in `acp_llm.rs`**

Replace:
```rust
pub async fn complete_text(
    cfg: &Config,
    req: AcpCompletionRequest,
) -> Result<AcpCompletionResponse, Box<dyn StdError>> {
    let runner = AcpRuntimeCompletionRunner::from_config(cfg)?;
    complete_text_with_runner(&runner, req).await
}

pub async fn complete_streaming<F>(
    cfg: &Config,
    req: AcpCompletionRequest,
    on_delta: F,
) -> Result<AcpCompletionResponse, Box<dyn StdError>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
{
    let runner = AcpRuntimeCompletionRunner::from_config(cfg)?;
    complete_streaming_with_runner(&runner, req, on_delta).await
}
```

With:
```rust
pub async fn complete_text(
    cfg: &Config,
    req: AcpCompletionRequest,
) -> Result<AcpCompletionResponse, Box<dyn StdError>> {
    if cfg.acp_ws_url.is_some() {
        let runner = ws_runner::AcpWsCompletionRunner::from_config(cfg)?;
        return complete_text_with_runner(&runner, req).await;
    }
    let runner = AcpRuntimeCompletionRunner::from_config(cfg)?;
    complete_text_with_runner(&runner, req).await
}

pub async fn complete_streaming<F>(
    cfg: &Config,
    req: AcpCompletionRequest,
    on_delta: F,
) -> Result<AcpCompletionResponse, Box<dyn StdError>>
where
    F: FnMut(&str) -> Result<(), Box<dyn StdError>> + Send,
{
    if cfg.acp_ws_url.is_some() {
        let runner = ws_runner::AcpWsCompletionRunner::from_config(cfg)?;
        return complete_streaming_with_runner(&runner, req, on_delta).await;
    }
    let runner = AcpRuntimeCompletionRunner::from_config(cfg)?;
    complete_streaming_with_runner(&runner, req, on_delta).await
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test acp_llm::routing_tests -- --nocapture
```

- [ ] **Step 5: Run the full test suite**

```bash
cargo test -- --nocapture 2>&1 | tail -10
```

All existing tests should still pass.

- [ ] **Step 6: Run clippy**

```bash
cargo clippy --bin axon -- -D warnings
```

- [ ] **Step 7: Commit**

```bash
git add crates/services/acp_llm.rs
git commit -m "feat(acp_llm): route completions through WS runner when AXON_ACP_WS_URL is set"
```

---

## Task 6: Document New Env Vars in `.env.example`

**Files:**
- Modify: `.env.example`

- [ ] **Step 1: Add the new entries**

Find the `# LLM / ACP completion settings` block in `.env.example` and add after the existing ACP vars:

```bash
# Remote ACP: route ask/research/evaluate completions through a remote axon serve
# instead of spawning a local adapter subprocess.
# Set AXON_ACP_WS_URL to the base URL of a running axon serve instance.
# Set AXON_ACP_WS_TOKEN to the same value as AXON_WEB_API_TOKEN on that server.
# When these are unset, the local AXON_ACP_ADAPTER_CMD is used (default behaviour).
AXON_ACP_WS_URL=
AXON_ACP_WS_TOKEN=
```

Also find the `QDRANT_URL` and `TEI_URL` lines and add `# Required` comments if not already present:

```bash
# Required — no default. For local dev: http://127.0.0.1:53333
QDRANT_URL=http://axon-qdrant:6333

# Required — no default. For local dev: http://127.0.0.1:52000
TEI_URL=http://axon-tei:80
```

- [ ] **Step 2: Verify the file**

```bash
grep -A2 "AXON_ACP_WS_URL\|QDRANT_URL\|TEI_URL" .env.example
```

- [ ] **Step 3: Run the full verify gate**

```bash
just verify
```

Expected: `fmt-check + clippy + check + test` all pass.

- [ ] **Step 4: Final commit**

```bash
git add .env.example
git commit -m "docs(env): document AXON_ACP_WS_URL, AXON_ACP_WS_TOKEN; mark QDRANT_URL + TEI_URL required"
```

---

## Remote Deployment Quick-Start (Summary)

After this implementation, deploying `axon` to a remote machine requires only:

```bash
# Backing services (on the server machine)
AXON_PG_URL=postgresql://axon:pass@server:53432/axon
AXON_REDIS_URL=redis://server:53379
AXON_AMQP_URL=amqp://axon:pass@server:45535/%2f
QDRANT_URL=http://server:53333
TEI_URL=http://server:52000

# ACP completions routed through the server's WebSocket
AXON_ACP_WS_URL=http://server:49000
AXON_ACP_WS_TOKEN=<matches AXON_WEB_API_TOKEN on the server>
```

No ACP adapter installed on the remote machine. `axon ask`, `axon research`, `axon evaluate` all work via the server's existing Pulse Chat WebSocket endpoint.
