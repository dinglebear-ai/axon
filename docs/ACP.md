# ACP (Agent Client Protocol)
Last Modified: 2026-03-21

Axon implements ACP (Agent Client Protocol) to communicate with AI agent CLI adapters — Claude Code, Codex, and Gemini — as child subprocesses. The Axon host spawns the adapter, exchanges JSON messages over stdio, and streams all events to the frontend over WebSocket.

## Table of Contents

1. [Overview](#overview)
2. [Adapter Binaries](#adapter-binaries)
3. [Execution Modes: One-Shot vs Persistent](#execution-modes-one-shot-vs-persistent)
4. [Process Lifecycle (One-Shot)](#process-lifecycle-one-shot)
5. [Persistent Connection Mode](#persistent-connection-mode)
6. [Adapter Resolution & Configuration](#adapter-resolution--configuration)
7. [Environment Isolation](#environment-isolation)
8. [Security Validation](#security-validation)
9. [Model Override Mechanics](#model-override-mechanics)
10. [Permission Gating](#permission-gating)
11. [Wire Protocol: Client → Adapter](#wire-protocol-client--adapter)
12. [Wire Protocol: Adapter → Client (Events)](#wire-protocol-adapter--client-events)
13. [ServiceEvent Channel](#serviceevent-channel)
14. [Session Cache & WS Reconnect](#session-cache--ws-reconnect)
15. [Prewarming](#prewarming)
16. [ACP-Backed LLM Completions](#acp-backed-llm-completions)
17. [WebSocket Message Protocol](#websocket-message-protocol)
18. [WsConnState](#wsconnstate)
19. [Security: ALLOWED_MODES and ALLOWED_FLAGS](#security-allowed_modes-and-allowed_flags)
20. [Concurrent Session Limits](#concurrent-session-limits)
21. [Validation Rules](#validation-rules)
22. [Config Options](#config-options)
23. [Preflight: Codex Skill Symlinks](#preflight-codex-skill-symlinks)
24. [Test Coverage](#test-coverage)
25. [Key Design Decisions](#key-design-decisions)

---

## Overview

ACP is the wire protocol connecting a **host** (Axon) to an **agent CLI** (Claude Code, Codex, Gemini). Axon uses the `agent_client_protocol` Rust crate (Anthropic's ACP SDK) to drive the protocol.

The ACP system in Axon has two primary surfaces:

- **CLI path** (`axon ask`, `axon research`, etc.) — uses one-shot mode: spawn, run, teardown
- **Pulse Chat** (WebSocket interactive sessions) — uses persistent-connection mode: adapter lives for the WS connection lifetime

Both paths share the same service layer (`crates/services/acp/`). All adapters are external CLI binaries spawned as child processes over stdio.

```
Browser (apps/web)
  │ WebSocket
  ▼
Axon WS bridge (crates/web)
  │ ServiceEvent channel
  ▼
ACP service layer (crates/services/acp/)
  │ stdio (stdin/stdout/stderr)
  ▼
Adapter process (claude-agent-acp / codex-acp / gemini)
  │ Anthropic API / OpenAI API / Google API
  ▼
LLM
```

### Key Source Files

| File | Role |
|------|------|
| `crates/services/acp.rs` | `AcpClientScaffold`, spawn, env allowlist |
| `crates/services/acp/runtime.rs` | One-shot: `run_prompt_turn`, `establish_acp_session` |
| `crates/services/acp/session.rs` | Session setup, config/model apply |
| `crates/services/acp/adapters.rs` | Adapter detection, model override |
| `crates/services/acp/bridge.rs` | `AcpBridgeClient`, `AcpRuntimeState`, event dispatch |
| `crates/services/acp/config.rs` | Config option builders, model cache readers |
| `crates/services/acp/mapping.rs` | SDK event → service type conversions |
| `crates/services/acp/mapping/validation.rs` | Input validation |
| `crates/services/acp/permission.rs` | Permission gating, `PermissionResponderMap` |
| `crates/services/acp/persistent_conn.rs` | `AcpConnectionHandle`, persistent mode |
| `crates/services/acp/persistent_conn/turn.rs` | Per-turn execution in persistent mode |
| `crates/services/acp/persistent_conn/session_options.rs` | Model/config apply in persistent mode |
| `crates/services/acp/session_cache.rs` | `SESSION_CACHE`, replay buffer, reaper |
| `crates/services/acp/preflight.rs` | Codex skill symlink repair |
| `crates/services/types/acp.rs` | All ACP wire types + custom Serialize impls |
| `crates/services/events.rs` | `ServiceEvent` enum, `emit()` |
| `crates/services/acp_llm.rs` | ACP-backed non-interactive completions |
| `crates/web/execute/sync_mode/acp_adapter.rs` | Adapter command resolution for Pulse Chat |
| `crates/web/execute/sync_mode/pulse_chat.rs` | WS → ACP orchestration |
| `crates/web/execute/sync_mode/pulse_chat/connection.rs` | Session cache integration |
| `crates/web/execute/sync_mode/pulse_chat/events.rs` | Event forwarding to WS |
| `crates/web/execute/sync_mode/prewarm.rs` | Adapter prewarming on server startup |
| `crates/web/ws_handler.rs` | WS message routing, `WsConnState` |
| `crates/web/ws_handler/acp_session.rs` | `handle_acp_resume`, `route_permission_response` |

---

## Adapter Binaries

Three adapter agents are supported. Each maps to a CLI binary:

| Agent | Default Binary | Default Args | Per-Agent Env Override |
|-------|---------------|--------------|------------------------|
| Claude | `claude-agent-acp` | (none) | `AXON_ACP_CLAUDE_ADAPTER_CMD` / `AXON_ACP_CLAUDE_ADAPTER_ARGS` |
| Codex | `codex-acp` | (none) | `AXON_ACP_CODEX_ADAPTER_CMD` / `AXON_ACP_CODEX_ADAPTER_ARGS` |
| Gemini | `gemini` | `--experimental-acp` | `AXON_ACP_GEMINI_ADAPTER_CMD` / `AXON_ACP_GEMINI_ADAPTER_ARGS` |

Defaults are defined in `acp_adapter.rs::default_adapter_for_agent()`.

---

## Execution Modes: One-Shot vs Persistent

| | One-Shot | Persistent |
|--|----------|------------|
| **Process lifetime** | Spawned per prompt, torn down after | Spawned once, shared across all turns on a WS connection |
| **Used by** | CLI commands (`ask`, `research`, `evaluate`, etc.) | Pulse Chat WebSocket sessions |
| **Session state** | Clean per turn | Preserved across turns on same connection |
| **Latency** | High on first turn (cold start ~45s) | Low after first turn (reuses live process) |
| **Overall timeout** | 300s (`ACP_ADAPTER_TIMEOUT`) | 3600s (configurable via `adapter_timeout_secs`) |
| **Turn timeout** | N/A | 5 min (`DEFAULT_TURN_TIMEOUT`, env: `AXON_ACP_TURN_TIMEOUT_MS`) |
| **Exit handling** | 10s graceful shutdown + SIGKILL | Process stays alive; evicted by reaper or fatal error |
| **Entry point** | `AcpClientScaffold::start_prompt_turn()` | `AcpConnectionHandle::spawn()` or `spawn_eager()` |

---

## Process Lifecycle (One-Shot)

### Why a Dedicated Runtime

ACP SDK futures are `!Send` (implement `?Send` traits). They cannot be spawned directly on Axon's multi-threaded tokio runtime. Every ACP operation is encapsulated in `run_acp_event_loop()` which provides a dedicated `spawn_blocking` thread with a `current_thread` tokio runtime + `LocalSet`.

> **Warning:** `acp_llm.rs::run_completion_inner` uses the same `spawn_blocking` + `current_thread` pattern. Direct `LocalSet::run_until` from a multi-threaded runtime panics. This path must always be wrapped in `spawn_blocking`.

```rust
// acp.rs:151
async fn run_acp_event_loop<F, Fut>(timeout: Duration, fut: F) -> Result<(), Box<dyn Error>>
```

### Exact Spawn → Teardown Sequence

**Phase 1: Adapter Override Resolution** (`establish_acp_session`)

1. Apply Codex model override: `append_codex_model_override(adapter, model)`
2. Apply Gemini model override: `append_gemini_model_override(adapter, model)`
3. Detect adapter kind (`is_codex_adapter`, `is_gemini_adapter`) for conditional logic downstream

**Phase 2: Process Spawn** (`spawn_adapter_with_io`)

1. Call `AcpClientScaffold::spawn_adapter()` — validates command, spawns via `tokio::process::Command`
2. Wrap child in `AdapterGuard` (RAII, calls `child.start_kill()` on drop)
3. Extract stdin, stdout, stderr from child
4. Spawn two local tasks (via `tokio::task::spawn_local`):
   - **Stderr reader**: reads adapter stderr line-by-line, emits `ServiceEvent::Log`
   - **Exit watcher**: awaits `child.wait()`, sends status via oneshot channel

**Phase 3: Connection Initialization** (`initialize_connection`)

1. Create `AcpRuntimeState` (RefCell-based, `!Send`, safe inside `LocalSet`)
2. Create `AcpBridgeClient` (implements ACP SDK `Client` trait)
3. Wrap stdin/stdout with tokio compat adapters
4. Call `ClientSideConnection::new()` — spawns I/O task
5. Send `InitializeRequest` with protocol version V1, client info `("axon", version)`, and capabilities

**Phase 4: Session Setup** (`setup_session`)

1. Validate CWD exists and is a directory (`validate_session_cwd`)
2. Call `conn.new_session()` or `conn.load_session()` depending on request type
3. On load failure: fall back to new session, emit `SessionFallback` event

**Phase 5: Config and Model Application** (`apply_config_and_model`)

1. Extract config options from session setup response
2. If Codex adapter and no options returned: read from `~/.codex/models_cache.json`
3. If Gemini adapter and no options returned: read from `GEMINI_CLI_HOME/settings.json` or `~/.gemini/settings.json`
4. If model override requested: apply via `SetSessionConfigOptionRequest` (only if model is in allowed values)
5. Store final config options in `runtime_state.config_options`

**Phase 6: Prompt Execution** (`run_prompt_turn`)

```rust
let mut exit_rx = exit_rx;
let prompt_fired = tokio::select! {
    biased;
    prompt_result = conn.prompt(PromptRequest::new(session_id.clone(), prompt_blocks)) => {
        // success path
        finalize_successful_turn(...).await?;
        true
    }
    exit_msg = &mut exit_rx => {
        // adapter crashed before returning result
        return Err("ACP adapter crashed mid-session");
    }
};
```

`biased;` ensures prompt branch is polled first.

**Phase 7: Teardown** (`wait_for_adapter_exit`)

1. Drop `conn` (closes stdin/stdout) → signals adapter to flush session `.jsonl` and exit
2. Drop `runtime_state` → releases bridge reference
3. Await `exit_rx` with **10-second timeout**:
   - `Ok(Ok(crash_msg))` → non-zero exit, log warning
   - `Ok(Err(_))` → sender dropped = clean exit (code 0), log success
   - `Err(_)` → timeout, `kill_on_drop(true)` fires SIGKILL on handle drop

**SIGKILL-FIX note:** `wait_for_adapter_exit()` must be called _inside_ `run_prompt_turn()` to keep the `LocalSet` alive. If the `LocalSet` tears down before await, the exit watcher task is cancelled → child handle dropped → `kill_on_drop(true)` fires SIGKILL before the adapter writes its session file.

### AdapterGuard (RAII)

```rust
pub(super) struct AdapterGuard(pub(super) Option<tokio::process::Child>);

impl Drop for AdapterGuard {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.0 {
            let _ = child.start_kill(); // SIGTERM
        }
    }
}
```

- Fires on scope exit, early return, or `?` propagation
- Disarmed via `.take()` when passing child to the exit watcher
- Prevents zombie processes on all error paths

---

## Persistent Connection Mode

### AcpConnectionHandle

The persistent-mode adapter manager. One handle per WS connection (per agent + capability combination).

```rust
pub struct AcpConnectionHandle {
    tx: mpsc::Sender<AdapterMessage>,     // Turn dispatch channel
    _join: tokio::task::JoinHandle<()>,   // Keeps background task alive
}

pub struct TurnRequest {
    pub req: AcpPromptTurnRequest,
    pub service_tx: Option<mpsc::Sender<ServiceEvent>>,
    pub result_tx: oneshot::Sender<Result<(), String>>,
}
```

**Spawn variants:**

`AcpConnectionHandle::spawn()` — Lazy init. Setup is deferred to the first turn. Setup progress events appear in the first turn's event stream.

`AcpConnectionHandle::spawn_eager()` — Eager init. Setup starts immediately in the background, overlapping with parallel work (e.g., Tavily search). Queued turns wait in the channel (capacity 16) until setup completes.

### Background Adapter Loop

> **Maintenance note:** `adapter_loop` and `adapter_loop_eager` share an identical main select loop body. Any behavior change (heartbeat, shutdown order, metrics) must be applied to both functions. This duplication is tracked as a known issue.

The background thread (inside `spawn_blocking` → `current_thread` runtime + `LocalSet`) runs:

```
On first turn (lazy):
  1. receive TurnRequest from channel
  2. establish_acp_session() → setup events go to first turn's service_tx
  3. run_turn_on_conn() for first turn
  4. loop: receive turns or detect adapter exit

On startup (eager):
  1. establish_acp_session() immediately → setup events go to setup_tx
  2. loop: receive turns or detect adapter exit
```

### Per-Turn Flow in Persistent Mode

From WS message to result:

1. **WS message arrives** → `handle_pulse_chat()`
2. **Get or create connection** via `get_or_create_acp_connection()`:
   - Build `agent_key = "{agent}:mcp={fingerprint}:{caps_fingerprint}"`
   - Check `SESSION_CACHE` for live handle with that key
   - If hung turn (>5 min): evict, spawn fresh
   - If cache miss: `AcpConnectionHandle::spawn()`, insert into cache
3. **Dispatch turn**: `conn_handle.run_turn(TurnRequest { req, service_tx, result_tx })`
4. **Drive events**: `drive_turn_events()` — `tokio::select!` polling result_rx and event_rx simultaneously
5. **Result**: classify fatal vs recoverable, evict on fatal, keep on recoverable

**Session resolution per turn:**
- No `session_id` in request → `conn.new_session()` (fresh session)
- `session_id` present → `conn.load_session()` (resume, fallback to new on error)

**Model change mid-session:**
- Checks `runtime_state.established_model` against request
- If different: sends `SetSessionConfigOptionRequest` to adapter
- Only applies if model is in the adapter's advertised allowed values

---

## Adapter Resolution & Configuration

### Priority Order (Highest to Lowest)

For each agent type (Claude / Codex / Gemini):

```
1. AXON_ACP_CLAUDE_ADAPTER_CMD (or CODEX / GEMINI variant)
   └── AXON_ACP_CLAUDE_ADAPTER_ARGS
2. Config.acp_adapter_cmd (from AXON_ACP_ADAPTER_CMD)
   └── Config.acp_adapter_args (from AXON_ACP_ADAPTER_ARGS)
3. Hardcoded defaults (binary + args from default_adapter_for_agent())
```

### Arg Format: Pipe-Delimited

`AXON_ACP_ADAPTER_ARGS` is pipe-delimited:

```
AXON_ACP_GEMINI_ADAPTER_ARGS=--stdio|--model|gemini-2.0-flash
```

Parsed by splitting on `|`, trimming whitespace, dropping empty segments.

### Executable Path Resolution

Search order:
1. If path contains `/` or is absolute: use as-is
2. Search every directory in `$PATH`
3. `$HOME/.local/bin/{program}`
4. `$HOME/.cargo/bin/{program}`
5. `/usr/local/bin/{program}`
6. `/usr/bin/{program}`

**Fallback heuristics:** If the program name doesn't resolve but args or program contain "codex", search for `codex` binary. Same for "gemini". Handles agent switching when `AXON_ACP_ADAPTER_CMD` is set globally.

---

## Environment Isolation

When spawning any adapter, Axon calls `cmd.env_clear()` then re-injects only an explicit allowlist. **This only affects the child subprocess.** The parent Axon process environment is never modified.

### ACP_ENV_ALLOWLIST (re-injected if present in parent)

| Variable | Purpose |
|----------|---------|
| `PATH`, `HOME`, `USER`, `SHELL`, `TERM`, `LANG`, `LC_ALL`, `TZ`, `TMPDIR`, `XDG_RUNTIME_DIR` | System/shell basics |
| `SSL_CERT_FILE`, `SSL_CERT_DIR` | Custom CA bundles |
| `ANTHROPIC_API_KEY` | Claude API auth |
| `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX` | Claude Code endpoint selection |
| `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME` | XDG standard directories |
| `GEMINI_API_KEY`, `GOOGLE_API_KEY`, `GOOGLE_CLOUD_PROJECT`, `GOOGLE_CLOUD_LOCATION`, `GOOGLE_APPLICATION_CREDENTIALS`, `GEMINI_CLI_HOME`, `GEMINI_FORCE_FILE_STORAGE` | Gemini auth and config |

### Intentionally Excluded

| Variable | Reason |
|----------|--------|
| `OPENAI_*` | Points to Axon's local LLM proxy, not OpenAI. Adapters use their own OAuth/stored keys. Forwarding would leak the wrong endpoint. |
| `CLAUDECODE` | Prevents nested-session detection. Claude Code sets this on child processes. If forwarded, the inner `claude` CLI detects it's running inside Claude Code and exits with an error. |

---

## Security Validation

`validate_adapter_command()` runs before every spawn:

### Shell Blocklist

Blocked basenames (case-insensitive, strips `.exe`):
```
sh, bash, zsh, fish, dash, ksh, csh, tcsh, cmd, powershell, pwsh
```

Checks:
1. Bare program name (e.g., `"bash"`)
2. Full path basename (e.g., `"/bin/bash"` → checks `"bash"`)
3. Canonicalized path basename (symlink resolution: `/tmp/safe_name → /bin/bash` → blocked)

### Other Checks

1. Program string is non-empty
2. If path-like, file exists (or assumed to be in PATH — non-existence is allowed at validation time)
3. Not a directory (no trailing `/` or `\`)

Integration tests verify environment isolation through validated binaries such as `/usr/bin/env`, not by bypassing adapter validation. There is no test-only ACP spawn escape hatch anymore.

---

## Model Override Mechanics

### Codex Model Override (`append_codex_model_override`)

**Trigger conditions:**
- Adapter binary name contains "codex"
- Model is non-empty and non-default
- Model does NOT start with "gemini" (prevents stale Gemini model names from forwarding)

**Format:** Appends `-c "model=\"{model}\""` to adapter args.

**Model string validation:** Alphanumeric + `-_./: ` only. Safe because args go via `execvp()`, not shell expansion.

### Gemini Model Override (`append_gemini_model_override`)

**Trigger conditions:**
- Adapter binary name contains "gemini"
- Model is non-empty and non-default
- Model DOES start with "gemini"

**Format:** Appends `--model {model}` to adapter args.

### Model Normalization

`normalized_requested_model()` returns `None` if the input is `None`, empty after trim, or equals "default" (exact match). Otherwise returns trimmed string. Used to normalize before passing to override functions.

---

## Permission Gating

When the adapter calls a tool that requires user approval, ACP delivers a permission request. The bridge suspends the turn and waits for the frontend to respond.

### PermissionResponderMap

```rust
pub type PermissionResponderMap =
    Arc<dashmap::DashMap<(String, String), tokio::sync::oneshot::Sender<String>>>;
```

**Key:** `(session_id, tool_call_id)` composite — SEC-7 prevents cross-session collision even if `tool_call_id` values happen to match between concurrent sessions.

**DashMap** instead of `Mutex<HashMap>`: shard-level locking eliminates contention when the bridge inserts from the ACP callback thread while the WS handler removes after response.

### Interactive Permission Flow

1. ACP bridge receives `RequestPermission` SDK callback
2. Bridge emits `PermissionRequest` event to frontend (via `ServiceEvent::AcpBridge`)
3. Bridge inserts a `oneshot::Sender<String>` into `PermissionResponderMap` keyed by `(session_id, tool_call_id)`
4. Bridge returns `RequestPermissionOutcome::Pending` to adapter (adapter suspends)
5. Frontend user picks an option, sends `permission_response` WS message
6. `route_permission_response()` in `ws_handler/acp_session.rs` looks up the key, sends chosen `option_id` through the oneshot sender
7. Bridge unblocks, dispatches approved tool call to adapter
8. `PermissionGuard` RAII removes the map entry on drop

**Permission timeout:** Default 60s (configurable via `permission_timeout_secs` on `AcpAdapterCommand`). On timeout: outcome is `Cancelled`.

**Auto-approve mode:** `AXON_ACP_AUTO_APPROVE=true`. Selects `AllowAlways` option first, then `AllowOnce`, then `Cancelled`. No frontend interaction.

### Blocked MCP Tools

`AcpPromptTurnRequest.blocked_mcp_tools` lists tool names (command IDs) that are auto-cancelled without reaching the frontend. Bridge checks against `runtime_state.blocked_mcp_tools` (lowercased HashSet) before any prompt.

---

## Wire Protocol: Client → Adapter

### AcpPromptTurnRequest

```rust
pub struct AcpPromptTurnRequest {
    pub session_id: Option<String>,       // Load existing session or None for new
    pub prompt: Vec<String>,              // Prompt blocks (at least 1 required)
    pub model: Option<String>,            // Model override (None = keep current)
    pub session_mode: Option<String>,     // Approval mode (e.g., "auto")
    pub blocked_mcp_tools: Vec<String>,   // Tool names to auto-cancel
    pub mcp_servers: Vec<AcpMcpServerConfig>, // MCP servers to pass through
}
```

### AcpSessionProbeRequest

```rust
pub struct AcpSessionProbeRequest {
    pub session_id: Option<String>,
    pub model: Option<String>,
}
```

### AcpMcpServerConfig

```rust
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum AcpMcpServerConfig {
    Stdio {
        name: String,
        command: String,
        #[serde(default)] args: Vec<String>,
        #[serde(default)] env: Vec<(String, String)>,
    },
    Http {
        name: String,
        url: String,
    },
}
```

---

## Wire Protocol: Adapter → Client (Events)

All events serialize from `AcpBridgeEvent` using custom `Serialize` impls. Each variant produces a flat JSON object with a `"type"` discriminator field.

### AcpBridgeEvent Variants

#### SessionUpdate

**Rust variant:** `AcpBridgeEvent::SessionUpdate(AcpSessionUpdateEvent)`

**Wire `type` values:** `"user_delta"` | `"assistant_delta"` | `"thinking_content"` | `"tool_use"` | `"tool_use_update"` | `"usage_update"` | `"unknown"`

**Wire shape:**

```json
{
  "type": "assistant_delta",
  "session_id": "sess-abc123",
  "tool_call_id": null,
  "delta": "streaming token text"
}
```

Special case — `thinking_content` uses `"content"` key instead of `"delta"`:

```json
{
  "type": "thinking_content",
  "session_id": "sess-abc123",
  "content": "internal reasoning text"
}
```

Tool call events additionally include:

```json
{
  "type": "tool_use",
  "session_id": "sess-abc123",
  "tool_call_id": "call-xyz",
  "tool_name": "Read",
  "tool_status": "in_progress",
  "tool_input": { "file_path": "/foo/bar.rs" },
  "tool_locations": ["/foo/bar.rs"]
}
```

All optional fields use `skip_serializing_if = "Option::is_none"`.

#### PermissionRequest

```json
{
  "type": "permission_request",
  "session_id": "sess-abc123",
  "tool_call_id": "call-xyz",
  "options": ["allow_once", "deny"]
}
```

Note: `option_ids` field serializes as `"options"` on the wire.

#### TurnResult

```json
{
  "type": "result",
  "session_id": "sess-abc123",
  "stop_reason": "end_turn",
  "result": "Full accumulated assistant response"
}
```

Stop reason values: `"end_turn"` | `"max_tokens"` | `"max_turn_requests"` | `"refusal"` | `"cancelled"` | `"unknown"`

#### ConfigOptionsUpdate

```json
{
  "type": "config_options_update",
  "session_id": "sess-abc123",
  "configOptions": [
    {
      "id": "model",
      "name": "Model",
      "description": "Select the LLM model",
      "category": "model",
      "currentValue": "claude-opus-4",
      "options": [
        { "value": "claude-opus-4", "name": "Claude Opus 4", "description": null }
      ]
    }
  ]
}
```

#### PlanUpdate

```json
{
  "type": "plan_update",
  "session_id": "sess-abc123",
  "entries": [
    { "content": "Research codebase", "priority": "high", "status": "in_progress" }
  ]
}
```

Priority: `"low"` | `"medium"` | `"high"` | `"unknown"` — Status: `"pending"` | `"in_progress"` | `"completed"` | `"unknown"`

#### ModeUpdate

```json
{
  "type": "mode_update",
  "session_id": "sess-abc123",
  "currentModeId": "standard"
}
```

#### CommandsUpdate

```json
{
  "type": "commands_update",
  "session_id": "sess-abc123",
  "commands": [
    { "name": "/search", "description": "Search the web" }
  ]
}
```

#### UsageUpdate

```json
{
  "type": "usage_update",
  "session_id": "sess-abc123",
  "usage": {
    "total_tokens": 4200,
    "input_tokens": 0,
    "output_tokens": 0
  },
  "size": 8000,
  "costAmount": "0.042",
  "costCurrency": "USD"
}
```

The nested `"usage"` object always includes all three token fields (even if 0) — required by the frontend Zod schema. Cost fields are omitted when absent.

#### SessionFallback

```json
{
  "type": "session_fallback",
  "old_session_id": "sess-old",
  "new_session_id": "sess-new"
}
```

Emitted when a `LoadSession` request fails and the runtime creates a new session instead.

#### SessionInfoUpdate

```json
{
  "type": "session_info_update",
  "session_id": "sess-abc123"
}
```

Signals that session metadata (title, updated_at) changed. Frontend should refetch.

### AcpSessionUpdateKind ↔ Wire Type

| Rust Variant | Wire `"type"` | Trigger |
|-------------|--------------|---------|
| `UserDelta` | `"user_delta"` | User message chunk from adapter |
| `AssistantDelta` | `"assistant_delta"` | Assistant response token |
| `ThinkingDelta` | `"thinking_content"` | Extended thinking token |
| `ToolCallStarted` | `"tool_use"` | Tool call begins |
| `ToolCallUpdated` | `"tool_use_update"` | Tool call state change |
| `Plan` | Intercepted → `PlanUpdate` | Agent plan notification |
| `AvailableCommandsUpdate` | Intercepted → `CommandsUpdate` | Available commands changed |
| `CurrentModeUpdate` | Intercepted → `ModeUpdate` | Agent mode changed |
| `ConfigOptionUpdate` | Intercepted → `ConfigOptionsUpdate` | Config options changed |
| `UsageUpdate` | `"usage_update"` | Context window stats |
| `Unknown` | `"unknown"` | Unrecognized SDK event |

---

## ServiceEvent Channel

Service functions emit progress events via `mpsc::Sender<ServiceEvent>` passed from callers. Pass `None` for `tx` in CLI commands that don't need streaming progress — `emit()` is a no-op when `tx` is `None`.

### ServiceEvent Variants

```rust
pub enum ServiceEvent {
    Log { level: LogLevel, message: String },
    AcpBridge { event: AcpBridgeEvent },
    EditorWrite { content: String, operation: EditorOperation },
    SynthesisDelta { text: String },
}
```

**LogLevel:** `Info` | `Warn` | `Error` (serializes lowercase)

**EditorOperation:** `Replace` | `Append` — applied by frontend to an editor pane

**EditorWrite** fires when `finalize_successful_turn()` finds `<axon:editor>` blocks in the accumulated assistant text.

### emit() vs emit_nonblocking()

| Function | Blocking | On Full Channel | Use For |
|----------|----------|-----------------|---------|
| `emit().await` | Yes | Waits (backpressure) | Critical events: EditorWrite, TurnResult, permission requests |
| `emit_nonblocking()` | No | Silent drop | Low-priority logs, progress text — never block the hot path |

### finalize_successful_turn()

Called from both one-shot and persistent paths after a turn completes:

1. Convert `StopReason` to string; log at `Warn` for limit/refusal/cancel
2. Emit `Log` event (fire-and-forget)
3. Parse all `<axon:editor>` blocks from accumulated `assistant_text`
4. For each: emit `ServiceEvent::EditorWrite` (blocking) with operation (`replace` | `append`)
5. Emit `ServiceEvent::AcpBridge { event: AcpBridgeEvent::TurnResult }` (blocking)
6. Emit completion `Log` event (fire-and-forget)

---

## Session Cache & WS Reconnect

### SESSION_CACHE

```rust
pub static SESSION_CACHE: std::sync::LazyLock<AcpSessionCache> =
    std::sync::LazyLock::new(AcpSessionCache::new);
```

Process-wide. Keyed by `agent_key` (encodes agent type + MCP fingerprint + capability flags).

### Hardcoded Constants

| Constant | Value | Meaning |
|----------|-------|---------|
| `SESSION_TTL` | 30 min | Idle session eviction threshold |
| `SESSION_HUNG_TURN_THRESHOLD` | 5 min | In-flight turn timeout (triggers eviction) |

> **Warning: Interaction hazard.** `SESSION_HUNG_TURN_THRESHOLD` is hardcoded at 5 minutes. If `AXON_ACP_TURN_TIMEOUT_MS` is set above 300,000ms, the session reaper will evict the session as "hung" before the turn's own timeout fires, killing the adapter mid-response. Both values must be kept in sync.
| `MAX_REPLAY_BUFFER` | 4096 messages | Per-session replay buffer count cap |
| `MAX_REPLAY_BUFFER_BYTES` | 4 MiB | Per-session replay buffer byte cap |
| Reaper interval | 60 seconds | Background cleanup frequency |
| `AXON_ACP_MAX_CONCURRENT_SESSIONS` | 8 (env override) | Semaphore limit on concurrent ACP sessions |

> **Configuration note:** `SESSION_TTL`, `MAX_REPLAY_BUFFER`, `MAX_REPLAY_BUFFER_BYTES`, and the reaper interval are hardcoded constants — they are **not** configurable via environment variables. Only `AXON_ACP_MAX_CONCURRENT_SESSIONS` can be overridden at runtime via env var.

### CachedSession

```rust
pub struct CachedSession {
    pub handle: Arc<AcpConnectionHandle>,
    pub permission_responders: PermissionResponderMap,
    last_active: Mutex<Instant>,
    replay_buffer: Mutex<Vec<String>>,
    replay_buffer_bytes: Mutex<usize>,
    turn_in_flight_since: Mutex<Option<Instant>>,
    last_turn_completed_at: Mutex<Option<Instant>>,
}
```

### Replay Buffer — Drain-on-Read (M-6)

Events are buffered when the WS connection is offline (when `tx.send()` fails). On reconnect:

```rust
pub fn read_replay_buffer(&self) -> Vec<String> {
    // drains and clears the buffer — returns all buffered events ONCE
}
```

First reconnect receives all buffered events. Subsequent reconnects see only events buffered after the previous drain. This prevents re-replay of already-delivered events.

Limits: 4096 messages OR 4 MiB cumulative. Messages exceeding either limit are silently dropped.

### WS Reconnect Flow

1. Client disconnects (WS drops)
2. Events continue arriving, buffered in `session.replay_buffer`
3. Client reconnects with a new WebSocket
4. Client sends `acp_resume { session_id: "sess-xyz" }`
5. Server: `SESSION_CACHE.get_by_session_id("sess-xyz")` → resolves via `session_id_index`
6. **H-8 Connection Binding:** First resume binds `session_id → conn_id` in `session_ownership`; subsequent resumes from different `conn_id` are rejected with `"session bound to another connection"`
7. Server drains replay buffer, sends all events to new WS connection
8. Sends `acp_resume_result { ok: true, replayed: N }`

### Reaper

Started lazily via `std::sync::Once` on first session insertion. Runs every 60 seconds:

1. **Pass 1**: Snapshot all sessions (clone keys + Arc refs, release DashMap lock immediately)
2. **Pass 2**: Check expiry (`is_expired()` or `is_turn_hung()`) — no DashMap locks held
3. **Pass 3**: Remove expired set (small subset, can hold locks briefly)

Eviction triggers: idle > 30 min OR current turn in-flight > 5 min.

---

## Prewarming

Eliminates cold-start latency (~45s) for the first Pulse Chat message.

**Controlled by:** `AXON_ACP_PREWARM` (env, default `true`)

**Trigger:** On server startup in `start_server()`, 2 seconds after bind:

```rust
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(2)).await;
    prewarm_adapter(&cfg, PulseChatAgent::Claude).await;
});
```

**What it does:**
1. Resolve default Claude adapter command (no MCP servers, `enable_fs=true`, `enable_terminal=true`)
2. Skip if already cached (idempotent)
3. `AcpConnectionHandle::spawn()` — lazy init
4. Send a warm-ping turn: `"Respond with exactly: WARM"` with `PREWARM_TURN_TIMEOUT`
5. Insert into `SESSION_CACHE` under the agent key
6. Drain events (5s timeout) — warm-ping response discarded

**Cache key matching:** First user request matches if it uses the same agent, MCP servers, and capability flags. Different MCP configuration = cache miss = fresh spawn.

**Working directory:** `$AXON_DATA_DIR/prewarm` (default: `~/.axon/prewarm`; created if absent)

**Failure behavior:** Non-fatal — if prewarm fails, first user request cold-starts normally.

---

## ACP-Backed LLM Completions

Non-interactive CLI commands route LLM calls through an ACP adapter subprocess rather than directly calling an OpenAI-compatible API.

**Commands using this path:** `ask`, `research`, `suggest`, `debug`, `evaluate`

### Core API

```rust
// Non-streaming
pub async fn complete_text(
    cfg: &Config,
    req: AcpCompletionRequest,
) -> Result<AcpCompletionResponse, Box<dyn Error>>

// Streaming with callback
pub async fn complete_streaming<F>(
    cfg: &Config,
    req: AcpCompletionRequest,
    on_delta: F,
) -> Result<AcpCompletionResponse, Box<dyn Error>>
where F: FnMut(&str) -> Result<(), Box<dyn Error>> + Send
```

### Warm Session Pattern

Overlaps adapter cold start with parallel work (e.g., Tavily search):

```rust
let warm = warm_session(&cfg, tx.clone())?;  // returns immediately, spawn in background
let search_results = do_tavily_search().await;  // parallel
let response = warm.complete_streaming(req, on_delta).await?;  // uses pre-warmed connection
```

### Timeouts

| Constant | Value | Scope |
|----------|-------|-------|
| `ACP_COMPLETION_TIMEOUT_SECS` | 300s | Per-completion overall timeout |
| One-shot exit grace | 10s | Graceful shutdown after turn |

ACP uses separate timeout classes instead of one global subprocess timer:

| Timeout | Default | Source | Behavior |
|---------|---------|--------|----------|
| Persistent adapter loop | 3600s, override via `adapter_timeout_secs` | `crates/services/acp/persistent_conn.rs` | Bounds the lifetime of the background ACP adapter loop. |
| Permission event delivery | 5s | `crates/services/acp/bridge.rs` | Cancels the turn if the frontend permission request cannot be delivered. |
| Permission response wait | 60s, override via `permission_timeout_secs` | `crates/services/acp/permission.rs` | Resolves the tool call as `Cancelled` and drops the responder map entry. |
| Turn cancellation drain | 15s | `crates/services/acp/persistent_conn/turn.rs` | After WebSocket disconnect, sends `session/cancel` and waits briefly for `PromptResponse::Cancelled`. |
| Adapter exit grace | 10s | `crates/services/acp/runtime.rs` | Gives the adapter time to flush state before kill-on-drop handles enforce cleanup. |

When changing these values, keep the class-specific behavior intact: permission
timeouts must clean up responder entries, cancellation drains must not outlive
WebSocket teardown, and adapter loop timeouts must remain bounded.

### Configuration

| Env Var | Purpose |
|---------|---------|
| `AXON_ACP_ADAPTER_CMD` | Adapter binary (required) |
| `AXON_ACP_ADAPTER_ARGS` | Pipe-delimited (`|`) list of args (optional). Spaces within each segment are preserved; pipe is the only separator. Example: `--stdio|--model|gemini-3-flash-preview`. Do NOT use spaces as delimiters. |
| `OPENAI_MODEL` | Model override for ACP completion calls |

---

## WebSocket Message Protocol

### Inbound Messages (Client → Server)

All messages are parsed by `ws_handler.rs::handle_ws_message()` based on the `"type"` field.

#### `execute`

```json
{
  "type": "execute",
  "mode": "pulse_chat",
  "input": "hello world",
  "flags": {
    "agent": "claude",
    "session_id": "sess-xyz",
    "enable_fs": true,
    "enable_terminal": false
  },
  "id": "exec-123"
}
```

**Fields:** `mode` (required), `input` (required), `flags` (optional object), `id` (optional, auto-generated as `exec-{uuid}` if absent)

#### `cancel`

```json
{ "type": "cancel", "mode": "crawl", "id": "crawl-job-123" }
```

If `id` is empty, cancels ALL tracked jobs for this connection (M-5).

> **Warning: Known limitation.** The `cancel` WebSocket message does **not** cancel in-progress ACP turns. The adapter will continue processing until its turn timeout expires or the WebSocket connection closes. `session/cancel` protocol support is not yet implemented — this is a known gap.

#### `acp_resume`

```json
{ "type": "acp_resume", "session_id": "sess-abc123" }
```

#### `permission_response`

```json
{
  "type": "permission_response",
  "tool_call_id": "call-xyz",
  "option_id": "allow_once",
  "session_id": "sess-abc123"
}
```

#### `read_file`

```json
{ "type": "read_file", "path": "relative/path/in/output_dir" }
```

Rate-limited (20/min per IP).

#### `subscribe_stats` / `unsubscribe_stats`

```json
{ "type": "subscribe_stats" }
```

Opts in/out of Docker container stats broadcast (every 500ms).

---

### Outbound Messages (Server → Client)

All structured results use `WsEventV2` format:

#### `command.start`

```json
{
  "type": "command.start",
  "data": { "ctx": { "exec_id": "exec-123", "mode": "pulse_chat", "input": "hello" } }
}
```

#### `command.output.json`

```json
{
  "type": "command.output.json",
  "data": {
    "ctx": { "exec_id": "exec-123", "mode": "pulse_chat", "input": "hello" },
    "data": {
      "type": "assistant_delta",
      "session_id": "sess-abc123",
      "delta": "streaming text"
    }
  }
}
```

For ACP events, the inner `"data"` object is the full serialized `AcpBridgeEvent`.

#### `command.done`

```json
{
  "type": "command.done",
  "data": {
    "ctx": { "exec_id": "exec-123", "mode": "pulse_chat", "input": "hello" },
    "payload": { "exit_code": 0, "elapsed_ms": 12345 }
  }
}
```

#### `command.error`

```json
{
  "type": "command.error",
  "data": {
    "ctx": { "exec_id": "exec-123", "mode": "pulse_chat", "input": "hello" },
    "payload": { "message": "ACP session queue full", "elapsed_ms": 50 }
  }
}
```

#### `acp_resume_result`

```json
{
  "type": "acp_resume_result",
  "ok": true,
  "session_id": "sess-abc123",
  "replayed": 42
}
```

Error cases:
```json
{ "type": "acp_resume_result", "ok": false, "reason": "session not found" }
{ "type": "acp_resume_result", "ok": false, "reason": "session bound to another connection" }
```

---

## WsConnState

Per-connection state created on WS upgrade. Shared between the read loop, forward task, and spawned command tasks.

| Field | Type | Purpose |
|-------|------|---------|
| `exec_tx` | `mpsc::Sender<String>` (256-slot) | Exec output → forward task → WS sink |
| `tracking_tx` | `mpsc::Sender<String>` (256-slot) | File reads / tracking → WS sink |
| `crawl_job_ids` | `Arc<Mutex<Vec<String>>>` | M-5: cumulative job IDs for cancel-all |
| `permission_responders` | `PermissionResponderMap` | DashMap `(session_id, tool_call_id) → oneshot::Sender<option_id>` |
| `conn_id` | `String` | UUID for this connection (H-8 binding) |
| `session_ownership` | `Arc<DashMap<String, String>>` | `session_id → conn_id` (H-8) |
| `client_ip` | `IpAddr` | For rate limiting |
| `rate_limiter` | `Arc<DashMap<...>>` | Process-wide sliding window rate limit state |
| `stats_subscribed` | `Arc<AtomicBool>` | M-12: stats opt-in |

**Forward task biased select (M-16):** Polls three channels with explicit priority — exec output first, tracking second, stats last. Prevents Docker stats from starving user-visible results.

---

## Security: ALLOWED_MODES and ALLOWED_FLAGS

Both whitelists are enforced in `handle_execute_msg()` before any subprocess spawn or service dispatch (L-1). Unknown mode or unknown flag key → `command.error` sent immediately, no action taken.

### ALLOWED_MODES (33 modes)

| Category | Modes |
|----------|-------|
| Vector / Search | `scrape`, `map`, `query`, `retrieve`, `ask`, `search`, `research` |
| System | `stats`, `sources`, `domains`, `doctor`, `status` |
| Diagnostic | `debug`, `suggest`, `screenshot`, `evaluate`, `dedupe`, `sessions` |
| Async Jobs | `crawl`, `extract`, `embed`, `github`, `reddit`, `youtube` |
| ACP / Chat | `pulse_chat`, `pulse_chat_probe` |
| Internal | `mcp_refresh` |

### ALLOWED_FLAGS (45 flags)

Key ACP-related flags:

| Flag | Purpose |
|------|---------|
| `agent` | Agent selection: `claude` / `codex` / `gemini` |
| `model` | Model override forwarded to adapter |
| `session_id` | Resume an existing ACP session |
| `session_mode` | Approval mode (e.g., `"auto"`) |
| `mcp_servers` | JSON array of MCP server configs |
| `blocked_mcp_tools` | Tool names to auto-cancel |
| `enable_fs` | Grant filesystem access (default true) |
| `enable_terminal` | Grant terminal access (default true) |
| `permission_timeout_secs` | Permission wait timeout |
| `adapter_timeout_secs` | Adapter process overall timeout |
| `assistant_mode` | Toggle assistant mode |

Full flag list in `crates/web/execute/constants.rs`.

---

## Concurrent Session Limits

### ACP_SESSION_SEMAPHORE

```rust
pub(crate) static ACP_SESSION_SEMAPHORE: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| Semaphore::new(
        std::env::var("AXON_ACP_MAX_CONCURRENT_SESSIONS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(8)
    ));
```

**Acquired** before `handle_sync_direct()` for `pulse_chat` and `pulse_chat_probe` modes.

**Timeout:** 30 seconds. If no slot available: sends "Waiting for available session slot..." to client (M-11), then sends error if timeout expires.

**Non-ACP sync modes** use a separate `SYNC_MODE_SEMAPHORE` (default 16) — ACP modes do not acquire this one. Prevents double-counting.

### Rate Limiting

Per-IP sliding window rate limits (1-minute windows):

| Category | Limit | Env Override |
|----------|-------|-------------|
| Execute commands | 10/min | — |
| `read_file` requests | 20/min | — |

Separate counters — burst of file reads cannot reset the execute window.

### Token Auth

WS endpoint (`/ws`) requires `AXON_WEB_API_TOKEN` as `?token=` query param (WebSocket upgrade requests cannot carry custom headers). If unset: gate is open (for trusted-network deployments only).

---

## Validation Rules

### validate_prompt_turn_request

- `prompt.len() > 0` — at least one prompt block required
- `session_id.trim().is_empty()` → rejected if provided but blank

### validate_probe_request

- `session_id.trim().is_empty()` → rejected if provided but blank

### validate_session_cwd

- Must be an absolute path
- Must exist (`path.exists()`)
- Must be a directory (`path.is_dir()`)

### validate_adapter_command

See [Security Validation](#security-validation).

---

## Config Options

`AcpConfigOption` represents a selectable option the frontend can display (model picker, mode selector, etc.):

```rust
pub struct AcpConfigOption {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,    // "model", "mode", "thought_level"
    pub current_value: String,
    pub options: Vec<AcpConfigSelectValue>,
}

pub struct AcpConfigSelectValue {
    pub value: String,
    pub name: String,
    pub description: Option<String>,
}
```

**Validation:** If `current_value` is not in the `options` list, the config option is dropped entirely (prevents invalid frontend state).

**Fallback for Codex:** If the adapter session response contains no config options, reads `~/.codex/models_cache.json`.

**Fallback for Gemini:** If the adapter session response contains no config options, reads `GEMINI_CLI_HOME/settings.json` (or `~/.gemini/settings.json`).

---

## AcpRuntimeState

Holds mutable per-session state for the bridge. Uses `RefCell` (not `Mutex`) because all access is single-threaded inside a `LocalSet` on a `current_thread` runtime.

| Field | Type | Purpose |
|-------|------|---------|
| `current_session_id` | `RefCell<Option<String>>` | Updated on every session notification |
| `assistant_text` | `RefCell<String>` | Accumulated response (1 MiB cap) |
| `service_tx` | `RefCell<Option<mpsc::Sender<ServiceEvent>>>` | Updated per-turn so callbacks route to active turn's channel |
| `current_turn_id` | `Cell<u64>` | Monotonic counter — late delta detection |
| `established_model` | `RefCell<Option<String>>` | Model currently applied to session |
| `config_options` | `RefCell<Vec<AcpConfigOption>>` | Latest from session setup + runtime updates |
| `blocked_mcp_tools` | `RefCell<HashSet<String>>` | Auto-cancelled tool names (lowercased) |
| `permission_timeout_secs` | `Cell<Option<u64>>` | Permission wait timeout override |
| `limit_warning_emitted` | `Cell<bool>` | Guard: emit 1 MiB warning at most once |

**Late delta detection:** `current_turn_id` is incremented at the start of each turn. If a notification arrives with an older turn ID, the delta is dropped — prevents attributing late results to the wrong turn.

---

## Preflight: Codex Skill Symlinks

Before spawning a Codex adapter, `spawn_adapter()` calls `repair_codex_skill_symlinks()`.

**Why:** Codex skill symlinks can become dangling when skills are moved or deleted without unlinking the symlink. Dangling symlinks cause Codex startup failures.

**What it does:**
1. Read `${XDG_CONFIG_HOME}/Codex/skills/` (or `~/.config/Codex/skills/`)
2. For each entry: check if it's a symlink whose target is missing
3. Remove dangling symlinks, leave valid ones

**Stats returned:**
```rust
pub struct SymlinkRepairStats {
    pub scanned_symlinks: usize,
    pub removed_dangling_symlinks: usize,
    pub failed_removals: usize,
}
```

Logged at `WARN` if anything was removed or failed. Silent on clean runs.

---

## Test Coverage

### Unit Tests

| File | What It Covers |
|------|---------------|
| `tests/services_acp_security.rs` | Shell blocklist, env allowlist, session update kind collisions, SEC-7 composite key isolation |
| `tests/services_acp_spawn_env.rs` | Subprocess env isolation: `CLAUDECODE` stripped, `OPENAI_*` stripped, `GEMINI_API_KEY` passed through |
| `tests/services_acp_smoke.rs` | Scaffold construction, empty program rejection, request construction |
| `tests/services_acp_lifecycle.rs` | Initialize request build, session setup (new vs load), blank session_id rejection |
| `tests/services_acp_bridge_event_serialize.rs` | Wire shapes: `"delta"` vs `"content"` key, `"usage"` nested object, conditional cost fields |
| `tests/services_acp_event_mapping.rs` | SDK event → service type mapping |
| `tests/services_acp_llm.rs` | ACP-backed LLM completion integration |
| `crates/web/execute/tests/acp_ws_event_tests.rs` | ACP bridge events → WS output.json payloads |
| `crates/services/acp/session_cache.rs` (inline) | Insert/get/remove, replay buffer drain-on-read, byte limits, reaper eviction, `get_sync` no-touch |
| `crates/services/acp.rs` (inline) | Dangling symlink repair |

### Key Security Scenarios Tested

- `CLAUDECODE` env var is stripped from subprocess → prevents nested-session detection
- `OPENAI_BASE_URL`, `OPENAI_API_KEY`, `OPENAI_MODEL` are stripped → prevents LLM proxy leak
- `GEMINI_API_KEY`, `GOOGLE_API_KEY` are forwarded → Gemini auth works
- Bare shell names (`sh`, `bash`, etc.) are rejected by validator
- `(session_id, tool_call_id)` composite key prevents cross-session permission injection (SEC-7)
- Unknown `AcpSessionUpdateKind` serializes as `"unknown"` (not `"status"`) — regression test

---

## Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| **`!Send` ACP SDK → `spawn_blocking` + `LocalSet`** | ACP SDK futures use `?Send` trait bounds. Encapsulating in a `current_thread` runtime inside `spawn_blocking` is the safe boundary. |
| **`RefCell` not `Mutex` in `AcpRuntimeState`** | Single-threaded via `LocalSet`. Eliminates lock overhead on the hot streaming token path. Compiler enforces via `?Send` bounds on `Client` trait. |
| **`DashMap` for `PermissionResponderMap`** | Shard-level locking vs global `Mutex<HashMap>`. Bridge inserts from ACP callback; WS handler removes on response — high-concurrency, low-contention pattern. |
| **`(session_id, tool_call_id)` composite key** (SEC-7) | Prevents cross-session collision if `tool_call_id` values happen to match between concurrent sessions. |
| **`env_clear()` + allowlist for subprocess** | Defense against OPENAI_* proxy leakage and CLAUDECODE nested-session detection. Parent env is never modified. |
| **10s exit grace period (one-shot)** | Allows adapter to write session `.jsonl` before SIGKILL. Without this wait, `kill_on_drop(true)` fires immediately when `conn` is dropped. |
| **Drain-on-read replay buffer** (M-6) | First reconnect gets all buffered events; buffer clears. Prevents stale event re-replay on subsequent reconnects. Bounded at 4096 messages / 4 MiB. |
| **Connection binding for session ownership** (H-8) | Session IDs are UUIDs — unguessable. Binding session to `conn_id` on first resume prevents cross-connection session hijacking without requiring explicit permission tokens. |
| **Cache key encodes MCP + capabilities** | Different MCP server configs or capability flags require different adapter processes. Can't share a process across differing configurations. |
| **30-minute session TTL** | Typical interactive session lasts < 30 min. Configurable via `SESSION_TTL` constant (not an env var). |
| **Two-pass reaper** | Pass 1 snapshots hold no locks; Pass 2 checks expiry lock-free; Pass 3 removes small subset. Prevents long lock hold during large cache iteration. |
| **Lazy vs eager spawn** | Lazy: setup progress appears in first turn's event stream. Eager: overlaps 45s cold start with parallel work (Tavily, Qdrant search). |
| **Biased select in forward task** (M-16) | Exec output > tracking > stats. Docker stats must not starve user-visible results during heavy crawls. |
| **Dual semaphore design** | `ACP_SESSION_SEMAPHORE` for ACP modes only; `SYNC_MODE_SEMAPHORE` for non-ACP sync modes. Prevents double-counting and keeps ACP concurrency configuration consistent. |
| **ALLOWED_MODES + ALLOWED_FLAGS before dispatch** (L-1) | Unknown modes/flags rejected before any subprocess spawns. No leakage, no ambiguous behavior. |
| **`option_ids` → `"options"` wire rename** | Frontend's existing JSON schema expected `"options"` key. Renamed in custom Serialize impl without changing the Rust field name. |
| **Late delta detection via turn ID counter** | Monotonic `current_turn_id` prevents attributing delayed streaming tokens from a timed-out turn to the new active turn. |
| **Model override validation against adapter's options** | Prevents invalid model values from reaching the adapter. If the model isn't in the advertised allowed values, the request is silently ignored (keeps current model). |
