# ACP Gap Closure — Design Spec
> axon_rust ACP implementation: items #1–#9 from gap analysis priority list
>
> Generated: 2026-03-23

---

## Table of Contents

1. [Scope](#scope)
2. [Already Implemented (Reference)](#already-implemented)
3. [Gap #1 — `close_session`](#gap-1--close_session)
4. [Gap #2 — Message Stream Observation](#gap-2--message-stream-observation)
5. [Gap #3 — `resume_session`](#gap-3--resume_session)
6. [Gap #4 — Session Usage Tracking](#gap-4--session-usage-tracking)
7. [Gap #5 — Boolean Config Options](#gap-5--boolean-config-options)
8. [Gap #6 — Message ID Tracking](#gap-6--message-id-tracking)
9. [Gap #7 — Extension Methods Skeleton](#gap-7--extension-methods-skeleton)
10. [Gap #8 — Authentication](#gap-8--authentication)
11. [Implementation Order](#implementation-order)
12. [Documentation Updates](#documentation-updates)
13. [Out of Scope](#out-of-scope)

---

## Scope

Close all non-terminal ACP protocol gaps in priority order (items #1–#9 from `ACP-GAP-ANALYSIS.md`). Terminal operations (#4 in the priority list) are explicitly deferred — they require PTY management, a TerminalRegistry, and a security sandbox. Frontend-blocked items (elicitation, list_sessions UI) are also deferred.

**Unstable feature status**: `agent-client-protocol = { version = "0.10.0", features = ["unstable"] }` is already present in `Cargo.toml` — all unstable flags are active. No Cargo changes required.

**Implementation strategy**: Sequential by priority. Each gap is a single focused commit with tests. `ACP-GAP-ANALYSIS.md` updated to ✅ after each gap.

---

## Already Implemented (Reference)

These are complete and not touched by this spec:

| Gap | Location |
|-----|----------|
| `CancelNotification` | `persistent_conn/turn.rs` — 15s grace + `CancellationToken` |
| `read_text_file` / `write_text_file` | `bridge.rs` — CWD-scoped, traversal-safe |
| `set_session_mode` | `persistent_conn/session_options.rs` — dedup + alias normalisation |

---

## Gap #1 — `close_session`

**Priority**: 1 | **Complexity**: Medium | **Feature flag**: `unstable_session_close` (active)

### What it does
Signals the adapter to terminate a session and free its resources. Currently axon relies on dropping the connection (EOF on stdin) then SIGKILLing after a timeout — the adapter gets no chance to write its session file or release server-side resources gracefully.

### Changes

**New helper** in `crates/services/acp/runtime.rs`:

```rust
/// Send `close_session` to the adapter and wait up to `timeout` for acknowledgement.
/// Best-effort: returns regardless of whether the adapter acknowledges.
/// `session_id_str` is the raw session ID string — `SessionId` is reconstructed internally.
async fn try_close_session(
    conn: &ClientSideConnection,
    session_id_str: &str,
    timeout: std::time::Duration,
) {
    let req = CloseSessionRequest::new(SessionId::new(session_id_str));
    let _ = tokio::time::timeout(timeout, conn.close_session(req)).await;
}
```

**One-shot path** (`runtime.rs` → `wait_for_adapter_exit`):

`wait_for_adapter_exit` already has `session_id_str: &str` in scope. Before the existing `drop(conn)` line, insert:

```rust
// Signal adapter to close session before dropping the connection.
try_close_session(&conn, session_id_str, std::time::Duration::from_secs(5)).await;
// Existing:
drop(conn);
drop(runtime_state);
```

The 10 s exit-wait budget is unchanged; the `close_session` RPC comes out of that budget (it is capped at 5 s within `try_close_session`).

**Persistent path** (`persistent_conn.rs` → `run_adapter_main_loop`):

In the `None` branch (channel closed = WS disconnected), `run_adapter_main_loop` has `session_id: &SessionId` (inner type `Arc<str>`). `try_close_session` takes `session_id_str: &str`. Use `session_id.0.as_ref()` to extract the `&str`, and `&*conn` to reborrow `&mut ClientSideConnection` as `&ClientSideConnection`:

```rust
None => {
    // WS disconnected — signal close before dropping connection.
    // &*conn: reborrow &mut as & for the shared borrow required by try_close_session.
    // session_id.0.as_ref(): SessionId wraps Arc<str>; .as_ref() gives &str.
    try_close_session(&*conn, session_id.0.as_ref(), std::time::Duration::from_secs(5)).await;
    break;
}
```

The helper `try_close_session` lives in `runtime.rs` and is `pub(super)` so `persistent_conn.rs` can call it (both are inside `crates/services/acp/`).

### State changes
None. `try_close_session` is stateless.

### Tests
- Unit test: `try_close_session` completes without panic — timeout fires gracefully because the mock adapter never responds.

---

## Gap #2 — Message Stream Observation

**Priority**: 2 | **Complexity**: Low

### What it does
`conn.subscribe()` returns a broadcast receiver delivering every JSON-RPC message in both directions. Used for wire-level debug tracing and protocol conformance visibility.

### Changes

**In `crates/services/acp/session.rs` → `initialize_connection`**, after the `ClientSideConnection` is created and before returning:

```rust
// Wire-level protocol tracing — zero overhead when tracing level > TRACE.
let mut wire_stream = conn.subscribe();
tokio::task::spawn_local(async move {
    while let Ok(msg) = wire_stream.recv().await {
        // StreamMessageContent is an enum; pattern-match to extract method name.
        let method: Option<&str> = match &msg.message {
            agent_client_protocol::StreamMessageContent::Request { method, .. } => Some(method.as_ref()),
            agent_client_protocol::StreamMessageContent::Notification { method, .. } => Some(method.as_ref()),
            agent_client_protocol::StreamMessageContent::Response { .. } => None,
        };
        tracing::trace!(
            direction = ?msg.direction,
            method = method,
            "acp_wire_message"
        );
    }
});
```

`StreamMessageContent` is an enum with `Request { id, method: Arc<str>, params }`, `Response { id, result }`, and `Notification { method: Arc<str>, params }` variants. There is **no `.method()` helper method**; pattern matching is required. The task is fire-and-forget. It drops when `wire_stream.recv()` returns `Err` (connection closed — sender dropped when `conn` is dropped).

### State changes
None. The spawned task holds only the `StreamReceiver` which is `!Send`-safe inside `LocalSet`.

### Tests
- Compile-time only: confirm `conn.subscribe()` satisfies trait bounds and the `spawn_local` compiles.
- Integration note: verified by any existing test that creates a `ClientSideConnection`.

---

## Gap #3 — `resume_session`

**Priority**: 3 | **Complexity**: Medium | **Feature flag**: `unstable_session_resume` (active)

### What it does
Reconnects to an existing session without replaying full history. Faster than `load_session` for sessions with many turns (>50). Falls back to `load_session` if the adapter doesn't support it.

### Changes

**New field** in `AcpRuntimeState` (`bridge.rs`):
```rust
/// Whether the adapter has successfully handled `resume_session` before.
/// None = unknown (first reconnect), true = use resume, false = fall back to load_session.
pub(super) resume_capable: std::cell::Cell<Option<bool>>,
```

**In `turn.rs` → `load_or_fallback_session`**, before calling `load_session`:

```rust
// Try resume_session first if adapter has previously supported it (or unknown on first try).
let should_try_resume = runtime_state.resume_capable.get().unwrap_or(true);
if should_try_resume {
    match conn.resume_session(ResumeSessionRequest::new(
        SessionId::new(requested_id),
        session_cwd.to_path_buf(),
    )).await {
        Ok(response) => {
            runtime_state.resume_capable.set(Some(true));
            // Apply config options from response (same path as load_session success).
            update_config_options_from_optional(&runtime_state, response.config_options).await;
            return Ok(SessionId::new(requested_id));
        }
        Err(e) if is_method_not_found(&e) => {
            // Adapter doesn't support resume_session — never try again for this session.
            runtime_state.resume_capable.set(Some(false));
            // Fall through to load_session below.
        }
        Err(_) => {
            // Transient error — fall through to load_session; retry resume next reconnect.
        }
    }
}
// Existing load_session call below, unchanged.
```

**Helper** `is_method_not_found(e: &acp::Error) -> bool` — checks the JSON-RPC error code for `-32601` (MethodNotFound).

### State changes
- `AcpRuntimeState::resume_capable: Cell<Option<bool>>` — `!Send`, `Cell` family, safe in `LocalSet`.

### Tests
- Unit test: success path sets `resume_capable = Some(true)`, returns session ID.
- Unit test: `MethodNotFound` error sets `resume_capable = Some(false)`, falls through to `load_session`.
- Unit test: other error leaves `resume_capable` unchanged (`Some(true)` from retry attempts), falls through to `load_session`.

---

## Gap #4 — Session Usage Tracking

**Priority**: 5 in gap analysis | **Complexity**: Low | **Feature flag**: `unstable_session_usage` (active)

### What it does
`PromptResponse::usage` (optional field, type `Usage`) carries per-turn token counts from the adapter. Emitting it enables accurate token billing visibility in Pulse Chat.

### SDK Types (verified from source)

`PromptResponse.usage: Option<Usage>` under `#[cfg(feature = "unstable_session_usage")]`.

`Usage` struct fields:
```rust
pub struct Usage {
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub thought_tokens: Option<u64>,
    pub cached_read_tokens: Option<u64>,
    pub cached_write_tokens: Option<u64>,
}
```

**Note**: The existing `AcpUsageUpdate` / `AcpBridgeEvent::UsageUpdate` maps `SessionUpdate::UsageUpdate` (context window stats: `used`, `size`, `cost`). Per-turn token counts from `PromptResponse::usage` are a **separate concept** and need a distinct wire type.

### Changes

**New type** in `crates/services/types/acp.rs`:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AcpTurnUsageUpdate {
    pub session_id: String,
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub thought_tokens: Option<u64>,
    pub cached_read_tokens: Option<u64>,
    pub cached_write_tokens: Option<u64>,
}
```

**New variant** in `AcpBridgeEvent`:

```rust
TurnUsageUpdate(AcpTurnUsageUpdate),
```

**In `bridge.rs` → `finalize_successful_turn`**: add `usage: Option<Usage>` parameter (under `#[cfg(feature = "unstable_session_usage")]`). After the existing `TurnResult` emit:

```rust
#[cfg(feature = "unstable_session_usage")]
if let Some(u) = usage {
    emit_nonblocking(
        service_tx,
        ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::TurnUsageUpdate(AcpTurnUsageUpdate {
                session_id: session_id.to_string(),
                total_tokens: u.total_tokens,
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
                thought_tokens: u.thought_tokens,
                cached_read_tokens: u.cached_read_tokens,
                cached_write_tokens: u.cached_write_tokens,
            }),
        },
    );
}
```

**Call site update in `runtime.rs`** (one-shot path, current line ~273):

```rust
// Before (current):
finalize_successful_turn(prompt_response.stop_reason, &runtime_state, &tx, &session).await;

// After:
let agent_client_protocol::PromptResponse { stop_reason, #[cfg(feature = "unstable_session_usage")] usage, .. } = prompt_response;
finalize_successful_turn(stop_reason, #[cfg(feature = "unstable_session_usage")] usage, &runtime_state, &tx, &session).await;
```

**Call site update in `persistent_conn/turn.rs`** (persistent path `run_prompt` or equivalent):

```rust
// Same pattern — destructure before passing:
let agent_client_protocol::PromptResponse { stop_reason, #[cfg(feature = "unstable_session_usage")] usage, .. } = response;
finalize_successful_turn(stop_reason, #[cfg(feature = "unstable_session_usage")] usage, &runtime_state, &tx, &session_id).await;
```

### Tests
- Unit test: `finalize_successful_turn` emits `TurnUsageUpdate` when `usage = Some(...)`.
- Unit test: no `TurnUsageUpdate` emitted when `usage = None`.

---

## Gap #5 — Boolean Config Options

**Priority**: 6 in gap analysis | **Complexity**: Low | **Feature flag**: `unstable_boolean_config` (active)

### What it does
Adds a `boolean` variant to `SessionConfigOption`. Adapters can expose toggle-style settings (e.g. "Enable extended thinking"). Without this, axon silently drops boolean options from the config options list because `map_config_options` only processes `Select` variants.

### Changes

**`AcpConfigOption` type** in `crates/services/types/acp.rs`:
```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ConfigOptionKind {
    Select,
    Boolean,
}

pub struct AcpConfigOption {
    // existing fields unchanged...
    #[serde(skip)]   // client-side classification; not forwarded on WS wire
    pub kind: ConfigOptionKind,
}
```

`#[serde(skip)]` is **required** on `kind` to avoid breaking the existing WebSocket wire format that frontends already parse.

**`map_config_options`** in `mapping.rs`: extend to handle boolean options from `SessionConfigKind::Boolean`. Boolean options have no `options` list; set `options: vec![]` and `current_value: ""` (or the boolean's current value serialised as `"true"`/`"false"`).

**New helper** `apply_boolean_config_option(conn: &ClientSideConnection, session_id: &SessionId, option_id: &str, value: bool)` in `session_options.rs` for future callers. Not wired to any turn request field — boolean config is set-and-forget at session setup time, not per-turn.

### Tests
- Unit test: `map_config_options` sets `kind = ConfigOptionKind::Boolean` for boolean SDK options and `ConfigOptionKind::Select` for select options.
- Unit test: `AcpConfigOption` serialises to JSON without the `kind` field (confirming `#[serde(skip)]` works).

---

## Gap #6 — Message ID Tracking

**Priority**: 7 in gap analysis | **Complexity**: Low | **Feature flag**: `unstable_message_id` (active)

### What it does
The SDK adds `message_id: Option<String>` to `ContentChunk` (under `unstable_message_id`). `ContentChunk` is the payload type inside `SessionUpdate::UserMessageChunk`, `AgentMessageChunk`, and `AgentThoughtChunk`. When present, this ID enables exactly-once replay after WS reconnect by deduplicating the replay buffer.

### SDK type location (verified)
`ContentChunk.message_id: Option<String>` at line 338–340 of the schema crate (`agent-client-protocol/src/client.rs`), under `#[cfg(feature = "unstable_message_id")]`. It is on `ContentChunk`, **not** on `SessionNotification`.

### Replay buffer facts (verified from `session_cache/entry.rs`)
- Buffer type: `Vec<String>` — serialized JSON strings, **not** `Vec<ServiceEvent>`
- Struct: `CachedSession` (not `SessionEntry`)
- State: `ReplayBufferState { messages: Vec<String>, total_bytes: usize }`
- Insertion: `buffer_event(json: String)` enforces count cap (4096) and byte cap (4 MiB)

### Changes

**`crates/services/acp/session_cache/entry.rs`**: extend `ReplayBufferState` with a dedup set wrapped in a `Mutex` (matching the existing inner-mutability pattern — `CachedSession` is stored behind `Arc`, so all mutation must go through `&self` + lock):

```rust
struct ReplayBufferState {
    messages: Vec<String>,
    total_bytes: usize,
    #[cfg(feature = "unstable_message_id")]
    seen_message_ids: std::collections::HashSet<String>,
}
```

`seen_message_ids` lives inside `ReplayBufferState`, which is already accessed via `self.replay_buffer.lock()`. No additional `Mutex` wrapping needed — the existing lock covers both fields.

**`buffer_event` signature change** — currently returns `()`, will return `bool` to signal whether the event was buffered (false = duplicate or cap exceeded). This is a **breaking change** to all call sites. All callers currently ignore the return value so the change is backward-compatible in practice, but each call site must be updated to pass the new `message_id` parameter.

```rust
// Before (current, entry.rs:60):
pub fn buffer_event(&self, json: String) { ... }

// After:
pub fn buffer_event(&self, json: String, message_id: Option<String>) -> bool {
    let mut state = self.replay_buffer.lock().expect("replay buffer lock poisoned");
    // Dedup: skip if we've seen this message_id before.
    #[cfg(feature = "unstable_message_id")]
    if let Some(ref id) = message_id {
        if state.seen_message_ids.contains(id) {
            return false; // duplicate, not buffered
        }
    }
    // Existing byte/count cap checks on state.messages and state.total_bytes...
    let json_len = json.len();
    if state.messages.len() >= MAX_REPLAY_BUFFER || state.total_bytes + json_len > MAX_REPLAY_BUFFER_BYTES {
        return false;
    }
    state.messages.push(json);
    state.total_bytes += json_len;
    #[cfg(feature = "unstable_message_id")]
    if let Some(id) = message_id {
        state.seen_message_ids.insert(id);
    }
    true
}
```

**All call sites** that must be updated (add `message_id` parameter; `_` or ignore the `bool` return):

| File | Line | Current call | Updated call |
|------|------|-------------|-------------|
| `crates/web/execute/sync_mode/pulse_chat/events.rs` | 97 | `cached.buffer_event(msg)` | `cached.buffer_event(msg, None)` (no message_id at web layer) |
| `crates/services/acp/session_cache.rs` | 136, 137, 157–158, 175, 191, 211, 235, 243 | `session.buffer_event(json)` | `session.buffer_event(json, None)` (tests; no message_id) |
| `crates/services/acp/bridge.rs` | new | (new call in `session_notification`) | `session.buffer_event(json, message_id)` — passes extracted `message_id` |

`crates/services/acp/session_cache/cache.rs` does **not** need changes — its `insert` function manages session-map entries and has no involvement in the replay buffer write path.

**`crates/services/acp/bridge.rs` → `session_notification`**: extract `message_id` from chunk variants and pass to `buffer_event`:

```rust
#[cfg(feature = "unstable_message_id")]
let message_id: Option<String> = match &args.update {
    SessionUpdate::UserMessageChunk(c)
    | SessionUpdate::AgentMessageChunk(c)
    | SessionUpdate::AgentThoughtChunk(c) => c.message_id.clone(),
    _ => None,
};
#[cfg(not(feature = "unstable_message_id"))]
let message_id: Option<String> = None;
// Pass message_id to cache insert.
```

### State changes
`ReplayBufferState` gains `seen_message_ids: HashSet<String>` under the feature flag. Size is bounded by `MAX_REPLAY_BUFFER` (4096) — same cap as `messages`.

### Tests
- Unit test: inserting the same `message_id` twice results in one entry in the buffer.
- Unit test: notifications without `message_id` are always inserted (no accidental dedup).
- Unit test: buffer byte cap and count cap still enforced correctly after dedup logic added.

---

## Gap #7 — Extension Methods Skeleton

**Priority**: 8 in gap analysis | **Complexity**: Low

### What it does
Implements the `ext_method` and `ext_notification` callbacks on `AcpBridgeClient` so the ACP `Client` trait is fully satisfied and the protocol error path is correct for unknown extensions.

### SDK default behaviour (verified)
The SDK provides default implementations that return `Ok(ExtResponse::new(null))` for `ext_method` and `Ok(())` for `ext_notification`. Overriding with `method_not_found` for `ext_method` is more correct: the client should reject unexpected RPC-style extension methods rather than silently returning null.

### Changes

**`bridge.rs` → `impl Client for AcpBridgeClient`**:

```rust
async fn ext_method(
    &self,
    args: agent_client_protocol::ExtRequest,
) -> agent_client_protocol::Result<agent_client_protocol::ExtResponse> {
    tracing::debug!(method = %args.method, "acp_ext_method (unhandled)");
    Err(agent_client_protocol::Error::method_not_found())
}

async fn ext_notification(
    &self,
    args: agent_client_protocol::ExtNotification,
) -> agent_client_protocol::Result<()> {
    tracing::debug!(method = %args.method, "acp_ext_notification (unhandled)");
    Ok(())  // Notifications are fire-and-forget; silently accept unknown ones to avoid
            // breaking future protocol extensions where axon is a client.
}
```

### Tests
- Unit test: `ext_method` with unknown method name returns `Err` with method_not_found error code.
- Unit test: `ext_notification` with unknown method name returns `Ok(())`.

---

## Gap #8 — Authentication

**Priority**: 9 in gap analysis | **Complexity**: Medium

### What it does
After `initialize`, check `InitializeResponse::auth_methods`. If the adapter advertises any auth methods and `AXON_ACP_REQUIRE_AUTH=true`, send `AuthenticateRequest` with credentials from `AXON_ACP_AUTH_TOKEN`. Fail the session if authentication fails. Backward-compatible by default.

### SDK type (verified)
`InitializeResponse.auth_methods: Vec<AuthMethod>` (plural, `#[serde(default)]`). Check `!init_response.auth_methods.is_empty()` — **not** `init_response.auth_method.is_some()` (that field does not exist).

### Changes

**`session.rs` → `initialize_connection`**, after receiving `init_response`:

```rust
handle_auth_if_required(&conn, &init_response, tx).await?;
```

**New function** `handle_auth_if_required` in `session.rs`:

```rust
async fn handle_auth_if_required(
    conn: &ClientSideConnection,
    init_response: &InitializeResponse,
    tx: &Option<mpsc::Sender<ServiceEvent>>,
) -> Result<(), String> {
    // Gate 1: adapter must advertise at least one auth method.
    if init_response.auth_methods.is_empty() {
        return Ok(());
    }
    // Gate 2: axon must be configured to require auth.
    if std::env::var("AXON_ACP_REQUIRE_AUTH").as_deref() != Ok("true") {
        tracing::debug!("adapter requests auth but AXON_ACP_REQUIRE_AUTH is not set; skipping");
        return Ok(());
    }
    // Lookup credential.
    let token = std::env::var("AXON_ACP_AUTH_TOKEN").unwrap_or_default();
    if token.is_empty() {
        return Err("AXON_ACP_REQUIRE_AUTH=true but AXON_ACP_AUTH_TOKEN is not set".to_string());
    }
    emit(tx, ServiceEvent::Log {
        level: LogLevel::Info,
        message: "ACP runtime: sending authenticate request".to_string(),
    }).await;
    let auth_resp = conn
        .authenticate(AuthenticateRequest::new(token))
        .await
        .map_err(|e| format!("ACP authenticate RPC failed: {e}"))?;
    if !auth_resp.success {
        return Err(format!(
            "ACP adapter rejected authentication: {}",
            auth_resp.message.as_deref().unwrap_or("no message")
        ));
    }
    Ok(())
}
```

### New env vars

| Var | Default | Purpose |
|-----|---------|---------|
| `AXON_ACP_REQUIRE_AUTH` | unset (= false) | Set to `true` to enforce auth handshake |
| `AXON_ACP_AUTH_TOKEN` | unset | Bearer token sent in `AuthenticateRequest` |

### Tests
- Unit test: empty `auth_methods` → `Ok(())`, no RPC sent.
- Unit test: non-empty `auth_methods`, `AXON_ACP_REQUIRE_AUTH` unset → `Ok(())`, no RPC sent.
- Unit test: non-empty `auth_methods`, `REQUIRE_AUTH=true`, `AUTH_TOKEN` unset → `Err`.
- Unit test: non-empty `auth_methods`, `REQUIRE_AUTH=true`, `AUTH_TOKEN` set, response `success=false` → `Err`.
- Unit test: non-empty `auth_methods`, `REQUIRE_AUTH=true`, `AUTH_TOKEN` set, response `success=true` → `Ok(())`.

---

## Implementation Order

| # | Gap | File(s) | Complexity |
|---|-----|---------|-----------|
| 1 | `close_session` | `runtime.rs`, `persistent_conn.rs` | Medium |
| 2 | Message stream observation | `session.rs` | Low (10 lines) |
| 3 | `resume_session` | `turn.rs`, `bridge.rs` | Medium |
| 4 | Session usage tracking | `types/acp.rs`, `bridge.rs`, `runtime.rs`, `turn.rs` | Low |
| 5 | Boolean config options | `types/acp.rs`, `mapping.rs`, `session_options.rs` | Low |
| 6 | Message ID tracking | `session_cache/entry.rs`, `bridge.rs` | Low |
| 7 | Extension methods skeleton | `bridge.rs` | Low (8 lines) |
| 8 | Authentication | `session.rs` | Medium |

---

## Documentation Updates

After each gap is implemented:
1. Mark the corresponding row in `ACP-GAP-ANALYSIS.md` as ✅ with a one-line "as shipped" note
2. Add new env vars (`AXON_ACP_REQUIRE_AUTH`, `AXON_ACP_AUTH_TOKEN`) to `docs/ACP.md` env var table and `.env.example`
3. Update `crates/services/CLAUDE.md` if `AcpRuntimeState` fields or `AcpBridgeEvent` variants change
4. Session cache changes (Gap #6): update the replay buffer description in `crates/services/CLAUDE.md`

---

## Out of Scope

| Item | Reason |
|------|--------|
| Terminal operations (`create_terminal` etc.) | Requires PTY, TerminalRegistry, security sandbox |
| `fork_session` | Needs session graph in cache — follow-on |
| `list_sessions` | Blocked on Pulse Chat frontend work |
| Elicitation | Requires frontend form renderer |
| `set_session_model` (unstable) | `SetSessionConfigOptionRequest` fallback still works; low urgency |
| Protocol version negotiation | V1 is current; add version check when V2 ships |

---

*End of spec*
