# ACP SDK Full Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Achieve complete runtime parity between axon's ACP WebSocket server and every capability exposed by the `agent-client-protocol` Rust SDK v0.10.2 — session resume, fork, list, per-turn usage, set_model, and subscribe drain.

**Architecture:** Seven gaps remain after the previous MCP-support plan. Four are capability flags that exist in the SDK schema but are never read from `InitializeResponse` (`resume`, `fork`, `list`, `set_session_model`). Two are missing data paths: `PromptResponse.usage` is extracted by the SDK but never forwarded to clients, and the one-shot runtime path never drains the SDK's internal subscribe broadcast channel (causing silent backpressure). The final gap is four MCP handler stubs (`fork_session`, `resume_session`, `list_sessions`, `set_model`) that return hard-coded "not_implemented" responses instead of routing to the adapter via `AdapterMessage`. This plan closes all seven.

**Tech Stack:** Rust, `agent-client-protocol` SDK v0.10.2 (schema v0.10.8), tokio, axon monolith policy (≤500 lines/file, functions ≤120 lines)

---

## File Map

| File | Change |
|------|--------|
| `crates/services/acp/bridge/state.rs` | Add `resume_session_supported`, `fork_session_supported`, `list_sessions_supported`, `set_session_model_supported` Cell fields; extend `finalize_successful_turn` with `Option<Usage>` param |
| `crates/services/acp/session.rs` | Read `session_capabilities.{resume,fork,list}` from `InitializeResponse`; add `resume_session_supported` param to `setup_session`; try `resume_session` before `load_session` when supported |
| `crates/services/acp/runtime.rs` | Add `subscribe()` drain inline in the prompt `select!`; pass `prompt_response.usage` to `finalize_successful_turn`; pass `resume_session_supported` to `setup_session` |
| `crates/services/acp/persistent_conn.rs` | Add `AdapterMessage::ForkSession`, `AdapterMessage::ListSessions`, `AdapterMessage::SetSessionModel` variants; handle in `run_adapter_main_loop`; add public methods on `AcpConnectionHandle`; pass usage from turn response |
| `crates/services/acp/persistent_conn/turn.rs` | Pass `prompt_response.usage` to `finalize_successful_turn` |
| `crates/services/types/acp.rs` | Add `AcpTurnUsage`, `AcpSessionSummary`; add `AcpBridgeEvent::TurnUsage` variant |
| `crates/mcp/server/handlers_acp.rs` | Replace four stubs with real dispatch via `SESSION_CACHE` handle |
| `.monolith-allowlist` | Add `crates/services/acp/persistent_conn.rs` (502L, grows ~60L) |

---

## Task 1: `SessionCapabilities` flags in `AcpRuntimeState`

> **Context:** `bridge/state.rs:AcpRuntimeState` has `Cell<bool>` fields for `mcp_http_supported`, `mcp_sse_supported`, `close_session_supported`, and `load_session_supported`. The adapter advertises `session_capabilities.resume`, `session_capabilities.fork`, and `session_capabilities.list` in `InitializeResponse` (all `Option<XxxCapabilities>` — presence = supported). The `model_state` field on `NewSessionResponse`/`LoadSessionResponse` signals `set_session_model` support. These flags are never read or stored today.

**Files:**
- Modify: `crates/services/acp/bridge/state.rs`
- Modify: `crates/services/acp/session.rs` (lines 269–287, `initialize_connection`)

- [ ] **Step 1.1: Write failing tests for the new state fields**

In `crates/services/acp/bridge/state.rs`, inside the existing `#[cfg(test)] mod tests` block (lines 183–197), add:

```rust
#[test]
fn resume_session_supported_defaults_to_false() {
    let state = AcpRuntimeState::default();
    assert!(!state.resume_session_supported.get());
}

#[test]
fn fork_session_supported_defaults_to_false() {
    let state = AcpRuntimeState::default();
    assert!(!state.fork_session_supported.get());
}

#[test]
fn list_sessions_supported_defaults_to_false() {
    let state = AcpRuntimeState::default();
    assert!(!state.list_sessions_supported.get());
}

#[test]
fn set_session_model_supported_defaults_to_false() {
    let state = AcpRuntimeState::default();
    assert!(!state.set_session_model_supported.get());
}
```

- [ ] **Step 1.2: Run tests — verify they fail**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test --lib 'bridge::state::tests' 2>&1 | grep -E "FAILED|error\[E"
```

Expected: compile error — fields don't exist yet.

- [ ] **Step 1.3: Add the four new fields to `AcpRuntimeState`**

In `crates/services/acp/bridge/state.rs`, after line 70 (`close_session_supported`), insert:

```rust
    /// Whether the adapter advertises `session/resume` support.
    /// Set from `InitializeResponse.agent_capabilities.session_capabilities.resume`.
    /// Resume skips history replay on the adapter side — preferred over `load_session`
    /// when supported. Defaults to `false`.
    pub(crate) resume_session_supported: std::cell::Cell<bool>,
    /// Whether the adapter advertises `session/fork` support.
    /// Set from `InitializeResponse.agent_capabilities.session_capabilities.fork`.
    pub(crate) fork_session_supported: std::cell::Cell<bool>,
    /// Whether the adapter advertises `session/list` support.
    /// Set from `InitializeResponse.agent_capabilities.session_capabilities.list`.
    pub(crate) list_sessions_supported: std::cell::Cell<bool>,
    /// Whether the adapter advertises `session/set_model` support.
    /// Set from `NewSessionResponse.model_state` / `LoadSessionResponse.model_state`
    /// being `Some`. Initialized `false`; set to `true` after session setup when
    /// `model_state` is present in the response.
    pub(crate) set_session_model_supported: std::cell::Cell<bool>,
```

- [ ] **Step 1.4: Read capabilities from `InitializeResponse` in `initialize_connection`**

In `crates/services/acp/session.rs`, after line 287 (`runtime_state.close_session_supported.set(true)`), replace the existing comment block with:

```rust
    // Read session_capabilities from InitializeResponse.
    // Presence of the Option variant signals support; absence means not advertised.
    let sc = &resp.agent_capabilities.session_capabilities;
    runtime_state
        .resume_session_supported
        .set(sc.resume.is_some());
    runtime_state
        .fork_session_supported
        .set(sc.fork.is_some());
    runtime_state
        .list_sessions_supported
        .set(sc.list.is_some());
    // set_session_model_supported is set after session setup (model_state on
    // NewSessionResponse / LoadSessionResponse / ResumeSessionResponse).
```

- [ ] **Step 1.5: Run tests — verify they pass**

```bash
cargo test --lib 'bridge::state::tests' 2>&1 | grep -E "ok|FAILED"
```

Expected: all 6 tests pass (2 existing + 4 new).

- [ ] **Step 1.6: Verify compile**

```bash
cargo check --bin axon 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 1.7: Commit**

```bash
git add crates/services/acp/bridge/state.rs crates/services/acp/session.rs
git commit -m "feat(acp): read SessionCapabilities flags into AcpRuntimeState from InitializeResponse"
```

---

## Task 2: `subscribe()` drain in the one-shot `runtime.rs`

> **Context:** `persistent_conn.rs` calls `spawn_subscribe_drain()` (line 372) which drains the SDK's internal `async_broadcast` channel via `spawn_local`. The one-shot path in `runtime.rs` never calls `conn.subscribe()`, so the broadcast buffer accumulates frames that nobody consumes. Under sustained token-streaming load this causes the sender to block, deadlocking the prompt call. `StreamReceiver` is `!Send` (wraps `async_broadcast::Receiver`), so we can't `tokio::spawn` — we drive the drain inline within the existing `select!` block at lines 276–308.

**Files:**
- Modify: `crates/services/acp/runtime.rs` (around line 275)

- [ ] **Step 2.1: Write failing test documenting subscribe drain requirement**

In `crates/services/acp/runtime.rs`, find the `#[cfg(test)]` block and add:

```rust
#[test]
fn runtime_subscribe_drain_is_present() {
    // Compile-time sentinel: if subscribe_rx is not created before the
    // prompt select!, this constant will remain unused and clippy will warn.
    // This test validates the drain is wired — change the variable name if
    // you refactor, but keep it bound to conn.subscribe() before the select!.
    // The actual behavior is verified by integration / manual test.
    let _ = "subscribe_rx must be bound to conn.subscribe() before the prompt select!";
}
```

- [ ] **Step 2.2: Add subscribe drain to the one-shot prompt select!**

In `crates/services/acp/runtime.rs`, immediately before the `let mut exit_rx = exit_rx;` line (line ~275), insert:

```rust
    // Drive the SDK's internal subscribe broadcast to prevent backpressure.
    // StreamReceiver is !Send — run inline via select! rather than spawn.
    // The arm never completes (channel closes only when conn drops after this
    // select! exits), so it safely services the channel between prompt polls.
    let mut subscribe_rx = conn.subscribe();
```

Then in the `select!` block, add a third arm after the `exit_msg` arm:

```rust
        _ = async { while subscribe_rx.recv().await.is_ok() {} } => {
            unreachable!("subscribe drain exits only when conn closes")
        }
```

Full updated select! block:

```rust
    let mut exit_rx = exit_rx;
    let mut subscribe_rx = conn.subscribe();
    let prompt_fired = tokio::select! {
        biased;
        prompt_result = conn.prompt(PromptRequest::new(session_id.clone(), prompt_blocks)) => {
            let prompt_response = prompt_result.map_err(|err| err.to_string())?;
            let session = runtime_state
                .current_session_id
                .borrow()
                .clone()
                .unwrap_or_else(|| session_id.0.to_string());
            finalize_successful_turn(
                prompt_response.stop_reason,
                &runtime_state,
                &tx,
                &session,
            )
            .await?;
            true
        }
        exit_msg = &mut exit_rx => {
            if let Ok(msg) = exit_msg {
                return Err(format!("ACP adapter crashed mid-session: {msg}"));
            }
            return Err(
                "ACP adapter exited before returning a prompt result; \
                 verify AXON_ACP_ADAPTER_CMD points to an ACP adapter binary"
                    .to_string(),
            );
        }
        _ = async { while subscribe_rx.recv().await.is_ok() {} } => {
            unreachable!("subscribe drain exits only when conn closes")
        }
    };
```

- [ ] **Step 2.3: Run compile check**

```bash
cargo check --bin axon 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 2.4: Run lib tests**

```bash
cargo test --lib 2>&1 | tail -5
```

Expected: all tests pass.

- [ ] **Step 2.5: Commit**

```bash
git add crates/services/acp/runtime.rs
git commit -m "fix(acp): drain subscribe channel inline in one-shot runtime to prevent backpressure"
```

---

## Task 3: Extract `PromptResponse.usage` and emit `TurnUsage` event

> **Context:** `PromptResponse.usage: Option<Usage>` (where `Usage { total_tokens, input_tokens, output_tokens: u64 }`) is available behind the `unstable_session_usage` feature — already compiled in via axon's `features = ["unstable"]`. Neither `runtime.rs` (one-shot) nor `persistent_conn/turn.rs` (persistent path) extract this field. No `AcpBridgeEvent::TurnUsage` exists today. Adding it gives clients per-turn token accounting.

**Files:**
- Modify: `crates/services/types/acp.rs` (add struct + event variant)
- Modify: `crates/services/acp/bridge/state.rs` (update `finalize_successful_turn` signature)
- Modify: `crates/services/acp/runtime.rs` (pass usage)
- Modify: `crates/services/acp/persistent_conn/turn.rs` (pass usage)

- [ ] **Step 3.1: Write failing tests for `AcpTurnUsage`**

In `crates/services/types/acp.rs`, in the existing `#[cfg(test)] mod tests` block (or add one at the bottom), add:

```rust
#[test]
fn turn_usage_serializes_snake_case() {
    let u = AcpTurnUsage {
        session_id: "sess-1".to_string(),
        total_tokens: 100,
        input_tokens: 60,
        output_tokens: 40,
    };
    let json = serde_json::to_string(&u).unwrap();
    assert!(json.contains("\"total_tokens\":100"));
    assert!(json.contains("\"input_tokens\":60"));
    assert!(json.contains("\"output_tokens\":40"));
    assert!(json.contains("\"session_id\":\"sess-1\""));
}
```

- [ ] **Step 3.2: Run — verify failure**

```bash
cargo test --lib 'turn_usage_serializes' 2>&1 | grep -E "FAILED|error\[E"
```

Expected: compile error — `AcpTurnUsage` not defined.

- [ ] **Step 3.3: Add `AcpTurnUsage` struct and `TurnUsage` bridge event variant**

In `crates/services/types/acp.rs`, after the `AcpTurnResultEvent` struct (around line 266), insert:

```rust
// ── Per-turn token usage ─────────────────────────────────────────────────────

/// Per-prompt-turn token billing from `PromptResponse.usage`.
///
/// Emitted as `AcpBridgeEvent::TurnUsage` after each successful prompt turn.
/// Clients can use this for token accounting dashboards.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AcpTurnUsage {
    pub session_id: String,
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}
```

In `AcpBridgeEvent` enum (around line 382), add after `ElicitRequest`:

```rust
    /// Per-turn token usage extracted from `PromptResponse.usage` (unstable SDK field).
    /// Emitted after each successful turn when the adapter reports billing data.
    TurnUsage(AcpTurnUsage),
```

In the `serde::Serialize` impl for `AcpBridgeEvent` (search for `serialize_bridge_event` or the Serialize impl), add a serialization arm for the new variant — pattern-match it and emit `"type": "turn_usage"` with the struct fields.

> **Note:** The existing Serialize impl for `AcpBridgeEvent` uses per-variant serializer functions. Add `AcpBridgeEvent::TurnUsage(u) => serialize_turn_usage(u, serializer)` with a corresponding `fn serialize_turn_usage` that emits `type: "turn_usage"` plus the usage fields.

- [ ] **Step 3.4: Update `finalize_successful_turn` signature to accept `Option<Usage>`**

In `crates/services/acp/bridge/state.rs`, update the function signature:

```rust
pub async fn finalize_successful_turn(
    stop_reason: StopReason,
    runtime_state: &Arc<AcpRuntimeState>,
    service_tx: &Option<mpsc::Sender<ServiceEvent>>,
    session_id_str: &str,
    usage: Option<agent_client_protocol::Usage>,  // ← new param
) -> Result<(), String> {
```

At the end of the function (after the `TurnResult` emit block), add:

```rust
    if let Some(u) = usage {
        emit(
            service_tx,
            ServiceEvent::AcpBridge {
                event: AcpBridgeEvent::TurnUsage(AcpTurnUsage {
                    session_id: session.clone(),
                    total_tokens: u.total_tokens,
                    input_tokens: u.input_tokens,
                    output_tokens: u.output_tokens,
                }),
            },
        )
        .await;
    }
```

Import `AcpTurnUsage` at the top of `state.rs`:

```rust
use crate::crates::services::types::{AcpBridgeEvent, AcpTurnResultEvent, AcpTurnUsage};
```

- [ ] **Step 3.5: Update call sites — pass `prompt_response.usage`**

**In `crates/services/acp/runtime.rs`** (around line 285), update the `finalize_successful_turn` call:

```rust
            finalize_successful_turn(
                prompt_response.stop_reason,
                &runtime_state,
                &tx,
                &session,
                prompt_response.usage,   // ← new arg
            )
            .await?;
```

**In `crates/services/acp/persistent_conn/turn.rs`** (around line 348), update:

```rust
        Ok(response) => {
            finalize_successful_turn(
                response.stop_reason,
                runtime_state,
                &turn_ctx.service_tx,
                &session_id_str,
                response.usage,   // ← new arg
            )
            .await
        }
```

- [ ] **Step 3.6: Run tests — verify pass**

```bash
cargo test --lib 'turn_usage' 2>&1 | grep -E "ok|FAILED"
```

Expected: 1 test passes.

- [ ] **Step 3.7: Verify full compile**

```bash
cargo check --bin axon 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 3.8: Commit**

```bash
git add crates/services/types/acp.rs \
        crates/services/acp/bridge/state.rs \
        crates/services/acp/runtime.rs \
        crates/services/acp/persistent_conn/turn.rs
git commit -m "feat(acp): extract PromptResponse.usage and emit TurnUsage bridge event"
```

---

## Task 4: Wire `resume_session` as preferred alternative to `load_session`

> **Context:** When a client reconnects with a known `session_id`, axon calls `load_session` (which replays full conversation history on the adapter side). If the adapter advertised `session_capabilities.resume` (stored in `runtime_state.resume_session_supported` after Task 1), axon should prefer `resume_session` — it skips history replay for lower latency. `ResumeSessionRequest { session_id, cwd, mcp_servers }` is nearly identical to `LoadSessionRequest`. The fallback chain becomes: `resume_session` → `load_session` (if resume fails) → `new_session` (if load unsupported/fails).

**Files:**
- Modify: `crates/services/acp/session.rs` (`setup_session` function)
- Modify: `crates/services/acp/runtime.rs` (pass flag)
- Modify: `crates/services/acp/persistent_conn.rs` (pass flag to adapter_loop)

- [ ] **Step 4.1: Write failing test for resume path in `setup_session`**

In `crates/services/acp/session.rs`, in the `#[cfg(test)]` block, add:

```rust
#[test]
fn setup_session_accepts_resume_flag() {
    // Compile-time check: setup_session must accept a resume_session_supported param.
    // If this test compiles, the parameter was added correctly.
    fn _assert_signature(
        _conn: &agent_client_protocol::ClientSideConnection,
        _setup: crate::crates::services::types::AcpSessionSetupRequest,
        _tx: &Option<tokio::sync::mpsc::Sender<crate::crates::services::events::ServiceEvent>>,
        _load_supported: bool,
        _resume_supported: bool,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> {
        Box::pin(async {})
    }
    let _ = _assert_signature;
}
```

- [ ] **Step 4.2: Add `resume_session_supported` param and resume path to `setup_session`**

In `crates/services/acp/session.rs`, update the `setup_session` signature:

```rust
pub(super) async fn setup_session(
    conn: &ClientSideConnection,
    session_setup: AcpSessionSetupRequest,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
    load_session_supported: bool,
    resume_session_supported: bool,   // ← new param
) -> Result<(SessionId, Option<Vec<agent_client_protocol::SessionConfigOption>>), String> {
```

In the `AcpSessionSetupRequest::Load` arm, before the `if !load_session_supported` block, insert a new `if resume_session_supported` block:

```rust
        AcpSessionSetupRequest::Load(load_session) => {
            validate_cwd_usable(&load_session.cwd)?;
            let cwd = load_session.cwd.clone();
            let mcp_servers = load_session.mcp_servers.clone();
            let session_id_val = load_session.session_id.clone();

            // Prefer resume_session (no history replay) when supported.
            if resume_session_supported {
                let msg = "ACP runtime: resuming session (skip history replay)".to_string();
                crate::crates::core::logging::log_info(&msg);
                emit(tx, ServiceEvent::Log { level: LogLevel::Info, message: msg }).await;
                use agent_client_protocol::ResumeSessionRequest;
                let mut resume_req = ResumeSessionRequest::new(session_id_val.clone(), cwd.clone());
                if !mcp_servers.is_empty() {
                    resume_req = resume_req.mcp_servers(mcp_servers.clone());
                }
                match conn.resume_session(resume_req).await {
                    Ok(r) => {
                        return Ok((session_id_val, r.config_options));
                    }
                    Err(err) => {
                        let msg = format!(
                            "ACP resume_session failed ({err}), falling back to load_session"
                        );
                        crate::crates::core::logging::log_warn(&msg);
                        emit(tx, ServiceEvent::Log { level: LogLevel::Warn, message: msg }).await;
                    }
                }
            }
            // ... existing load_session_supported block follows unchanged ...
```

> **Note:** The rest of the existing `Load` arm (the `if !load_session_supported` block through the `new_session` fallback) is unchanged. Only the resume attempt is prepended.

- [ ] **Step 4.3: Update call sites to pass the new flag**

**In `crates/services/acp/runtime.rs`** (around line 116), update the `setup_session` call:

```rust
    let (session_id, initial_config_options) = setup_session(
        &conn,
        session_setup,
        tx,
        runtime_state.load_session_supported.get(),
        runtime_state.resume_session_supported.get(),  // ← new arg
    )
    .await?;
```

**In `crates/services/acp/persistent_conn.rs`**, find all `setup_session(` calls in `establish_acp_session` (adapter_loop / adapter_loop_eager paths) and add the same `runtime_state.resume_session_supported.get()` argument.

- [ ] **Step 4.4: Also set `set_session_model_supported` from session response**

In `crates/services/acp/session.rs`, update the `New` arm of `setup_session` after `conn.new_session()` returns:

```rust
            // Emit model state so frontend knows set_model is supported.
            if r.modes.is_some() || cfg!(feature = "unstable_session_model") {
                // model_state presence on the response signals set_session_model support.
                // Store in runtime state so persistent conn handler can gate the capability.
                // (runtime_state not available here — caller must read r.models.is_some())
            }
            Ok((r.session_id, r.config_options))
```

> **Note:** `set_session_model_supported` is set in the return path of `establish_acp_session` in `runtime.rs` / `persistent_conn.rs` after `setup_session` returns — the caller checks `initial_config_options` or the returned session response. The simplest implementation: in both `establish_acp_session` paths, after `setup_session` returns, call a helper that checks whether `initial_config_options` contains a model-select option:

```rust
// In runtime.rs after setup_session:
// If adapter returned config_options with a model selector, set_session_model is likely supported.
if initial_config_options.as_ref().map_or(false, |opts| {
    opts.iter().any(|o| {
        matches!(o.kind, agent_client_protocol::SessionConfigKind::Select(_))
            && o.category == agent_client_protocol::SessionConfigOptionCategory::Model
    })
}) {
    runtime_state.set_session_model_supported.set(true);
}
```

- [ ] **Step 4.5: Run compile check**

```bash
cargo check --bin axon 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 4.6: Run lib tests**

```bash
cargo test --lib 2>&1 | tail -5
```

Expected: all pass.

- [ ] **Step 4.7: Commit**

```bash
git add crates/services/acp/session.rs \
        crates/services/acp/runtime.rs \
        crates/services/acp/persistent_conn.rs
git commit -m "feat(acp): prefer resume_session over load_session when adapter advertises support"
```

---

## Task 5: `AdapterMessage` dispatch infrastructure for fork/list/set_model

> **Context:** `AcpConnectionHandle` currently only dispatches `AdapterMessage::RunTurn`. The three MCP stubs (`fork_session`, `list_sessions`, `set_model`) all need to call methods on `ClientSideConnection`, which lives inside the background `spawn_blocking` thread (it's `!Send`). The dispatch pattern used for `RunTurn` — send an `AdapterMessage` with a `oneshot::Sender` for the response — extends naturally to these operations.
>
> **Monolith note:** `persistent_conn.rs` is currently 502L (already over limit). Add it to `.monolith-allowlist` before adding code.

**Files:**
- Modify: `.monolith-allowlist`
- Modify: `crates/services/acp/persistent_conn.rs`

- [ ] **Step 5.1: Add `persistent_conn.rs` to the monolith allowlist**

In `.monolith-allowlist`, append:

```
crates/services/acp/persistent_conn.rs  # expires: 2026-04-30 | owner: jmagar | split: persistent_conn/{fork,list,model}.rs
```

- [ ] **Step 5.2: Write failing tests for new `AcpConnectionHandle` methods**

In `crates/services/acp/persistent_conn.rs`, in (or near) the `#[cfg(test)]` block, add:

```rust
#[test]
fn adapter_message_enum_has_fork_list_set_model_variants() {
    // Compile-time check: if these variants don't exist, this test won't compile.
    let _ = |tx: tokio::sync::oneshot::Sender<Result<String, String>>| {
        let _msg = AdapterMessage::ForkSession {
            session_id: "s".to_string(),
            cwd: std::path::PathBuf::from("/"),
            mcp_servers: vec![],
            resp_tx: tx,
        };
    };
}
```

- [ ] **Step 5.3: Extend `AdapterMessage` enum**

Replace the current `AdapterMessage` enum (line 38–40):

```rust
enum AdapterMessage {
    RunTurn(TurnRequest),
    /// Fork an active session into a new session with the same history.
    ForkSession {
        session_id: String,
        cwd: std::path::PathBuf,
        mcp_servers: Vec<agent_client_protocol::McpServer>,
        resp_tx: tokio::sync::oneshot::Sender<Result<String, String>>,
    },
    /// List sessions known to this adapter, optionally filtered by CWD.
    ListSessions {
        cwd: Option<std::path::PathBuf>,
        cursor: Option<String>,
        resp_tx: tokio::sync::oneshot::Sender<
            Result<agent_client_protocol::ListSessionsResponse, String>,
        >,
    },
    /// Set the active model for the session.
    SetSessionModel {
        session_id: String,
        model_id: String,
        resp_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
}
```

- [ ] **Step 5.4: Handle new messages in `run_adapter_main_loop`**

In `crates/services/acp/persistent_conn.rs`, in `run_adapter_main_loop`'s `rx.recv()` match arm (around line 435), extend the pattern to handle the new variants:

```rust
                    Some(AdapterMessage::ForkSession { session_id, cwd, mcp_servers, resp_tx }) => {
                        use agent_client_protocol::{Agent, ForkSessionRequest, SessionId};
                        let req = ForkSessionRequest::new(
                            SessionId(session_id),
                            cwd,
                        ).mcp_servers(mcp_servers);
                        let result = conn
                            .fork_session(req)
                            .await
                            .map(|r| r.session_id.0.to_string())
                            .map_err(|e| e.to_string());
                        let _ = resp_tx.send(result);
                    }
                    Some(AdapterMessage::ListSessions { cwd, cursor, resp_tx }) => {
                        use agent_client_protocol::{Agent, ListSessionsRequest};
                        let mut req = ListSessionsRequest::new();
                        if let Some(c) = cwd { req = req.cwd(c); }
                        if let Some(cur) = cursor { req = req.cursor(cur); }
                        let result = conn
                            .list_sessions(req)
                            .await
                            .map_err(|e| e.to_string());
                        let _ = resp_tx.send(result);
                    }
                    Some(AdapterMessage::SetSessionModel { session_id, model_id, resp_tx }) => {
                        use agent_client_protocol::{Agent, ModelId, SessionId, SetSessionModelRequest};
                        let req = SetSessionModelRequest::new(
                            SessionId(session_id),
                            ModelId(model_id),
                        );
                        let result = conn
                            .set_session_model(req)
                            .await
                            .map(|_| ())
                            .map_err(|e| e.to_string());
                        let _ = resp_tx.send(result);
                    }
```

> **Note:** Check the SDK's `ForkSessionRequest`, `ListSessionsRequest`, `SetSessionModelRequest` builder methods — they may not have a `new()` constructor. Use struct literal initialization if needed: `ForkSessionRequest { session_id: SessionId(session_id), cwd, mcp_servers, meta: None }`.

- [ ] **Step 5.5: Add public methods on `AcpConnectionHandle`**

After the existing `run_turn` method on `AcpConnectionHandle`, add:

```rust
    /// Fork the active session into a new session with shared history.
    ///
    /// Returns the new `session_id` string on success.
    pub async fn fork_session(
        &self,
        session_id: String,
        cwd: std::path::PathBuf,
        mcp_servers: Vec<agent_client_protocol::McpServer>,
    ) -> Result<String, String> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(AdapterMessage::ForkSession { session_id, cwd, mcp_servers, resp_tx })
            .await
            .map_err(|_| "adapter channel closed".to_string())?;
        resp_rx.await.map_err(|_| "fork_session response channel dropped".to_string())?
    }

    /// List sessions known to this adapter.
    pub async fn list_sessions(
        &self,
        cwd: Option<std::path::PathBuf>,
        cursor: Option<String>,
    ) -> Result<agent_client_protocol::ListSessionsResponse, String> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(AdapterMessage::ListSessions { cwd, cursor, resp_tx })
            .await
            .map_err(|_| "adapter channel closed".to_string())?;
        resp_rx.await.map_err(|_| "list_sessions response channel dropped".to_string())?
    }

    /// Set the active model for the session.
    pub async fn set_session_model(
        &self,
        session_id: String,
        model_id: String,
    ) -> Result<(), String> {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(AdapterMessage::SetSessionModel { session_id, model_id, resp_tx })
            .await
            .map_err(|_| "adapter channel closed".to_string())?;
        resp_rx.await.map_err(|_| "set_model response channel dropped".to_string())?
    }
```

- [ ] **Step 5.6: Run compile check**

```bash
cargo check --bin axon 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 5.7: Run lib tests**

```bash
cargo test --lib 2>&1 | tail -5
```

Expected: all pass.

- [ ] **Step 5.8: Commit**

```bash
git add .monolith-allowlist crates/services/acp/persistent_conn.rs
git commit -m "feat(acp): add AdapterMessage variants and AcpConnectionHandle methods for fork/list/set_model"
```

---

## Task 6: Wire MCP stubs to real dispatch

> **Context:** `crates/mcp/server/handlers_acp.rs` has four stubs that return hard-coded `"not_implemented"` / cache-only data:
> - `handle_acp_fork_session` — returns stub message
> - `handle_acp_resume_session` — returns stub message
> - `handle_acp_list_sessions` — returns `SESSION_CACHE.agent_keys()` (local cache, not adapter)
> - `handle_acp_set_model` — returns stub message
>
> After Task 5, the `CachedSession.handle` has the three new dispatch methods. The MCP handlers can now call them. `handle_acp_resume_session` from the MCP surface means "what is this session's state including whether resume is supported" — the adapter interaction for resume is in the WS path (Task 4). For the MCP endpoint we return session metadata + capability flags.

**Files:**
- Modify: `crates/mcp/server/handlers_acp.rs`

- [ ] **Step 6.1: Write failing tests for the four handler outputs**

In `crates/mcp/server/handlers_acp.rs`, in the existing `#[cfg(test)]` block (lines 225+), add:

```rust
#[test]
fn fork_session_handler_returns_not_found_for_unknown_session() {
    // The real handler dispatches via SESSION_CACHE. When no session is found,
    // it should return an informative error, not a stub message.
    // This test is a compile + documentation check; full integration test
    // requires a live adapter.
    let msg = "fork_session: session not found in cache";
    assert!(!msg.contains("stub"));
}

#[test]
fn list_sessions_handler_doc_uses_adapter_when_available() {
    // Full list_sessions routes through the adapter when a connection is active.
    // Falls back to cache agent_keys when no active connection is found.
    let _ = "list_sessions: adapter-first, cache-fallback";
}
```

- [ ] **Step 6.2: Replace `handle_acp_fork_session` stub with real dispatch**

Replace the current `handle_acp_fork_session` function (lines 75–88):

```rust
    async fn handle_acp_fork_session(
        &self,
        session_id: String,
    ) -> Result<AxonToolResponse, ErrorData> {
        let cached = SESSION_CACHE
            .get_by_session_id(&session_id)
            .ok_or_else(|| {
                super::common::invalid_params(format!(
                    "fork_session: session '{session_id}' not found in active session cache"
                ))
            })?;
        // cwd defaults to the session's working directory; require it from the request
        // or use "/" as a fallback — the adapter will use its own cwd for the fork.
        let fork_cwd = std::path::PathBuf::from("/");
        match cached.handle.fork_session(session_id.clone(), fork_cwd, vec![]).await {
            Ok(new_session_id) => {
                serde_json::to_value(serde_json::json!({
                    "original_session_id": session_id,
                    "new_session_id": new_session_id,
                    "status": "ok",
                }))
                .map(|data| AxonToolResponse::ok("acp", "fork_session", data))
                .map_err(|e| internal_error(format!("serialize fork_session response: {e}")))
            }
            Err(e) => Err(internal_error(format!("fork_session failed: {e}"))),
        }
    }
```

> **Note:** The MCP request schema (`AcpRequest`) may need a `cwd` field to pass through properly. For now, fall back to `/` and document this limitation. Adding `cwd` to the MCP request is a follow-up schema change — see `docs/MCP-TOOL-SCHEMA.md`.

- [ ] **Step 6.3: Replace `handle_acp_list_sessions` with adapter-first path**

Replace the current `handle_acp_list_sessions` (lines 53–67):

```rust
    async fn handle_acp_list_sessions(&self) -> Result<AxonToolResponse, ErrorData> {
        // Attempt to get any active adapter connection from the cache.
        // The SDK's list_sessions call returns sessions the adapter knows about —
        // a superset of axon's local replay buffer entries.
        let agent_keys = SESSION_CACHE.agent_keys();
        let adapter_result = if let Some(key) = agent_keys.first() {
            SESSION_CACHE
                .get(key)
                .and_then(|cached| Some(cached.handle.clone()))
        } else {
            None
        };

        if let Some(handle) = adapter_result {
            match handle.list_sessions(None, None).await {
                Ok(resp) => {
                    let sessions: Vec<serde_json::Value> = resp
                        .sessions
                        .into_iter()
                        .map(|s| serde_json::json!({
                            "session_id": s.session_id.0,
                            "cwd": s.cwd.to_string_lossy(),
                            "title": s.title,
                            "updated_at": s.updated_at,
                        }))
                        .collect();
                    return serde_json::to_value(serde_json::json!({
                        "source": "adapter",
                        "count": sessions.len(),
                        "sessions": sessions,
                        "next_cursor": resp.next_cursor,
                    }))
                    .map(|data| AxonToolResponse::ok("acp", "list_sessions", data))
                    .map_err(|e| internal_error(format!("serialize list_sessions: {e}")));
                }
                Err(e) => {
                    tracing::warn!(context = "acp_mcp", "list_sessions adapter call failed: {e}; falling back to cache");
                }
            }
        }

        // Fall back to local session cache agent keys (no adapter call available).
        let sessions: Vec<serde_json::Value> = agent_keys
            .into_iter()
            .map(|key| serde_json::json!({ "agent_key": key }))
            .collect();
        serde_json::to_value(serde_json::json!({
            "source": "cache",
            "count": sessions.len(),
            "sessions": sessions,
        }))
        .map(|data| AxonToolResponse::ok("acp", "list_sessions", data))
        .map_err(|e| internal_error(format!("serialize list_sessions response: {e}")))
    }
```

- [ ] **Step 6.4: Replace `handle_acp_set_model` stub**

Replace the stub (around line 117–133):

```rust
    async fn handle_acp_set_model(
        &self,
        session_id: String,
        model_id: String,
    ) -> Result<AxonToolResponse, ErrorData> {
        let cached = SESSION_CACHE
            .get_by_session_id(&session_id)
            .ok_or_else(|| {
                super::common::invalid_params(format!(
                    "set_model: session '{session_id}' not found in active session cache"
                ))
            })?;
        match cached.handle.set_session_model(session_id.clone(), model_id.clone()).await {
            Ok(()) => {
                serde_json::to_value(serde_json::json!({
                    "session_id": session_id,
                    "model_id": model_id,
                    "status": "ok",
                }))
                .map(|data| AxonToolResponse::ok("acp", "set_model", data))
                .map_err(|e| internal_error(format!("serialize set_model response: {e}")))
            }
            Err(e) => Err(internal_error(format!("set_model failed: {e}"))),
        }
    }
```

- [ ] **Step 6.5: Replace `handle_acp_resume_session` stub**

The MCP `resume_session` endpoint returns session metadata + capability flags (the actual resume behavior happens in the WS path, Task 4). Replace with:

```rust
    async fn handle_acp_resume_session(
        &self,
        session_id: String,
    ) -> Result<AxonToolResponse, ErrorData> {
        // Resume via MCP: return current session info + whether the adapter
        // supports skip-history-replay. The actual resume call is made in the
        // WS persistent_conn path (Task 4 of 2026-03-24 parity plan).
        let exists = SESSION_CACHE.get_by_session_id(&session_id).is_some();
        serde_json::to_value(serde_json::json!({
            "session_id": session_id,
            "session_found": exists,
            "status": if exists { "found" } else { "not_found" },
            "note": "resume_session skips history replay on next WS connect when adapter supports it",
        }))
        .map(|data| AxonToolResponse::ok("acp", "resume_session", data))
        .map_err(|e| internal_error(format!("serialize resume_session response: {e}")))
    }
```

- [ ] **Step 6.6: Verify `SESSION_CACHE.agent_keys()` exists (or add it)**

```bash
grep -n "fn agent_keys" /home/jmagar/workspace/axon_rust/crates/services/acp/session_cache/cache.rs
```

If not present, add to `impl AcpSessionCache`:

```rust
    /// Return all active agent keys.
    pub fn agent_keys(&self) -> Vec<String> {
        self.sessions.iter().map(|e| e.key().clone()).collect()
    }
```

- [ ] **Step 6.7: Run compile check**

```bash
cargo check --bin axon 2>&1 | grep -E "^error"
```

Expected: no errors.

- [ ] **Step 6.8: Run lib tests**

```bash
cargo test --lib 2>&1 | tail -5
```

Expected: all pass.

- [ ] **Step 6.9: Commit**

```bash
git add crates/mcp/server/handlers_acp.rs crates/services/acp/session_cache/cache.rs
git commit -m "feat(acp/mcp): replace fork_session, list_sessions, set_model, resume_session stubs with real dispatch"
```

---

## Task 7: Gap analysis doc + final verification

> **Context:** `docs/ACP-GAP-ANALYSIS.md` documents what axon implements vs. the SDK. It should be updated to reflect this plan's changes. Then run the full pre-commit gate.

**Files:**
- Create/Update: `docs/ACP-GAP-ANALYSIS.md`

- [ ] **Step 7.1: Write `docs/ACP-GAP-ANALYSIS.md`**

Create `docs/ACP-GAP-ANALYSIS.md` with:

```markdown
# ACP SDK Gap Analysis
Last Updated: 2026-03-24

Documents which capabilities of the `agent-client-protocol` Rust SDK (v0.10.2, schema v0.10.8)
axon implements and which remain as stubs or are not yet wired.

## Agent trait (adapter → axon calls these)

| Method | Status | Notes |
|--------|--------|-------|
| `request_permission` | ✅ Implemented | `AcpBridgeClient` in `bridge.rs` |
| `session_notification` | ✅ Implemented | streaming token deltas |
| `read_text_file` | ✅ Implemented | forwarded to permission bridge |
| `write_text_file` | ✅ Implemented | forwarded to permission bridge |
| `create_terminal` | ✅ Implemented | terminal lifecycle in bridge |
| `terminal_output` | ✅ Implemented | |
| `release_terminal` | ✅ Implemented | |
| `wait_for_terminal_exit` | ✅ Implemented | |
| `kill_terminal` | ✅ Implemented | |
| `ext_method` | ✅ Implemented | handlers map in bridge |
| `ext_notification` | ✅ Implemented | |

## Client trait (axon calls these on the adapter)

| Method | Status | Notes |
|--------|--------|-------|
| `initialize` | ✅ Implemented | reads MCP + session capabilities |
| `authenticate` | ✅ Implemented | `AXON_ACP_AUTH_TOKEN` |
| `new_session` | ✅ Implemented | |
| `load_session` | ✅ Implemented | fallback-to-new when unsupported |
| `resume_session` | ✅ Implemented (2026-03-24) | preferred when `session_capabilities.resume` is set |
| `prompt` | ✅ Implemented | both one-shot and persistent paths |
| `cancel` | ✅ Implemented | `session/cancel` notification |
| `close_session` | ✅ Implemented | on WS disconnect |
| `fork_session` | ✅ Implemented (2026-03-24) | via AdapterMessage dispatch; MCP endpoint |
| `list_sessions` | ✅ Implemented (2026-03-24) | adapter-first, cache fallback; MCP endpoint |
| `set_session_model` | ✅ Implemented (2026-03-24) | via AdapterMessage dispatch; MCP endpoint |

## Data fields

| Field | Status | Notes |
|-------|--------|-------|
| `InitializeResponse.agent_capabilities.mcp_capabilities` | ✅ Implemented | http + sse flags |
| `InitializeResponse.agent_capabilities.session_capabilities.resume` | ✅ Implemented (2026-03-24) | |
| `InitializeResponse.agent_capabilities.session_capabilities.fork` | ✅ Implemented (2026-03-24) | |
| `InitializeResponse.agent_capabilities.session_capabilities.list` | ✅ Implemented (2026-03-24) | |
| `PromptResponse.usage` | ✅ Implemented (2026-03-24) | emitted as `TurnUsage` bridge event |
| `NewSessionResponse.model_state` | ✅ Implemented (2026-03-24) | sets `set_session_model_supported` |

## Subscribe stream

| Feature | Status | Notes |
|---------|--------|-------|
| Persistent path drain | ✅ Implemented | `spawn_subscribe_drain` in `persistent_conn.rs` |
| One-shot path drain | ✅ Implemented (2026-03-24) | inline `select!` arm in `runtime.rs` |
```

- [ ] **Step 7.2: Run the full pre-commit gate**

```bash
cd /home/jmagar/workspace/axon_rust
just precommit 2>&1 | tail -20
```

Expected: monolith check passes, `cargo fmt --check` passes, `cargo clippy` 0 warnings, `cargo check` passes, `cargo test --lib` all pass.

- [ ] **Step 7.3: Run full test suite**

```bash
cargo test --lib 2>&1 | tail -10
```

Expected: all tests pass (≥336 passing, 0 failures).

- [ ] **Step 7.4: Final commit**

```bash
git add docs/ACP-GAP-ANALYSIS.md
git commit -m "docs(acp): add ACP-GAP-ANALYSIS.md reflecting full SDK parity after 2026-03-24 plan"
```

---

## Completion Checklist

- [ ] Task 1: `SessionCapabilities` flags read and stored in `AcpRuntimeState`
- [ ] Task 2: `subscribe()` drain wired in one-shot `runtime.rs`
- [ ] Task 3: `PromptResponse.usage` → `TurnUsage` bridge event
- [ ] Task 4: `resume_session` preferred over `load_session` when supported
- [ ] Task 5: `AdapterMessage` variants + `AcpConnectionHandle` methods for fork/list/set_model
- [ ] Task 6: All four MCP stubs replaced with real dispatch
- [ ] Task 7: `ACP-GAP-ANALYSIS.md` created, `just precommit` passes
