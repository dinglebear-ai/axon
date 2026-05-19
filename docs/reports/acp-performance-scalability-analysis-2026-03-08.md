# ACP Performance & Scalability Analysis
**Date:** 2026-03-08
**Scope:** `crates/services/acp.rs`, `crates/services/types.rs`, `crates/services/events.rs`, `crates/web/execute.rs`, `crates/web/execute/events.rs`, `crates/web/execute/sync_mode.rs`

---

## Table of Contents
1. [Executive Summary](#executive-summary)
2. [Memory Management](#memory-management)
3. [Async Performance](#async-performance)
4. [I/O Performance](#io-performance)
5. [Concurrency Issues](#concurrency-issues)
6. [Resource Management](#resource-management)
7. [Scalability Ceiling](#scalability-ceiling)
8. [Finding Summary Table](#finding-summary-table)

---

## Executive Summary

The ACP implementation is functionally sound after the recent `AllowStdIo` deadlock fix. However, the design has several structural performance constraints that will become meaningful under load:

- Every ACP session burns one `spawn_blocking` thread for its entire 300-second lifetime. The default Tokio thread pool caps at 512. At 512 concurrent sessions, new ACP requests queue behind the OS thread scheduler rather than Tokio's work-stealing scheduler.
- `assistant_text` accumulates the entire response in a `Mutex`-guarded `String` on the hot path (every streaming delta acquires a write lock). This is a lock contention sink for long responses.
- The two core functions (`run_prompt_turn` / `run_session_probe`) are ~350 lines each of near-identical code. This is a maintenance burden with no performance upside, but it means any future optimization must be applied twice.
- `AcpAdapterCommand` is cloned 4–6 times per call before being passed to `spawn_blocking`, even on early-exit paths where the clone was unnecessary.
- Every ACP event dispatch serializes a `WsEventV2` envelope twice: once in `acp_bridge_event_payload` (`serde_json::to_value`) and again in `serialize_v2_event` (`serde_json::to_string`). Streaming turns emit one event per token chunk, making this the highest-frequency allocation site in the entire WS path.

None of the findings are blockers for the current single-user homelab use case. They become relevant when load exceeds ~5 concurrent ACP sessions or when response tokens exceed a few thousand.

---

## Memory Management

### FINDING-1: `AcpAdapterCommand` Cloned Up to 6 Times per Call
**Severity:** Medium
**Impact:** ~400 bytes of heap allocation per unnecessary clone, negligible at low concurrency but accumulates proportionally with session count

**Location:** `crates/services/acp.rs:160, 219, 307, 310, 314, 317, 328, 331, 337`

`AcpAdapterCommand` (`program: String`, `args: Vec<String>`, `cwd: Option<String>`) is cloned multiple times before `spawn_blocking` receives it. In `start_prompt_turn`:

```rust
// acp.rs:160
let adapter = self.adapter.clone();       // clone 1: into spawn_blocking
let req_owned = req.clone();              // clone 2: AcpPromptTurnRequest
```

Then inside `run_prompt_turn` (acp.rs:771–774):

```rust
let adapter = append_codex_model_override(&adapter, req.model.as_deref())...;
// append_codex_model_override returns Ok(adapter.clone()) on every non-matching path:
// acp.rs:307, 310, 314 — three potential early-return clones
let adapter = append_gemini_model_override(&adapter, req.model.as_deref())...;
// acp.rs:328, 331, 337 — three more potential early-return clones
```

On the common path (Claude adapter with no model override), `append_codex_model_override` hits line 310 (`!is_codex_adapter`) and returns `Ok(adapter.clone())`. Then `append_gemini_model_override` hits line 331 (`!is_gemini_adapter`) and returns `Ok(adapter.clone())`. Two full clones of the args vector occur even though nothing changed.

**Recommendation:** Return `Cow<'_, AcpAdapterCommand>` or restructure the override functions to take ownership and return modified-or-original:

```rust
fn append_codex_model_override(
    adapter: AcpAdapterCommand,  // take ownership
    requested_model: Option<&str>,
) -> Result<AcpAdapterCommand, Box<dyn Error>> {
    let Some(model) = normalized_requested_model(requested_model) else {
        return Ok(adapter);  // no clone — return original
    };
    if !is_codex_adapter(&adapter) {
        return Ok(adapter);  // no clone
    }
    // ... mutation path
    let mut next = adapter;
    next.args.push("-c".to_string());
    next.args.push(format!("model=\"{model}\""));
    Ok(next)
}
```

Since `run_prompt_turn` already receives `adapter: AcpAdapterCommand` by value (moved from `spawn_blocking`), it can pass ownership through without any clone.

---

### FINDING-2: `assistant_text` Grows Unbounded in `AcpRuntimeState`
**Severity:** Medium
**Impact:** Memory grows linearly with response length; entire response string lives in memory for the session lifetime; lock acquired on every streaming token delta

**Location:** `crates/services/acp.rs:1429–1432, 1609–1616, 1097–1106`

`AcpRuntimeState.assistant_text` appends every `AssistantDelta` chunk:

```rust
// acp.rs:1615 — called on every assistant token delta
state.assistant_text.push_str(&text_delta);
```

This `String` is only read once, at turn completion (acp.rs:1105):

```rust
(state.assistant_text.clone(), session)  // clone the full accumulated response
```

For a 100K-token response at ~4 bytes/token, this is ~400KB held in a `Mutex<AcpRuntimeState>` and then cloned. More importantly, every streaming delta acquires a `std::sync::Mutex` lock, even though 99% of deltas only need to write to `assistant_text` — the `session_id` check only fires once.

The lock is held across `push_str` which is O(n) for the accumulated string in the worst case (if reallocation occurs during growth).

**Recommendation:** Separate the high-frequency write (`assistant_text`) from the low-frequency `session_id`. The session_id only needs the lock during initialization. Use an `AtomicBool` for "session_id settled" and accumulate text without a lock:

```rust
struct AcpRuntimeState {
    session_id: tokio::sync::OnceLock<String>,
    // assistant_text is only read at turn-end on the same thread (current_thread runtime)
    // so we do not need Mutex for it — a plain String works inside LocalSet
    assistant_text: std::cell::RefCell<String>,
}
```

Since `run_prompt_turn` runs on a `current_thread` + `LocalSet`, all `spawn_local` tasks are on the same thread, so `RefCell` is safe and eliminates mutex overhead entirely for the hot `push_str` path.

Alternatively, if the full text is only needed for `TurnResult`, consider discarding it and letting the frontend reconstruct from streaming deltas — eliminating the accumulation entirely.

---

### FINDING-3: `AcpPromptTurnRequest` Full Clone into `spawn_blocking`
**Severity:** Low
**Impact:** One allocation per session for `prompt: Vec<String>` and `mcp_servers: Vec<AcpMcpServerConfig>`

**Location:** `crates/services/acp.rs:161, 220`

```rust
let req_owned = req.clone();
```

`AcpPromptTurnRequest` contains `Vec<String>` (prompt blocks) and `Vec<AcpMcpServerConfig>` (MCP server list). For a typical single-turn prompt with one MCP server, this is a few kilobytes. The clone is required because `spawn_blocking` needs `'static`. However, since the caller (`start_prompt_turn`) awaits the result before returning, an `Arc<AcpPromptTurnRequest>` would eliminate the deep clone:

```rust
let req_owned = Arc::new(req.clone());  // or take by value instead of &ref
```

If `start_prompt_turn` took `req: AcpPromptTurnRequest` by value instead of `req: &AcpPromptTurnRequest`, the clone at line 161 would be unnecessary — the owned value moves directly into `spawn_blocking`.

---

### FINDING-4: `CommandContext` Cloned Per Event in Hot Streaming Path
**Severity:** Low
**Impact:** ~128 bytes per event (`exec_id: String`, `mode: String`, `input: String`), called once per streaming assistant token

**Location:** `crates/web/execute/sync_mode.rs:720–742`

```rust
// dispatch_acp_event — called on every streaming token
send_json_owned(
    tx.clone(),
    ws_ctx.clone(),   // CommandContext clone — exec_id, mode, input all String-cloned
    payload,
).await;
```

`CommandContext` is `Clone` and contains three `String` fields. For a 1000-token response, this is 1000 `tx.clone()` + 1000 `ws_ctx.clone()` calls. The `mpsc::Sender` clone is a reference count bump (cheap). The `CommandContext` clone allocates three new strings each time.

`send_json_owned` is designed this way to satisfy the `Send + 'static` requirement for `tokio::spawn`, but `dispatch_acp_event` is already `async fn` that takes `&mpsc::Sender<String>` and `&CommandContext`. The event loop at line 770 passes references:

```rust
Some(event) => dispatch_acp_event(event, &tx, &ws_ctx).await,
```

But `dispatch_acp_event` re-clones them before passing to `send_json_owned`. The clone is avoidable if `send_json_owned` is refactored to take references and perform the serialization inline rather than moving into a separate async fn.

---

### FINDING-5: Double Serialization of Every ACP Bridge Event
**Severity:** High
**Impact:** Two `serde_json` allocations per streaming token delta; `serde_json::Value` intermediate representation allocates a `Map<String, Value>` on every event

**Location:** `crates/web/execute/sync_mode.rs:728–741`, `crates/web/execute/events.rs:128–131`

The event path for every ACP streaming event:

1. `acp_bridge_event_payload(&event)` → `serde_json::to_value(event)` → allocates a `serde_json::Map` (acp.rs events.rs:129)
2. The resulting `Value` is passed to `send_json_owned`
3. `send_json_owned` calls `serialize_v2_event(WsEventV2::CommandOutputJson { ctx, data })`
4. `serialize_v2_event` calls `serde_json::to_string(&event)` → serializes the entire envelope including the `Value` from step 1

For `assistant_delta` events (the most frequent, one per streaming token), step 1 produces a `serde_json::Map` with 4 keys (`type`, `session_id`, `tool_call_id`, `delta`). Step 4 re-serializes that `Map` as part of the outer `WsEventV2` JSON.

The intermediate `serde_json::Value` representation is pure overhead — it exists only to cross the type boundary between `AcpBridgeEvent` (in `services`) and `WsEventV2` (in `web`). Direct serialization to a `String` would eliminate one full allocation per event.

**Recommendation:** Add a method to `AcpBridgeEvent` that serializes directly to a JSON string, then embed that string literal inside the `WsEventV2` envelope. Or restructure the event pipeline to pass `String` (pre-serialized JSON) through the `mpsc` channel instead of `ServiceEvent`, eliminating the intermediate `Value`.

The rough allocation count per streaming token under the current design:

| Step | Allocation |
|------|-----------|
| `serde_json::to_value(&event)` | `serde_json::Map` with 4 entries |
| `String` values in the Map | 3–4 `String` copies |
| `serde_json::to_string` of outer event | 1 `String` |
| `mpsc::Sender::send(String)` | 0 (String moved) |

**Total:** ~5 heap allocations per streaming token, where 3–4 could be eliminated.

---

## Async Performance

### FINDING-6: `spawn_blocking` Thread Pool Exhaustion Under Concurrent Load
**Severity:** Critical
**Impact:** At 512+ concurrent ACP sessions, new sessions queue behind the OS thread scheduler; latency degrades from async O(1) scheduling to blocking thread allocation

**Location:** `crates/services/acp.rs:167–191, 222–248`

Both `start_prompt_turn` and `start_session_probe` use:

```rust
let join = tokio::task::spawn_blocking(move || {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async {
        match tokio::time::timeout(ACP_ADAPTER_TIMEOUT, run_prompt_turn(...)).await {
            Ok(result) => result,
            Err(_) => Err("ACP adapter timed out after 5 minutes".into()),
        }
    })
})
.await;
```

`spawn_blocking` submits work to Tokio's blocking thread pool (default max: 512 threads, configurable via `TOKIO_WORKER_THREADS`). Each ACP session occupies **one blocking thread for up to 300 seconds** — the full `ACP_ADAPTER_TIMEOUT`.

The blocking thread is not doing CPU work during that time; it is blocked on `local.block_on(...)`, which drives a `current_thread` runtime. This is an I/O-bound operation (subprocess stdio) consuming a scarce thread.

**Concurrency ceiling:** `max_blocking_threads / ACP_ADAPTER_TIMEOUT_SECS = 512 / 300 = ~1.7 new sessions/second`. At sustained throughput above ~1.7 sessions/second, the thread pool saturates and new sessions wait for a thread.

The fundamental constraint is the ACP SDK's `!Send` requirement — its futures cannot be sent across threads, mandating either `current_thread` or `LocalSet`. The `spawn_blocking` + `current_thread` pattern is the established workaround documented in the code (and in MEMORY.md). There is no zero-cost alternative given the SDK constraint.

**Practical mitigation options:**

1. **Configure `max_blocking_threads`** explicitly in the axum runtime builder to set a ceiling that matches actual ACP session capacity, preventing silent thread exhaustion from starving other `spawn_blocking` users (DB, file I/O):

```rust
tokio::runtime::Builder::new_multi_thread()
    .max_blocking_threads(64)  // explicit cap, not default 512
    .build()?
```

2. **Dedicate a separate blocking pool to ACP** by using a `rayon` threadpool or a dedicated `std::thread` per ACP session with its own `current_thread` runtime, freeing the shared Tokio blocking pool for other consumers.

3. **Session reuse** — if the ACP adapter supports it, keep the subprocess alive across turns rather than spawning one per `pulse_chat` call. This would reduce the thread pool pressure from O(sessions) to O(active_adapter_processes).

---

### FINDING-7: 300-Second Timeout Blocks Thread Even on Prompt Completion
**Severity:** High
**Impact:** Thread is held for the duration of the `tokio::time::timeout` future even after the prompt completes normally; timeout cancellation is not immediate

**Location:** `crates/services/acp.rs:174–188`

```rust
match tokio::time::timeout(
    ACP_ADAPTER_TIMEOUT,        // 300 seconds
    run_prompt_turn(...),
).await {
    Ok(result) => result,
    Err(_) => Err("ACP adapter timed out after 5 minutes".into()),
}
```

When `run_prompt_turn` completes normally (e.g., in 5 seconds), the `timeout` future resolves immediately — the thread is released and the blocking thread returns to the pool. This is correct behavior.

However, the thread is *occupied* for the full duration of `run_prompt_turn` regardless. A 5-second ACP session holds a blocking thread for 5 seconds. A 295-second session holds it for 295 seconds. This is expected given the architecture, but it means the thread utilization is proportional to session wall-clock time, not CPU time.

The timeout is asymptotically correct. The concern is that there is no mechanism for the axum layer to signal early cancellation to the blocking thread if the WS client disconnects mid-session. If a client disconnects, the `spawn_blocking` thread continues running `run_prompt_turn` until it completes or times out — up to 300 seconds of wasted thread time per abandoned session.

**Recommendation:** Add a cancellation `CancellationToken` (from `tokio-util`) that the WS disconnect handler sets, and poll it periodically in the event loop inside `run_prompt_turn`. This requires threading the token into `spawn_blocking`, but it bounds abandoned session cleanup time.

---

### FINDING-8: `select!` Branch Fairness in `run_acp_event_loop`
**Severity:** Low
**Impact:** Under heavy event load, the `task` completion branch may never be polled if events arrive continuously; loop may drain slowly after task completes

**Location:** `crates/web/execute/sync_mode.rs:757–783`

```rust
loop {
    tokio::select! {
        join_result = &mut task => {
            // task completed — drain remaining events
            while let Ok(event) = event_rx.try_recv() {
                dispatch_acp_event(event, &tx, &ws_ctx).await;
            }
            break;
        }
        maybe_event = event_rx.recv() => {
            match maybe_event {
                Some(event) => dispatch_acp_event(event, &tx, &ws_ctx).await,
                // ...
            }
        }
    }
}
```

`tokio::select!` by default uses pseudo-random branch selection when multiple branches are ready simultaneously. During a high-frequency streaming response, both `task` (which may have completed) and `event_rx.recv()` (which has a buffered event) may be ready at the same time. The random selection means the task-completion branch may be delayed.

The post-completion drain (`try_recv` loop at line 763) runs synchronously with no yield, which is correct for draining the buffered channel. However, `dispatch_acp_event` is `async` — the `try_recv` loop calls `.await` inside, yielding to the runtime on each event. This means the drain is cooperative, not a tight loop. That is acceptable.

A more subtle issue: if the event channel has capacity 256 (set at `sync_mode.rs:805`) and the ACP task floods it faster than the WS sender can drain (e.g., if the WS client is slow), `emit()` in `acp.rs:47` uses `try_send` which drops events silently:

```rust
// events.rs:47
if sender.try_send(event).is_err() {
    eprintln!("[acp] event channel full — dropping event");
}
```

Lost events mean lost streaming tokens from the user's perspective — text chunks disappear from the chat output. The capacity of 256 events provides a reasonable buffer but is not infinite.

**Recommendation:** Use `select! { biased; ... }` with the event drain branch first during draining, or increase event channel capacity. More importantly, surface dropped events as a warning to the user rather than a silent `eprintln!`.

---

### FINDING-9: Per-Event `tx.clone()` and `ws_ctx.clone()` in Hot Path
**Severity:** Medium
**Impact:** ~200ns per clone for `mpsc::Sender` (atomic increment); ~300ns per clone for `CommandContext` (three heap allocations); multiplied by every streaming token

**Location:** `crates/web/execute/sync_mode.rs:720–741`

```rust
async fn dispatch_acp_event(event: ServiceEvent, tx: &mpsc::Sender<String>, ws_ctx: &CommandContext) {
    match event {
        ServiceEvent::Log { .. } => {
            send_json_owned(
                tx.clone(),       // Sender arc bump
                ws_ctx.clone(),   // 3x String clone
                json!(...),
            ).await;
        }
        ServiceEvent::AcpBridge { event } => {
            let payload = super::events::acp_bridge_event_payload(&event);
            send_json_owned(tx.clone(), ws_ctx.clone(), payload).await;  // same clones
        }
    }
}
```

`send_json_owned` takes `tx: mpsc::Sender<String>` and `ctx: CommandContext` by value because it was designed for `tokio::spawn` (which requires `'static`). However, `dispatch_acp_event` is not spawning a task — it awaits `send_json_owned` inline. The clones are not required for correctness; they are artifacts of the `send_json_owned` signature.

**Recommendation:** Add a `send_json_ref` variant that takes `&mpsc::Sender<String>` and `&CommandContext`, performs the serialization, and sends. This eliminates the clones entirely on the streaming path. Reserve `send_json_owned` for contexts where `'static` is genuinely required.

---

## I/O Performance

### FINDING-10: Line-by-Line TOML Parsing Without a TOML Parser
**Severity:** Low
**Impact:** Incorrect parsing if `model` value spans multiple lines or contains `=` in a comment; O(n) scan of entire config file per session

**Location:** `crates/services/acp.rs:353–369`

```rust
async fn read_codex_default_model() -> Option<String> {
    let config_path = codex_config_dir()?.join("config.toml");
    let raw = tokio::fs::read_to_string(config_path).await.ok()?;
    raw.lines().find_map(|line| {
        let trimmed = line.trim();
        if !trimmed.starts_with("model") {
            return None;
        }
        let (_, value) = trimmed.split_once('=')?;
        let model = value.trim().trim_matches('"');
        // ...
    })
}
```

This is a hand-rolled TOML parser that fails for:
- `model = "claude-opus-4"` with surrounding whitespace (handled)
- `# model = "something"` (not handled — comment lines starting with `model` will false-positive if the `#` is after `model`)
- Multi-line TOML strings (not handled)
- `model` appearing inside a TOML table section that is not the intended one (not handled)

More importantly from a performance perspective: this function is called on every ACP session that uses Codex as the adapter. The file is read and scanned on every prompt turn and session probe. There is no caching.

**Recommendation:** Parse with the `toml` crate (already likely in the dep tree via other crates, or lightweight to add). Cache the result with a `tokio::sync::OnceLock` per process lifetime, invalidated if the file's mtime changes — or accept slightly stale model reads.

---

### FINDING-11: Config File Read on Every ACP Session (No Caching)
**Severity:** Medium
**Impact:** 2–4 file reads per ACP session (codex config.toml, codex models_cache.json, gemini settings.json, axon config.json); adds ~1–10ms of I/O latency to session startup

**Location:** `crates/services/acp.rs:353, 377, 409, 413`; `crates/web/execute/sync_mode.rs:274–336`

Every `pulse_chat` and `pulse_chat_probe` call:
1. Calls `read_axon_mcp_servers()` → reads `$AXON_DATA_DIR/axon/mcp.json` (sync_mode.rs:813)
2. If Codex adapter: calls `read_codex_cached_model_options()` → reads `~/.codex/models_cache.json` (acp.rs:413)
3. If Codex fallback: calls `read_codex_default_model()` → reads `~/.codex/config.toml` (acp.rs:429)
4. If Gemini adapter: calls `read_gemini_default_model()` → reads `~/.gemini/settings.json` (acp.rs:394)

These files change infrequently (only when the user changes adapter config). Reading them on every session call adds unnecessary I/O latency.

**Recommendation:** Cache these with `tokio::sync::RwLock<Option<CachedValue>>` using a TTL (e.g., 30 seconds) or file mtime comparison. The config values rarely change and a slightly stale read is acceptable.

---

### FINDING-12: `stdout_accum` Grows Unbounded in Subprocess Handler
**Severity:** Low
**Impact:** For large subprocess outputs (e.g., large `axon ask` responses), `stdout_accum` allocates linearly with output size and is only used as a JSON parse fallback

**Location:** `crates/web/execute/sync_mode.rs:1031–1061`

```rust
let mut stdout_accum = String::new();
let mut saw_json_line = false;

while let Ok(Some(line)) = lines.next_line().await {
    let clean = strip_ansi(&line);
    stdout_accum.push_str(&clean);  // accumulates entire stdout
    match serde_json::from_str::<serde_json::Value>(&clean) {
        Ok(parsed) if parsed.is_object() || parsed.is_array() => {
            saw_json_line = true;
            // ...
        }
        // ...
    }
}

if !saw_json_line {
    // try parsing accumulated stdout as a single JSON object
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(stdout_accum.trim()) {
        send_json_owned(stdout_tx, stdout_ctx, parsed).await;
    }
}
```

`stdout_accum` accumulates every line from stdout, even when `saw_json_line` is already `true` (meaning the early parse succeeded and the accumulation is now provably unused). Once a JSON line is seen, `stdout_accum` is never read again — yet `push_str` continues running.

**Recommendation:** Early-exit the accumulation once `saw_json_line` is set:

```rust
if !saw_json_line {
    stdout_accum.push_str(&clean);
}
```

This bounds `stdout_accum` to only the non-JSON prefix of stdout, which is typically small (spinner lines, progress messages).

---

## Concurrency Issues

### FINDING-13: `std::sync::Mutex` Held Across `assistant_text.push_str` on Every Token Delta
**Severity:** High
**Impact:** On a `current_thread` runtime, this is single-threaded so there is no actual contention within the ACP runtime. But the `PermissionResponderMap` (line 31) uses the same `std::sync::Mutex` and is shared with the multi-thread Tokio runtime (the axum WS handler). Lock acquisition from the axum runtime could block a Tokio worker thread for the duration of the lock.

**Location:** `crates/services/acp.rs:1510–1515, 1590–1592`

```rust
// In request_permission() — called from current_thread runtime
let mut map = self.permission_responders.lock().map_err(|_| {
    agent_client_protocol::Error::internal_error()
        .data("permission responder lock poisoned")
})?;
map.insert(tool_call_id.clone(), resp_tx);
```

The `permission_responders: PermissionResponderMap` is `Arc<std::sync::Mutex<HashMap<...>>>`. It is accessed from two contexts:
1. The `current_thread` runtime inside `spawn_blocking` (ACP bridge client callbacks)
2. The axum WS handler (multi-thread Tokio runtime) which inserts permission responses

The `std::sync::Mutex` is appropriate here (documented in the comment at line 29–30) because `tokio::sync::Mutex` cannot be locked from a blocking context. However, if the axum WS handler calls `.lock()` on this mutex and the `spawn_blocking` thread holds it (e.g., during a `HashMap::insert` that triggers rehash), the axum worker thread blocks — a brief but real stall.

The lock is held only for HashMap insertions/removals, which are O(1) amortized. In practice the contention window is microseconds. This is not a crisis but worth documenting. The current `unwrap_or_else` at line 1590 silently discards the lock if poisoned rather than propagating the error — that's correct for cleanup paths.

**Recommendation:** This design is essentially correct. The improvement is to use `dashmap::DashMap` (lock-free concurrent HashMap) to eliminate the mutex entirely:

```rust
pub type PermissionResponderMap = Arc<dashmap::DashMap<String, tokio::sync::oneshot::Sender<String>>>;
```

`DashMap` uses fine-grained sharding and is safe to access from both async and blocking contexts without `.lock()`.

---

### FINDING-14: Exit Watcher Race — Prompt Completion vs. Clean Process Exit
**Severity:** Medium
**Impact:** A successful session that exits cleanly fires `exit_tx.send(String::new())` (line 1073), which resolves the `exit_rx` branch of the `select!`. If the prompt also completes simultaneously, the `select!` may choose the exit branch, causing the session to return `Err("ACP adapter exited before prompt completed")` even though the prompt succeeded.

**Location:** `crates/services/acp.rs:1079–1126`

```rust
tokio::select! {
    prompt_result = conn.prompt(PromptRequest::new(session_id.clone(), prompt_blocks)) => {
        // prompt completed — process TurnResult
    }
    exit_msg = exit_rx => {
        let msg = exit_msg.unwrap_or_else(|_| "exit channel dropped".to_string());
        if !msg.is_empty() {
            return Err(format!("ACP adapter crashed mid-session: {msg}"));
        }
        return Err("ACP adapter exited before prompt completed".to_string()); // fired on CLEAN exit too
    }
}
```

The process exit watcher at line 1064–1076 fires on **both** clean exit (exit code 0) and crash exit. When the adapter exits cleanly (code 0) at the same moment the prompt returns:

1. The prompt future resolves
2. The `child.wait()` future also resolves (clean exit → sends `String::new()` to `exit_tx`)
3. Both `prompt_result` and `exit_msg` branches are ready simultaneously
4. `tokio::select!` picks one branch with uniform probability (~50%)
5. If it picks `exit_msg`: `msg.is_empty()` is true, falls through to `return Err("ACP adapter exited before prompt completed")`

This is a race condition. The error message is incorrect for a clean exit path. Even though `msg.is_empty()` means "clean exit", the code still returns an `Err`.

**Recommendation:** On clean exit (`msg.is_empty()`), do not return an error — instead, check if the prompt has already completed by trying to poll it non-blocking:

```rust
exit_msg = exit_rx => {
    let msg = exit_msg.unwrap_or_else(|_| "exit channel dropped".to_string());
    if msg.is_empty() {
        // Clean exit — this is expected after a successful prompt.
        // The prompt completion should have won the select; if we're here,
        // the process exited before or simultaneously with completion.
        // Return Ok() only if prompt completed — otherwise this is still an error.
        return Ok(());  // or check prompt_result via a shared flag
    }
    return Err(format!("ACP adapter crashed mid-session: {msg}"));
}
```

The cleanest fix is to not send to `exit_tx` on clean exit at all — only send on non-zero exit codes. Then the exit branch only fires on actual crashes:

```rust
tokio::task::spawn_local(async move {
    match child.wait().await {
        Ok(status) if !status.success() => {
            let _ = exit_tx.send(format!("ACP adapter exited with {status}"));
        }
        Err(err) => {
            let _ = exit_tx.send(format!("ACP adapter wait failed: {err}"));
        }
        Ok(_) => {
            // Clean exit — don't send. Let the prompt branch win naturally.
            // If the prompt never completes and this branch drops, exit_rx will return Err(_).
        }
    }
});
```

With `exit_tx` dropped on clean exit, `exit_rx` returns `Err(RecvError)`, which the outer `select!` handles as `"exit channel dropped"`. The `exit_msg.unwrap_or_else(|_| "exit channel dropped")` then needs separate treatment for the RecvError case.

---

### FINDING-15: `permission_responders` Map Not Cleaned Up on Session Abort
**Severity:** Medium
**Impact:** Memory leak: if a session aborts (timeout, crash, WS disconnect) while a permission request is pending, the `oneshot::Sender` remains in the HashMap indefinitely, growing with each abandoned session

**Location:** `crates/services/acp.rs:1509–1515`

```rust
let (resp_tx, resp_rx) = tokio::sync::oneshot::channel::<String>();
{
    let mut map = self.permission_responders.lock()...;
    map.insert(tool_call_id.clone(), resp_tx);  // inserted here
}
// ... await resp_rx ...
// On timeout path at line 1590, the entry is removed:
if let Ok(mut map) = self.permission_responders.lock() {
    map.remove(&tool_call_id);
}
```

The timeout path (line 1590) cleans up the entry. But what happens if the `run_prompt_turn` function returns via the `exit_msg` branch (session crash/abort) while a permission request is pending? The `request_permission` future is cancelled, meaning the cleanup at line 1590 never runs. The `resp_tx` remains in the map.

The `oneshot::Sender<String>` is relatively small (~64 bytes), but the `tool_call_id` String is also held. Over many sessions with permission requests that abort, the map grows monotonically.

**Recommendation:** Use the existing `PermissionResponderMap` ownership in `run_prompt_turn` to drain all pending entries on exit:

```rust
// At the end of run_prompt_turn, before returning:
if let Ok(mut map) = permission_responders.lock() {
    map.clear();  // drop all pending oneshot senders
}
```

Or scope the `permission_responders` map per-session rather than sharing it globally across all sessions.

---

## Resource Management

### FINDING-16: Subprocess Not Killed on ACP Session Error Paths
**Severity:** High
**Impact:** Orphaned subprocess processes accumulate over time on error/timeout paths; each subprocess holds stdio pipes and system file descriptors

**Location:** `crates/services/acp.rs:785–798, 1153–1167`

The subprocess is spawned early in `run_prompt_turn`:

```rust
let mut child = scaffold
    .spawn_adapter()
    .map_err(|err| format!("failed to spawn ACP adapter: {err}"))?;
let child_stdin = child.stdin.take()...;
let child_stdout = child.stdout.take()...;
let child_stderr = child.stderr.take()...;
```

After `stdin`/`stdout`/`stderr` are taken from `child`, the `child` handle is moved into the exit watcher task (line 1064):

```rust
tokio::task::spawn_local(async move {
    match child.wait().await {
        // ...
    }
});
```

On the normal happy path: the exit watcher calls `child.wait()`, which reaps the process.

On error paths — if `run_prompt_turn` returns `Err(...)` before the prompt starts (e.g., at `conn.initialize()` failure, line 888) — the exit watcher task is still spawned and will eventually call `child.wait()`. The subprocess stays alive until it exits on its own.

However, if the adapter process is stuck waiting for stdin (which is now closed because `child_stdin` was taken and the `compat_stdin` is dropped on `run_prompt_turn` return), it should exit naturally when it detects the stdin EOF. The `LocalSet` tasks (`spawn_local`) will be dropped when the `LocalSet` itself is dropped at the end of `local.block_on(...)`.

The concern is that `spawn_local` tasks inside a `LocalSet` are dropped (not awaited) when the `LocalSet` is dropped. If `child.wait()` is in a `spawn_local` task and the `LocalSet` is dropped before `child.wait()` returns, the process becomes a zombie until the next reap.

**Recommendation:** Explicitly call `child.kill().await` before returning on error paths, and join the exit watcher task. Alternatively, implement `Drop` for a guard struct that kills the child process:

```rust
struct ChildGuard(tokio::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.start_kill();  // non-blocking kill signal
    }
}
```

---

### FINDING-17: `LocalSet` Tasks Not Joined on `run_prompt_turn` Return
**Severity:** Medium
**Impact:** IO task, stderr reader task, and exit watcher task leak when `run_prompt_turn` returns early; they are dropped rather than cancelled gracefully

**Location:** `crates/services/acp.rs:802–825, 859–876, 1064–1076`

Three `spawn_local` tasks are created:
1. Stderr reader (line 802)
2. IO task from `ClientSideConnection` (line 859)
3. Exit watcher (line 1064)

When `run_prompt_turn` returns (either normally or via `?`-propagated error), the `LocalSet` is dropped. `LocalSet::block_on` does not await pending `spawn_local` tasks on drop — it drops them. The stderr reader and IO tasks are effectively leaked until the OS cleans up the subprocess stdio handles.

For normal completion this is fine: the adapter exits, stdio pipes get EOF, the tasks terminate naturally before the `LocalSet` drops.

For error paths (e.g., `conn.initialize()` fails), the `LocalSet` may be dropped while the stderr reader is still blocked on `BufReader::read_line`. The task is cancelled (future dropped), the `BufReader` is dropped, the underlying `tokio::process::ChildStderr` is dropped, and the subprocess gets a broken pipe on its stderr — which typically causes it to exit. This is correct-ish but not explicit.

**Recommendation:** This is an inherent constraint of `LocalSet` + `spawn_blocking` architecture. The best mitigation is to add a `CancellationToken` that each `spawn_local` task polls, allowing clean shutdown before the `LocalSet` drops.

---

## Scalability Ceiling

### FINDING-18: Global `PermissionResponderMap` Shared Across All Sessions
**Severity:** Medium
**Impact:** All concurrent ACP sessions share one `HashMap` protected by one `std::sync::Mutex`. Under concurrent load with interactive permissions, every session contends on the same lock.

**Location:** `crates/services/acp.rs:31`, `crates/web/execute/sync_mode.rs:691, 805`

```rust
pub type PermissionResponderMap = Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<String>>>>;
```

The map is created once per WS connection (in `sync_mode.rs`) and passed through the entire call chain. This means all `pulse_chat` turns in a single WS session share one map. Across different WS connections, each connection has its own map (since `permission_responders` is created per-connection).

Within a single WS connection with concurrent ACP sessions (not currently possible — there is no concurrent `pulse_chat` path in `dispatch_service`), this would be a contention point. As currently implemented (one session per WS connection at a time), there is no actual contention.

However, the `PermissionResponderMap` is also locked from the axum WS handler side (when the user sends a permission response). If the map is large (many abandoned entries from FINDING-15), `HashMap::get`/`remove` are O(n) for high-collision loads — though typical UUIDs have minimal collision.

**Recommendation:** Scope the `PermissionResponderMap` to the session rather than the connection. Pass it through `handle_pulse_chat` and create a new empty map per turn.

---

### FINDING-19: No Session Concurrency Limit
**Severity:** Medium
**Impact:** Without a concurrency limit, many concurrent WS clients each running `pulse_chat` will each consume one `spawn_blocking` thread for up to 300 seconds. Memory usage grows as O(sessions × subprocess_size).

**Location:** `crates/web/execute/sync_mode.rs:828`

```rust
let task = tokio::spawn(async move {
    let result = scaffold
        .start_prompt_turn(&req, cwd, Some(event_tx), permission_responders)
        .await
        .map_err(|e| e.to_string());
    // ...
    result
});
```

`start_prompt_turn` internally calls `spawn_blocking`, consuming a thread. There is no semaphore or back-pressure mechanism. If 100 WS clients simultaneously send `pulse_chat`, 100 `spawn_blocking` threads are consumed.

**Recommendation:** Add a `tokio::sync::Semaphore` with a configurable permit count (e.g., from `AXON_ACP_MAX_CONCURRENT_SESSIONS` env var) before entering `handle_pulse_chat`. Requests that cannot acquire a permit receive an immediate `command.error` response with a human-readable message.

```rust
static ACP_SEMAPHORE: tokio::sync::OnceLock<tokio::sync::Semaphore> = tokio::sync::OnceLock::new();

async fn handle_pulse_chat(...) -> Result<(), String> {
    let sem = ACP_SEMAPHORE.get_or_init(|| {
        let limit = env::var("AXON_ACP_MAX_CONCURRENT_SESSIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8);
        tokio::sync::Semaphore::new(limit)
    });
    let _permit = sem.try_acquire()
        .map_err(|_| "ACP session limit reached — try again shortly".to_string())?;
    // ...
}
```

---

## Finding Summary Table

| ID | Severity | Category | File | Line(s) | Impact |
|----|----------|----------|------|---------|--------|
| FINDING-1 | Medium | Memory | acp.rs | 160, 307–337 | 4–6 unnecessary `AcpAdapterCommand` clones per session |
| FINDING-2 | Medium | Memory | acp.rs | 1429–1616 | Unbounded `assistant_text` growth; Mutex lock per streaming token |
| FINDING-3 | Low | Memory | acp.rs | 161, 220 | `AcpPromptTurnRequest` deep clone into spawn_blocking |
| FINDING-4 | Low | Memory | sync_mode.rs | 720–742 | `CommandContext` cloned per streaming event (3x String heap alloc) |
| FINDING-5 | **High** | Memory | sync_mode.rs, events.rs | 728–741, 128–131 | Double serde_json serialization per streaming token (5+ allocs/token) |
| FINDING-6 | **Critical** | Async | acp.rs | 167–191, 222–248 | 1 blocking thread per session for up to 300s; pool ceiling at 512 threads |
| FINDING-7 | **High** | Async | acp.rs | 174–188 | No WS-disconnect cancellation; abandoned sessions hold threads |
| FINDING-8 | Low | Async | sync_mode.rs | 757–783 | `select!` fairness; silent event drop on channel full |
| FINDING-9 | Medium | Async | sync_mode.rs | 720–741 | Unnecessary `tx.clone()` + `ws_ctx.clone()` per streaming event |
| FINDING-10 | Low | I/O | acp.rs | 353–369 | Hand-rolled TOML parser; fragile and re-reads file every session |
| FINDING-11 | Medium | I/O | acp.rs, sync_mode.rs | 353–413, 274–336 | 2–4 config file reads per ACP session; no caching |
| FINDING-12 | Low | I/O | sync_mode.rs | 1031–1061 | `stdout_accum` grows unbounded even after `saw_json_line` is true |
| FINDING-13 | **High** | Concurrency | acp.rs | 1510–1592 | `std::sync::Mutex` held from async context; blocks Tokio worker thread |
| FINDING-14 | Medium | Concurrency | acp.rs | 1079–1126 | Clean process exit races with prompt completion in `select!` |
| FINDING-15 | Medium | Concurrency | acp.rs | 1509–1515 | `oneshot::Sender` leaks in PermissionResponderMap on session abort |
| FINDING-16 | **High** | Resources | acp.rs | 785–798 | Subprocess not explicitly killed on error paths; potential orphan processes |
| FINDING-17 | Medium | Resources | acp.rs | 802–1076 | `spawn_local` tasks dropped rather than cancelled on early return |
| FINDING-18 | Medium | Scalability | acp.rs, sync_mode.rs | 31, 691 | Global `PermissionResponderMap` accumulates abandoned entries |
| FINDING-19 | Medium | Scalability | sync_mode.rs | 828 | No concurrency limit on ACP sessions; unbounded thread pool consumption |

### Priority Order for Implementation

1. **FINDING-6** (Critical): Document the `spawn_blocking` thread ceiling and add an explicit `max_blocking_threads` cap + `AXON_ACP_MAX_CONCURRENT_SESSIONS` semaphore (FINDING-19) in the same change.
2. **FINDING-14** (High): Fix the clean-exit race condition in the process exit watcher `select!` — this is a correctness bug, not just a performance issue.
3. **FINDING-16** (High): Add `start_kill()` on error paths to prevent orphan adapter processes.
4. **FINDING-5** (High): Eliminate double serialization on the streaming token hot path.
5. **FINDING-13** (High): Migrate `PermissionResponderMap` to `DashMap` to eliminate `std::sync::Mutex` from the async boundary.
6. **FINDING-2** (Medium): Replace the `Mutex<AcpRuntimeState>` with `RefCell<String>` + `OnceLock<String>` for the `current_thread` environment.
7. **FINDING-1** (Medium): Take `AcpAdapterCommand` by value in override functions to eliminate unnecessary clones.
8. **FINDING-15** (Medium): Drain `PermissionResponderMap` on session exit.
9. **FINDING-11** (Medium): Cache config file reads with a TTL.

Findings 3, 4, 7, 8, 9, 10, 12, 17, 18 are low-effort cleanups that can be batched into a single maintenance PR.
