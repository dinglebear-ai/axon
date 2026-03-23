# ACP Rust SDK Gap Analysis
> axon_rust vs. `agent-client-protocol` v0.10.2
>
> Generated: 2026-03-23

---

## Table of Contents

1. [Overview](#overview)
2. [Role Mapping — Who Is Who](#role-mapping)
3. [What axon Currently Implements](#what-axon-currently-implements)
4. [Gap Matrix — Client Trait (Agent → Axon Callbacks)](#gap-matrix-client-trait)
5. [Gap Matrix — Agent Trait (Axon → Agent Calls)](#gap-matrix-agent-trait)
6. [Gap Matrix — Unstable Protocol Features](#gap-matrix-unstable)
7. [Gap Matrix — SDK Infrastructure Features](#gap-matrix-sdk-infrastructure)
8. [Detailed Gap Analysis](#detailed-gap-analysis)
   - [Terminal Operations](#1-terminal-operations)
   - [File System Access](#2-file-system-access-requestresponse)
   - [Session Modes](#3-session-modes)
   - [Authentication](#4-authentication)
   - [CancelNotification](#5-cancelnotification)
   - [Session Fork / Resume / Close / List](#6-session-fork--resume--close--list-unstable)
   - [Extension Methods and Notifications](#7-extension-methods-and-notifications)
   - [Message Stream Observation](#8-message-stream-observation)
   - [Per-Session Model Selection (Unstable)](#9-per-session-model-selection-unstable)
   - [Session Usage Tracking (Unstable)](#10-session-usage-tracking-unstable)
   - [Elicitation (Unstable)](#11-elicitation-unstable)
   - [Boolean Config Options (Unstable)](#12-boolean-config-options-unstable)
   - [Message ID Tracking (Unstable)](#13-message-id-tracking-unstable)
   - [MCP Server Management](#14-mcp-server-management)
9. [Priority Ranking](#priority-ranking)
10. [Implementation Notes](#implementation-notes)

---

## Overview

axon_rust uses the ACP (Agent Client Protocol) SDK to communicate with agent adapter subprocesses (Claude Code, Codex, Gemini). axon acts as the **client** — it spawns the adapter as a child process and drives it over stdin/stdout JSON-RPC.

This document maps every feature exposed by the `agent-client-protocol` v0.10.2 Rust SDK against what axon currently implements, and describes the functional value of each gap.

**SDK version**: `agent-client-protocol = 0.10.2`
**Schema version**: `agent-client-protocol-schema = 0.11.3`
**axon ACP entry points**: `crates/services/acp/`, `crates/services/acp_llm/`

---

## Role Mapping

In ACP protocol terms:

| SDK Role | axon Component | Description |
|----------|---------------|-------------|
| **Client** | `AcpBridgeClient` (implements `Client` trait) | Receives callbacks from the adapter; handles permission requests, session notifications, file/terminal ops |
| **Agent** | Subprocess adapter (`claude-agent-acp`, `codex-acp`, `gemini`) | Implements ACP `Agent` trait; receives prompts, runs LLM, calls tools |
| **ClientSideConnection** | Created in `acp/runtime.rs` and `acp/persistent_conn.rs` | Owns the stdin/stdout I/O loop; implements `Agent` so axon can call adapter methods |
| **AgentSideConnection** | Lives inside the adapter subprocess | Implements `Client` so the adapter can call axon back |

**axon drives the session.** The adapter receives it. Callbacks flow from adapter → axon via `AcpBridgeClient`.

---

## What axon Currently Implements

### Client Trait (AcpBridgeClient — adapter calls axon)

| Method | Status | Notes |
|--------|--------|-------|
| `request_permission(RequestPermissionRequest)` | ✅ **Implemented** | Full permission gating via `PermissionResponderMap`, DashMap, oneshot channels, 60s timeout, auto-approve mode |
| `session_notification(SessionNotification)` | ✅ **Implemented** | Maps all `SessionUpdate` variants to `ServiceEvent`; streams to WS or collects for CLI |
| `write_text_file(WriteTextFileRequest)` | ✅ **Implemented** | CWD-scoped path validation (no `../` traversal); `tokio::fs::write`; emits `ServiceEvent::Log` audit event |
| `read_text_file(ReadTextFileRequest)` | ✅ **Implemented** | Same CWD validation; `tokio::fs::read_to_string`; returns `Error::resource_not_found` on missing path |
| `create_terminal(CreateTerminalRequest)` | ❌ **Not implemented** | No terminal management layer |
| `terminal_output(TerminalOutputRequest)` | ❌ **Not implemented** | — |
| `release_terminal(ReleaseTerminalRequest)` | ❌ **Not implemented** | — |
| `wait_for_terminal_exit(WaitForTerminalExitRequest)` | ❌ **Not implemented** | — |
| `kill_terminal(KillTerminalRequest)` | ❌ **Not implemented** | — |
| `ext_method(ExtRequest)` | ❌ **Not implemented** | Custom protocol extension requests not handled |
| `ext_notification(ExtNotification)` | ❌ **Not implemented** | Custom one-way notifications not handled |

### Agent Calls (ClientSideConnection — axon calls the adapter)

| Method | Status | Notes |
|--------|--------|-------|
| `initialize(InitializeRequest)` | ✅ **Implemented** | Sent on every adapter spawn; protocol version V1 |
| `new_session(NewSessionRequest)` | ✅ **Implemented** | Both one-shot and persistent mode |
| `load_session(LoadSessionRequest)` | ✅ **Implemented** | Persistent mode on reconnect; fallback to new on error |
| `prompt(PromptRequest)` | ✅ **Implemented** | Full prompt execution with biased `select!` exit-watcher |
| `set_session_config_option(SetSessionConfigOptionRequest)` | ✅ **Implemented** | Model/config changes, validated against allowed values |
| `authenticate(AuthenticateRequest)` | ❌ **Not called** | No authentication handshake implemented |
| `set_session_mode(SetSessionModeRequest)` | ✅ **Implemented** | Sent via `apply_requested_mode_before_prompt`; deduped via `current_mode` in `AcpRuntimeState`; hyphen/underscore alias normalisation |
| `list_sessions(ListSessionsRequest)` | ❌ **Not called** | Unstable — not used |
| `cancel(CancelNotification)` | ✅ **Implemented** | Persistent mode: `CancellationToken` on WS disconnect sends `cancel` → waits up to 15 s for `Cancelled` response → SIGKILL only as last resort |
| `fork_session(ForkSessionRequest)` | ❌ **Not called** | Unstable — not used |
| `resume_session(ResumeSessionRequest)` | ❌ **Not called** | Unstable — not used (`load_session` is the current resumption path) |
| `close_session(CloseSessionRequest)` | ❌ **Not called** | Unstable — adapter teardown relies on SIGKILL |
| `set_session_model(SetSessionModelRequest)` | ❌ **Not called** | Unstable — model is set via `SetSessionConfigOptionRequest` instead |
| `new_session(NewSessionRequest{mcp_servers})` | ✅ **Implemented** | Stdio + Http + Sse; capability-filtered via `McpCapabilities` from `InitializeResponse` |
| `load_session(LoadSessionRequest{mcp_servers})` | ✅ **Implemented** | MCP servers passed through; preserved on fallback to `new_session` (one-shot + persistent paths) |
| `ext_method(ExtRequest)` | ❌ **Not called** | Custom protocol extensions not sent |
| `ext_notification(ExtNotification)` | ❌ **Not called** | Custom one-way messages not sent |

---

## Gap Matrix — Client Trait

> These are operations the **adapter calls on axon**. axon must implement them.

| Method | Gap Type | Value If Implemented | Complexity |
|--------|----------|---------------------|------------|
| `write_text_file` | ~~Capability gap~~ ✅ **Implemented** | Adapter can create/modify files in axon's workspace | — |
| `read_text_file` | ~~Capability gap~~ ✅ **Implemented** | Adapter can read files without a separate tool call | — |
| `create_terminal` | Capability gap | Adapter can run shell commands via axon's terminal | High |
| `terminal_output` | Capability gap (depends on above) | Adapter can poll command output / exit code | High |
| `release_terminal` | Capability gap (depends on above) | Adapter signals command done, releases resources | High |
| `wait_for_terminal_exit` | Capability gap (depends on above) | Adapter blocks until command exits — enables scripted flows | High |
| `kill_terminal` | Capability gap (depends on above) | Adapter can abort runaway commands | High |
| `ext_method` | Extensibility gap | Custom axon-specific agent ↔ client RPC | Low |
| `ext_notification` | Extensibility gap | Custom one-way events (telemetry, diagnostics) | Low |

---

## Gap Matrix — Agent Trait

> These are operations **axon calls on the adapter**.

| Method | Gap Type | Value If Implemented | Complexity |
|--------|----------|---------------------|------------|
| `authenticate` | Security gap | Lets adapter verify axon's identity before accepting prompts | Low–Medium |
| `set_session_mode` | ~~UX gap~~ ✅ **Implemented** | Switch adapter into ask/architect/code behavior mid-session | — |
| `cancel` | ~~Correctness gap~~ ✅ **Implemented** | Clean cancellation via ACP notification; 15 s grace window before SIGKILL | — |
| `close_session` | Lifecycle gap | Graceful teardown frees adapter-side resources without SIGKILL | Medium |
| `list_sessions` | Discovery gap | Enumerate persistent sessions across adapter restarts | Low |
| `fork_session` | Collaboration gap | Clone session for A/B comparison or parallel exploration | Medium |
| `resume_session` | Performance gap | Reconnect without replaying full history (faster than load) | Medium |
| `set_session_model` | UX gap | Switch model mid-session via dedicated RPC (vs. config option hack) | Low |
| `ext_method` | Extensibility gap | Send custom requests to adapter | Low |
| `ext_notification` | Extensibility gap | Send custom one-way messages to adapter | Low |

---

## Gap Matrix — Unstable Protocol Features

> All behind `unstable_*` feature flags in the SDK. Stable once protocol ratifies them.

| Feature Flag | Enables | axon Status |
|-------------|---------|-------------|
| `unstable_auth_methods` | Extended authentication negotiation | ❌ Not used |
| `unstable_cancel_request` | Structured cancel with `CancelRequest` type | ❌ Not used |
| `unstable_elicitation` | Adapter prompts user for structured input | ❌ Not used |
| `unstable_logout` | Client can log out / clear credentials | ❌ Not used |
| `unstable_session_fork` | `fork_session` RPC | ❌ Not used |
| `unstable_session_close` | `close_session` RPC | ❌ Not used |
| `unstable_session_model` | `set_session_model` RPC | ❌ Not used |
| `unstable_session_resume` | `resume_session` RPC | ❌ Not used |
| `unstable_session_usage` | Usage metadata on prompt responses | ❌ Not used |
| `unstable_message_id` | Per-message ID tracking | ❌ Not used |
| `unstable_boolean_config` | Boolean config option type | ❌ Not used |

---

## Gap Matrix — SDK Infrastructure Features

| Feature | SDK Mechanism | axon Status |
|---------|--------------|-------------|
| Message stream observation | `conn.subscribe()` → `StreamReceiver` | ❌ Not used |
| Protocol version negotiation | `InitializeRequest` with version field | ⚠️ V1 hardcoded; no multi-version handling |
| Graceful cancel via notification | `conn.cancel(CancelNotification)` | ✅ Implemented — persistent mode sends cancel, waits 15 s, then SIGKILL |
| `!Send` trait compliance | `LocalSet` + `current_thread` runtime | ✅ Correctly isolated |
| `Rc<T>` / `Arc<T>` blanket impls | Automatic for `Agent` + `Client` types | ⚠️ `Arc<RefCell<...>>` used directly, not via blanket |

---

## Detailed Gap Analysis

### 1. Terminal Operations

**SDK methods**: `create_terminal`, `terminal_output`, `release_terminal`, `wait_for_terminal_exit`, `kill_terminal`

**What they do**: These let the adapter subprocess run shell commands *inside the client* (axon's) environment. The adapter calls `create_terminal` to open a shell, `wait_for_terminal_exit` to block until the command finishes, `terminal_output` to read stdout/stderr, and `release_terminal`/`kill_terminal` to clean up.

**Why it matters for axon**: Currently, axon's adapters can only use MCP tools to run commands — this goes through an extra layer (MCP server → tool dispatch → result). With terminal support in `AcpBridgeClient`, Claude Code could run `cargo test`, `git diff`, `axon crawl` etc. directly via axon's process environment with proper PTY-style output, which is exactly what Claude Code does when used in a terminal session.

**Current workaround**: None. Adapters relying on terminal access fall back to MCP tools if configured, or the terminal call is rejected, degrading agent effectiveness.

**Implementation path**:

```rust
// In AcpBridgeClient (crates/services/acp/bridge.rs)
async fn create_terminal(&self, args: CreateTerminalRequest) -> Result<CreateTerminalResponse> {
    // 1. Spawn child process (with PTY or piped stdio)
    // 2. Store in TerminalRegistry keyed by terminal_id
    // 3. Return CreateTerminalResponse { terminal_id }
}

async fn terminal_output(&self, args: TerminalOutputRequest) -> Result<TerminalOutputResponse> {
    // 1. Look up terminal_id in registry
    // 2. Read buffered output (stdout + stderr) since last poll
    // 3. Return exit code if process has exited
}
```

**New state required**:
- `TerminalRegistry: Arc<DashMap<TerminalId, TerminalEntry>>` — shared between all methods
- `TerminalEntry { child: Child, stdout_buf: Arc<Mutex<Vec<u8>>>, exit_code: Option<i32> }`
- Background task per terminal to drain stdout/stderr into buffer

**Security considerations**: Terminal commands execute with axon's process permissions. Must validate `CreateTerminalRequest::command` against the same blocklist used for adapter commands. Consider a separate per-session `allowed_commands` whitelist.

---

### 2. File System Access (Request/Response)

> **Status: ✅ Implemented** (`crates/services/acp/bridge.rs`)

**SDK methods**: `write_text_file`, `read_text_file`

**What they do**: These are *request-response* file operations where the adapter asks axon to read or write a specific file path and axon confirms. This is different from `SessionNotification::EditorWrite` (which is a one-way notification that the adapter wrote something — axon doesn't gate it).

**Distinction from EditorWrite**:
- `SessionNotification(SessionUpdate::EditorWrite { ... })` — notification, no axon approval
- `write_text_file(WriteTextFileRequest)` — blocking request; axon must respond before adapter continues

**Implementation (as shipped)**:

- `validate_fs_path(cwd, path)` — pure function; normalises `..` and `.` components without `canonicalize()` (works for paths that don't exist yet); rejects anything outside session CWD with `Error::internal_error()`
- `read_text_file` — calls `tokio::fs::read_to_string`; maps `io::Error` to `Error::resource_not_found(Some(path))`
- `write_text_file` — calls `tokio::fs::write`; emits `ServiceEvent::Log { Info, "ACP adapter wrote file: …" }` for audit trail before writing
- Both functions receive `session_cwd` from `AcpBridgeClient::session_cwd: PathBuf`, which is populated from `AcpSessionSetupRequest::cwd` via `initialize_connection`

**Security**: Path traversal blocked at the normalisation layer — no `../` escape possible. Absolute paths within CWD are accepted; absolute paths outside CWD are rejected.

---

### 3. Session Modes

> **Status: ✅ Implemented** (`crates/services/acp/persistent_conn/session_options.rs`)

**SDK method**: `set_session_mode(SetSessionModeRequest)`

**What it does**: Switches the adapter's behavior mode. Standard modes are `ask` (read-only Q&A), `architect` (planning without edits), `code` (full agentic mode). The adapter advertises its supported modes in the `NewSessionResponse`.

**Implementation (as shipped)**:

- `apply_requested_mode_before_prompt` called in `persistent_conn/turn.rs` before each `PromptRequest`
- Deduplication: `AcpRuntimeState::current_mode: RefCell<Option<String>>` tracks the adapter's current mode; skips the RPC if already in the requested mode
- `resolve_mode_option_for_request` validates the requested mode against the adapter's advertised options; falls back conservatively (does not apply) if no mode option is known
- Alias normalisation: hyphen/underscore and case differences are handled (e.g., `"accept-edits"` matches `"accept_edits"`)
- Uses `SetSessionModeRequest::new(session_id, mode_string)` — not the `SetSessionConfigOptionRequest` workaround
- `session_mode: Option<String>` field on `AcpPromptTurnRequest` is the entry point from Pulse Chat / CLI

---

### 4. Authentication

**SDK method**: `authenticate(AuthenticateRequest)`

**What it does**: After `initialize`, the adapter may indicate it requires authentication (in `InitializeResponse::auth_method`). The client sends credentials via `authenticate`. The adapter returns `AuthenticateResponse` indicating success/failure.

**Why it matters for axon**: Without authentication, axon accepts any adapter that speaks the protocol. An `authenticate` flow would:
1. Prevent rogue processes from masquerading as an ACP adapter
2. Enable future API key or token-based adapter authentication
3. Make the integration auditable

**Current state**: axon sends `initialize` and proceeds regardless of what `InitializeResponse::auth_method` contains.

**Implementation path**: Read `init_response.auth_method`. If present and non-null, lookup credentials from env/keychain, send `AuthenticateRequest`. Fail the session if `AuthenticateResponse::success` is false.

**Risk**: Implementing this could break existing adapters that don't implement authentication. Gate behind `AXON_ACP_REQUIRE_AUTH=false` env var.

---

### 5. CancelNotification

> **Status: ✅ Implemented** (`crates/services/acp/persistent_conn/turn.rs`, `persistent_conn.rs`)

**SDK method**: `conn.cancel(CancelNotification)`

**What it does**: Signals the adapter to stop the current prompt turn cleanly. The adapter aborts in-flight LLM requests, stops tool execution, and returns a `PromptResponse` with `StopReason::Cancelled`. The adapter then flushes its session state.

**Implementation (as shipped)**:

- `AcpConnectionHandle` owns a `CancellationToken`; `Drop` impl calls `cancel_token.cancel()` so WS disconnect automatically triggers the cancel path
- A child token (`loop_cancel`) is threaded through `adapter_loop` → `run_adapter_main_loop` → `run_turn_on_conn` → `run_prompt`
- In `run_prompt`, the prompt future is pinned (`tokio::pin!`) and raced in a `select!`:
  - **Cancel branch**: sends `conn.cancel(CancelNotification::new(session_id))`, then awaits the pinned prompt future for up to **15 seconds** for a clean `PromptResponse`; only returns an error string if the 15 s window expires (SIGKILL fires when the handle drops)
  - **Prompt branch**: completes normally if no cancellation arrives
- One-shot mode (`runtime.rs`) is unchanged — it already waits 10 s for adapter exit after stdin close; cancellation there is triggered by the caller dropping the future

**Remaining gap**: One-shot mode does not send `CancelNotification` — it still relies on stdin-close → adapter exit. This is acceptable for one-shot because no persistent session state can be lost (fresh spawn per turn).

---

### 6. Session Fork / Resume / Close / List (Unstable)

These four `Agent` trait methods are all behind `unstable_*` feature flags. Enable in `Cargo.toml`:

```toml
agent-client-protocol = { version = "0.10.2", features = ["unstable_session_fork", "unstable_session_resume", "unstable_session_close"] }
```

#### `fork_session`

Creates a copy of an existing session including its history. Useful for:
- A/B testing different prompts from the same context
- Spawning parallel sub-agents exploring different solutions
- Letting the user "branch" a conversation

axon does not implement this. The session cache architecture (TTL, replay buffer) could be extended to hold fork relationships.

#### `resume_session`

Resumes a session *without* replaying its full history. The adapter already has the session in memory (or can re-derive it cheaply). Faster than `load_session` which must replay every message.

axon uses `load_session` for reconnect — which forces the adapter to re-process the full history. For long sessions (>50 turns), `resume_session` would significantly reduce reconnect latency.

**Implementation**: Before `load_session`, try `resume_session` first. Fall back to `load_session` on `MethodNotFound` error (adapter doesn't support it). Cache `session_ids` that successfully resumed vs. required load.

#### `close_session`

Signals the adapter to terminate a session and free its resources (in-memory conversation history, model context window). Currently axon relies on SIGKILL — the adapter gets no chance to write its session file or release server-side resources.

**Implementation**: Call `close_session` before dropping `ClientSideConnection`. Give adapter 5s to respond before falling back to SIGKILL.

#### `list_sessions`

Returns all sessions the adapter knows about (persisted on disk). Enables:
- Session browser UI in Pulse Chat
- Resuming arbitrary historical sessions, not just the most recent
- Session cleanup / pruning old sessions

**Implementation**: Add an MCP tool or Pulse Chat WS message type that calls `conn.list_sessions()` and returns results to the frontend.

---

### 7. Extension Methods and Notifications

**SDK methods**: `ext_method(ExtRequest)`, `ext_notification(ExtNotification)` (both `Client` and `Agent` sides)

**What they do**: Allow axon-specific protocol extensions that don't belong in the standard ACP spec. Extension method names are prefixed with `_` on the wire.

**Use cases for axon**:

| Extension | Direction | Purpose |
|-----------|-----------|---------|
| `_axon.index_file` | Axon → Adapter | Tell adapter to embed the file it just wrote into Qdrant |
| `_axon.crawl_url` | Axon → Adapter | Ask adapter to trigger a crawl on a URL it discovered |
| `_axon.session_context` | Axon → Adapter | Push RAG search results as context before prompt |
| `_axon.workspace_snapshot` | Adapter → Axon | Adapter sends a manifest of files it modified |
| `_axon.emit_metric` | Adapter → Axon | Adapter emits custom metrics (token count, operation latency) |

**Implementation path**: Implement `ext_method` and `ext_notification` in `AcpBridgeClient`, dispatch on `args.method`:

```rust
async fn ext_method(&self, args: ExtRequest) -> Result<ExtResponse> {
    match args.method.as_str() {
        "_axon.index_file" => self.handle_index_file(args.params).await,
        "_axon.workspace_snapshot" => self.handle_workspace_snapshot(args.params).await,
        _ => Err(Error::method_not_found()),
    }
}
```

---

### 8. Message Stream Observation

**SDK mechanism**: `conn.subscribe()` → `StreamReceiver`

**What it does**: Returns a broadcast receiver that delivers every JSON-RPC message (requests, responses, notifications) in both directions with direction metadata (`Incoming`/`Outgoing`).

**Why it matters for axon**:
1. **Debugging**: Trace exact protocol messages without adding `tracing` spans to the SDK internals
2. **Audit log**: Record all ACP traffic per session for compliance or replay
3. **Protocol conformance testing**: Assert which methods are called and in what order
4. **Latency profiling**: Timestamp each message to find slow adapter methods

**Current state**: Not used. axon adds `tracing` spans around SDK calls, but doesn't have visibility into the raw JSON-RPC layer.

**Implementation path**: After creating `ClientSideConnection`, call `conn.subscribe()` and spawn a task that forwards each `StreamMessage` to the session's tracing subscriber or an audit log:

```rust
let mut stream = conn.subscribe();
tokio::task::spawn_local(async move {
    while let Ok(msg) = stream.recv().await {
        tracing::trace!(
            direction = ?msg.direction,
            method = ?msg.message.method(),
            "acp_wire_message"
        );
    }
});
```

---

### 9. Per-Session Model Selection (Unstable)

**SDK method**: `set_session_model(SetSessionModelRequest)` — requires `unstable_session_model` feature

**What it does**: Dedicated RPC to change the active LLM model for a session. Separate from `set_session_config_option` which uses a generic key-value system.

**Current state**: axon uses `SetSessionConfigOptionRequest` with the model config key from the session's advertised options. This works but depends on:
1. The adapter advertising a model config option
2. axon correctly identifying which option key maps to model selection
3. The Codex-specific workaround of reading `~/.codex/models_cache.json` when no options are advertised

**Advantage of `set_session_model`**: Semantically clear, no adapter-specific workarounds needed.

**Risk**: Unstable — may change before ratification. Keep `SetSessionConfigOptionRequest` as fallback.

---

### 10. Session Usage Tracking (Unstable)

**SDK feature**: `unstable_session_usage`

**What it does**: Attaches usage metadata (token counts, cost estimates) to `PromptResponse`. The existing `AcpUsageSnapshot` in axon already tracks this for `acp_llm` completions — but this is axon tracking usage *after the fact*, not the adapter reporting it natively via the protocol.

**Advantage**: Per-turn token counts from the adapter's perspective (which may differ from axon's estimates for streamed turns). Enables accurate billing and quota enforcement.

**Implementation**: Enable feature flag, read `PromptResponse::usage` if present, incorporate into `AcpUsageSnapshot`. Expose via WS event or `axon stats` output.

---

### 11. Elicitation (Unstable)

**SDK feature**: `unstable_elicitation`

**What it does**: Allows the adapter to prompt the user for structured input mid-turn (e.g., "Choose which files to include:", "Enter your API key:") without the adapter having to encode this into its response text. The client presents the structured form and returns the user's response.

**Why it matters for axon**: Interactive Pulse Chat sessions could use this to display inline structured inputs (file pickers, option lists) when the adapter needs clarification. Currently adapters must put clarification questions into the text stream and hope the user responds in the next prompt.

**Complexity**: Medium — requires Pulse Chat frontend changes to render `ElicitationRequest` UI and a new WS message type to route the response back.

---

### 12. Boolean Config Options (Unstable)

**SDK feature**: `unstable_boolean_config`

**What it does**: Adds a `boolean` variant to the `SessionConfigOption` type (currently only selector/enum options are supported). Adapters can expose toggle-style settings (e.g., "Enable extended thinking", "Auto-compact context").

**Current state**: `SetSessionConfigOptionRequest` only sends string values. If an adapter exposes boolean config, axon may be sending the wrong type.

**Implementation**: Enable feature flag, add boolean handling to `apply_config_options()` in `acp/persistent_conn/session_options.rs`.

---

### 13. Message ID Tracking (Unstable)

**SDK feature**: `unstable_message_id`

**What it does**: Attaches a stable `message_id` to each `SessionNotification`. Enables deduplication and ordered replay across reconnects.

**Current state**: axon's session cache stores messages by insertion order, relying on sequential delivery for correct replay. If the adapter resends a notification (e.g., after a transport hiccup), the replay buffer will contain duplicates.

**Advantage**: With `message_id`, axon can deduplicate replay buffer entries by ID, guaranteeing exactly-once delivery to the frontend after reconnect.

**Implementation**: Enable feature flag, use `SessionNotification::message_id` as the replay buffer key instead of insertion order.

---

### 14. MCP Server Management

> **Status: ✅ Implemented** (2026-03-23 implementation sprint)

**SDK types**: `McpServer` (enum: `Stdio`, `Http`, `Sse`), `McpCapabilities`, `McpServerHttp`, `McpServerSse`, `McpServerStdio`, `HttpHeader`

**What axon implements:**

| Feature | Status | Location |
|---------|--------|----------|
| `McpServer::Stdio` passthrough | ✅ | `mapping::convert_mcp_servers` |
| `McpServer::Http` passthrough + headers | ✅ | `mapping::convert_mcp_servers` + `AcpMcpServerConfig::Http.headers` |
| `McpServer::Sse` passthrough + headers | ✅ | `mapping::convert_mcp_servers` + `AcpMcpServerConfig::Sse` variant |
| `McpCapabilities` reading from `InitializeResponse` | ✅ | `session::initialize_connection` → `AcpRuntimeState.mcp_http/sse_supported` |
| Capability-based transport filtering (one-shot) | ✅ | `mapping::filter_sdk_mcp_servers` via `runtime::apply_mcp_capability_filter` |
| Capability-based transport filtering (persistent) | ✅ | `mapping::filter_sdk_mcp_servers` via `turn::ensure_turn_session` |
| MCP servers on load-session fallback (one-shot) | ✅ | `session::setup_session` — clones before consume, passes to fallback |
| MCP servers on load-session fallback (persistent) | ✅ | `turn::load_or_fallback_session` — passes through to `create_new_session` |
| `mcp.json` SSE transport (`"transport": "sse"`) | ✅ | `mcp_config::fetch_axon_mcp_servers_from_disk` |
| `mcp.json` HTTP/SSE headers (`"headers": [...]`) | ✅ | `mcp_config::fetch_axon_mcp_servers_from_disk` |
| Unknown `mcp.json` transport → warn + Http fallback | ✅ | `mcp_config::fetch_axon_mcp_servers_from_disk` |
| `blocked_mcp_tools` per-turn | ✅ | `bridge::AcpRuntimeState.blocked_mcp_tools`, set in `turn::build_turn_context` |

**Pre-sprint gaps (now closed):**

1. `AcpMcpServerConfig` had no `Sse` variant — `McpServer::Sse` was unrepresentable
2. `McpCapabilities` from `InitializeResponse` was never read — Http/Sse servers sent blindly even to stdio-only adapters
3. Load-session fallback silently dropped MCP servers (both one-shot and persistent paths)
4. `mcp.json` disk loader could not represent SSE transport or HTTP/SSE headers

---

## Priority Ranking

Ordered by impact-to-complexity ratio:

| # | Gap | Impact | Complexity | Notes |
|---|-----|--------|-----------|-------|
| ~~1~~ | ~~**CancelNotification**~~ | ~~High — prevents data loss~~ | ~~Medium~~ | ✅ **Done** — 15 s graceful window + `CancellationToken` in persistent mode |
| ~~2~~ | ~~**File System Access** (`read`/`write_text_file`)~~ | ~~High — unlocks coding tasks~~ | ~~Medium~~ | ✅ **Done** — CWD-scoped, path-traversal-safe, audit log |
| ~~3~~ | ~~**Session Modes** (`set_session_mode`)~~ | ~~High — unlocks read-only mode~~ | ~~Low~~ | ✅ **Done** — dedup via `current_mode`, alias normalisation |
| ~~4~~ | ~~**MCP Server Management**~~ | ~~Medium — enables agent tooling~~ | ~~Medium~~ | ✅ **Done** — Stdio/Http/Sse, capability filtering, mcp.json support |
| 1 | **`close_session`** | Medium — prevents resource leaks | Medium | Pairs with CancelNotification work |
| 2 | **Message Stream Observation** | Medium — debug/audit | Low | Two lines of code + a spawned task |
| 3 | **`resume_session`** | Medium — reduces reconnect latency | Medium | Try before `load_session`, fallback on error |
| 4 | **Terminal Operations** | High — enables shell commands | High | Needs TerminalRegistry, PTY, security sandbox |
| 5 | **Session Usage (Unstable)** | Medium — accurate token reporting | Low | Read optional field from PromptResponse |
| 6 | **Boolean Config (Unstable)** | Low — config completeness | Low | Add variant to config option handling |
| 7 | **Message ID Tracking (Unstable)** | Low — dedup on reconnect | Low | Key replay buffer by message_id |
| 8 | **Extension Methods** | Medium — axon-specific RPCs | Low | Dispatch table in AcpBridgeClient |
| 9 | **Authentication** | Medium — security posture | Medium | Gate with env var, backward compatible |
| 10 | **Session Fork** | Low–Medium — advanced UX | Medium | Needs session graph in cache |
| 11 | **`list_sessions`** | Low — session browser UI | Low | Blocked on frontend work |
| 12 | **Elicitation (Unstable)** | Medium — interactive UX | High | Requires frontend form renderer |
| 13 | **`set_session_model` (Unstable)** | Low — supersedes config hack | Low | Keep config fallback, add dedicated path |
| 14 | **Protocol version negotiation** | Low — future-proofing | Low | V1 is current; add version check |

---

## Implementation Notes

### Why Terminal is Last Despite High Impact

Terminal operations require a full PTY management layer (`tokio-pty-process` or `portable-pty`), a per-terminal I/O buffer, and a security sandbox to prevent the adapter from running arbitrary commands outside the session scope. The risk/complexity ratio is the highest of any gap, and the same functionality is currently achievable via MCP `filesystem` + `shell` servers. File access (`read_text_file` / `write_text_file`) is now implemented and covers 80% of coding use cases without requiring PTY machinery.

### ACP SDK `!Send` Constraint

Every gap implementation must be added to `AcpBridgeClient`, which lives inside a `tokio::task::LocalSet` on a `current_thread` runtime. All new state must be `!Send`-compatible (i.e., `Rc<RefCell<...>>` not `Arc<Mutex<...>>`). The `PermissionResponderMap` uses `Arc<DashMap>` because it's shared with the WS handler layer outside the `LocalSet` — that specific pattern is fine, but new state internal to the bridge must use `Rc`.

### Unstable Feature Flag Strategy

The recommended approach is to enable unstable features selectively, gated by runtime env checks:

```toml
# Cargo.toml
[features]
acp-unstable = ["agent-client-protocol/unstable"]
```

Then at runtime:
```rust
#[cfg(feature = "acp-unstable")]
if env::var("AXON_ACP_UNSTABLE").is_ok() {
    // use unstable feature
}
```

This avoids breaking stable builds while allowing opt-in experimentation.

### Cargo.toml Current ACP Dependency

```toml
# crates/services/Cargo.toml (approximate)
agent-client-protocol = { version = "0.10.2" }
```

No unstable features are currently enabled. To unlock any unstable gap:

```toml
agent-client-protocol = { version = "0.10.2", features = [
    "unstable_session_close",
    "unstable_session_resume",
    "unstable_session_usage",
    "unstable_message_id",
    "unstable_boolean_config",
] }
```

---

*End of ACP Gap Analysis*
