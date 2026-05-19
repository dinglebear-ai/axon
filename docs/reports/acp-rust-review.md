# ACP Implementation — Idiomatic Rust Review
**Scope:** `crates/services/acp*`, `crates/services/types/acp.rs`, `crates/web/execute/sync_mode/pulse_chat/connection.rs`
**Date:** 2026-03-21
**Reviewer:** Claude Code (Phase 4 of prior review series)

---

## Table of Contents

1. [Summary](#summary)
2. [Critical Findings](#critical-findings)
3. [High Severity Findings](#high-severity-findings)
4. [Medium Severity Findings](#medium-severity-findings)
5. [Low Severity Findings](#low-severity-findings)
6. [Positive Patterns Worth Preserving](#positive-patterns-worth-preserving)

---

## Summary

The ACP implementation is well-structured and demonstrates strong awareness of the `!Send` constraint imposed by the ACP SDK. The `spawn_blocking` + `current_thread` runtime + `LocalSet` pattern is correctly applied, and the `RefCell`-over-`Mutex` choice for hot streaming paths is appropriately justified and documented. Significant prior work (AdapterGuard RAII, DashMap for permissions, `expect` over `allow`, SIGKILL-safe exit handling) has already addressed the most dangerous bugs.

The remaining findings fall into three categories: one latent panic from `LocalSet::run_until` called outside `spawn_blocking`, several structural issues with stringly-typed error classification and split mutex state, and a collection of style/ergonomics improvements against modern Rust idioms (Rust 2024 edition is already declared in `Cargo.toml`).

**Counts by severity:** Critical: 1 | High: 3 | Medium: 6 | Low: 5

---

## Critical Findings

### C-1 — `LocalSet::run_until` Called Directly on Multi-Thread Runtime (Correctness/Panic Risk)

**File:** `crates/services/acp_llm.rs`, lines 184–188
**Type:** Correctness — latent panic

**Current pattern:**
```rust
// run_completion_inner — called from AcpCompletionRunner::complete_text/complete_streaming
// which are async fn, executed on the multi-thread Tokio runtime
let local = tokio::task::LocalSet::new();
match tokio::time::timeout(
    timeout,
    local.run_until(run_completion_local(scaffold, req, on_delta)),
)
.await
```

**Problem:**
`LocalSet::run_until` is documented to panic if called from within a multi-thread Tokio runtime context. The surrounding code in `run_acp_event_loop` (in `acp.rs`) and `adapter_loop` / `adapter_loop_eager` (in `persistent_conn.rs`) correctly guard against this by calling `local.block_on(&rt, ...)` from inside a `spawn_blocking` closure — the `spawn_blocking` thread is not a Tokio async context, so `block_on` is safe there.

`run_completion_inner` in `acp_llm.rs` does **not** follow this pattern. It calls `local.run_until(...).await` directly within an `async fn`, which runs on the multi-thread runtime. The Tokio docs state:

> This method panics if the `LocalSet` is driven on a `current_thread` runtime on a different thread, or from a `Runtime::block_on` call on the multi-thread runtime.

In practice this may be masked by the fact that `run_completion_local` itself spawns `!Send` tasks via `spawn_local`, but the panic is triggered at the `run_until` call site when the executor detects it is being polled on a thread that is not the `current_thread` runtime thread.

This matches the prior finding CQ-8/PERF-6 referenced in the review brief. The fix applied in `acp.rs` (wrapping in `spawn_blocking`) has **not** been applied to `acp_llm.rs`.

**Recommended pattern:**
```rust
// Wrap in spawn_blocking to establish a current_thread context — same
// pattern as run_acp_event_loop in acp.rs:
let result = tokio::task::spawn_blocking(move || {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("acp_llm: failed to build runtime: {e}"))?;
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async {
        match tokio::time::timeout(timeout, run_completion_local(scaffold, req, on_delta)).await {
            Ok(r) => r,
            Err(_) => Err(format!("ACP completion timed out after {ACP_COMPLETION_TIMEOUT_SECS}s").into()),
        }
    })
})
.await
.map_err(|e| format!("acp_llm: failed to join runtime worker: {e}"))?;
```

---

## High Severity Findings

### H-1 — Fatal Error Classification via String Matching (Correctness/Maintainability)

**File:** `crates/web/execute/sync_mode/pulse_chat/connection.rs`, lines 157–160
**Type:** Correctness — brittle error classification

**Current pattern:**
```rust
let is_fatal = err.contains("channel closed")
    || err.contains("channel dropped")
    || err.contains("adapter exited")
    || err.contains("result unavailable after channel close");
```

**Problem:**
This matches prior finding CQ-10. Error classification via substring matching on `String` messages is fragile: any refactor that changes an error message string silently breaks the classification logic. This includes message changes from upstream crate updates, i18n changes, or future logging normalization. If `is_fatal` incorrectly evaluates to `false`, a broken adapter session is kept in the cache and future turns are dispatched to a dead channel, producing confusing errors for the user. If it incorrectly evaluates to `true`, a healthy session is evicted unnecessarily, paying the cold-start cost.

**Recommended pattern:**
Replace `Result<(), String>` at internal async-task boundaries with a typed error enum:

```rust
#[derive(Debug)]
pub enum AcpTurnError {
    ChannelClosed,          // adapter exited, rx dropped
    AdapterExited(String),  // non-zero exit or wait failure
    TurnError(String),      // per-turn recoverable error
}

impl AcpTurnError {
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::ChannelClosed | Self::AdapterExited(_))
    }
}
```

The `run_turn_on_conn` function and the `adapter_loop` exit path would produce `AcpTurnError` variants; `classify_and_evict_on_fatal` would call `.is_fatal()`. No string pattern matching required.

This is a style/correctness trade-off: the current `String`-based approach is pragmatic given the `!Send` / cross-thread-boundary constraint (typed errors do not cross `spawn_blocking` without `Send` bounds), but an intermediate typed enum for in-process dispatch is entirely feasible.

---

### H-2 — Five Separate `std::sync::Mutex` Fields in `CachedSession` (Structural Correctness)

**File:** `crates/services/acp/session_cache.rs`, lines 43–53
**Type:** Structural — fragmented lock state, deadlock potential

**Current pattern:**
```rust
pub struct CachedSession {
    pub handle: Arc<AcpConnectionHandle>,
    pub permission_responders: PermissionResponderMap,
    last_active: std::sync::Mutex<Instant>,
    replay_buffer: std::sync::Mutex<Vec<String>>,
    replay_buffer_bytes: std::sync::Mutex<usize>,
    turn_in_flight_since: std::sync::Mutex<Option<Instant>>,
    last_turn_completed_at: std::sync::Mutex<Option<Instant>>,
}
```

**Problem:**
This matches prior finding CQ-2. The `buffer_event` method acquires `replay_buffer_bytes` first, then `replay_buffer`:
```rust
let mut bytes = self.replay_buffer_bytes.lock()...;
let mut buf = self.replay_buffer.lock()...;
```

`drain_replay_buffer` acquires them in the same order. This is consistent and deadlock-free **today**, but the two locks are semantically a single unit of state: the byte count is an invariant of the buffer. Separating them into two fields requires callers to manually maintain the invariant (and remember the acquisition order) forever.

Additionally, five independent mutexes means five separate lock acquisitions for any operation that touches multiple fields (e.g. `mark_turn_completed` acquires two mutexes sequentially). The cognitive overhead of auditing lock ordering across methods is higher than necessary.

**Recommended pattern:**
Consolidate logically related mutable state into single mutex-protected structs:

```rust
struct ReplayState {
    buffer: Vec<String>,
    byte_count: usize,
}

struct TurnState {
    in_flight_since: Option<Instant>,
    last_completed_at: Option<Instant>,
}

pub struct CachedSession {
    pub handle: Arc<AcpConnectionHandle>,
    pub permission_responders: PermissionResponderMap,
    last_active: std::sync::Mutex<Instant>,
    replay: std::sync::Mutex<ReplayState>,
    turn: std::sync::Mutex<TurnState>,
}
```

This reduces five mutexes to three, eliminates the ordering hazard between `replay_buffer` and `replay_buffer_bytes`, and makes the invariant (byte count == sum of buffer lengths) enforced by the type system rather than by convention.

---

### H-3 — `read_replay_buffer` is an Alias for `drain_replay_buffer` With a Misleading Name

**File:** `crates/services/acp/session_cache.rs`, lines 121–125
**Type:** API design — semantics mismatch on a public method

**Current pattern:**
```rust
/// Read and drain all buffered events for replay to a reconnecting client.
/// Delegates to `drain_replay_buffer()` -- both operations have identical
/// drain-and-clear semantics.
pub fn read_replay_buffer(&self) -> Vec<String> {
    self.drain_replay_buffer()
}
```

**Problem:**
The method is named `read_replay_buffer` but unconditionally drains. "Read" in Rust convention implies a non-consuming, non-mutating access (compare `Read` trait, `read_to_string`, `peek`). A caller reading this API for the first time would reasonably expect `read_replay_buffer` to return a borrow or a clone-for-display, not to clear the buffer as a side effect. This is a public API on a module-visible struct (`pub`), so the semantic confusion can propagate to future call sites.

The comment acknowledges both methods have "identical drain-and-clear semantics," which means one of the two names is wrong. `drain_replay_buffer` is the accurate name.

**Recommended action:**
Remove `read_replay_buffer` and rename all call sites to `drain_replay_buffer`. If the intent was to preserve API surface for callers that only wanted to "read without clearing," that variant needs a different name (e.g. `peek_replay_buffer`) and a different implementation (returning `&[String]` or a clone).

---

## Medium Severity Findings

### M-1 — `#[allow]` Used in Test Modules Where `#[expect]` Should Be Used

**Files:**
- `crates/services/acp/bridge.rs`, line 334: `#[allow(clippy::arc_with_non_send_sync)]`
- `crates/services/acp/permission.rs`, line 236: `#[allow(clippy::arc_with_non_send_sync)]`

**Type:** Style/Maintainability — weaker lint suppression

**Current pattern:**
```rust
#[allow(clippy::arc_with_non_send_sync)]
mod tests { ... }
```

**Problem:**
`#[allow]` silently continues to suppress a lint even after the triggering code is removed. `#[expect]` (stabilized in Rust 1.81, available here since the project targets Rust 1.94) emits a compiler warning if the suppressed lint no longer fires, preventing stale suppressions from accumulating. The production code in `session.rs` already uses `#[expect(clippy::arc_with_non_send_sync)]` on line 158 — the test modules in `bridge.rs` and `permission.rs` should be consistent.

**Recommended pattern:**
```rust
#[expect(clippy::arc_with_non_send_sync)]
mod tests { ... }
```

---

### M-2 — `AdapterGuard` Tuple Field Accessed via `.0` Directly (Encapsulation)

**File:** `crates/services/acp/runtime.rs`, line 31; `crates/services/acp/session.rs`, line 49
**Type:** Structural — leaking internal representation through `pub(super)` field

**Current pattern:**
```rust
// In runtime.rs:
pub(super) struct AdapterGuard(pub(super) Option<tokio::process::Child>);

// In session.rs:
let inner = guard.0.as_mut().ok_or("adapter guard empty")?;
```

**Problem:**
Exposing the inner `Option<Child>` as a `pub(super)` field forces callers to know the internal representation of the guard. `AdapterGuard` already exposes `take()`, which is the correct API for disarming the guard. The `stdin`/`stdout`/`stderr` accesses in `session.rs` could use a `child_mut()` accessor instead of reaching into `.0` directly.

**Recommended pattern:**
Add a private or `pub(super)` accessor method and make the tuple field private:
```rust
pub(super) struct AdapterGuard(Option<tokio::process::Child>);

impl AdapterGuard {
    pub(super) fn child_mut(&mut self) -> Option<&mut tokio::process::Child> {
        self.0.as_mut()
    }
    // take() already exists
}
```

This allows the internal representation to change (e.g. adding a second field) without touching `session.rs`.

---

### M-3 — Inconsistent Single-Character Error Variable Names in `map_err` Closures

**File:** `crates/services/acp/session.rs`, lines 292, 325
**Type:** Style — naming inconsistency within the module

**Current pattern:**
```rust
// session.rs:292 — uses |e|
.map_err(|e| e.to_string())?;

// session.rs:46 — uses |err|  (same file, same operation)
.map_err(|err| format!("failed to spawn ACP adapter: {err}"))?;
```

The rest of the ACP module consistently uses `|err|` in `map_err` closures, making these two instances stand out. `|e|` is not incorrect but breaks the local pattern and can be confused with iterator variable `e` in closures.

**Recommended pattern:** Use `|err|` consistently throughout the ACP module:
```rust
.map_err(|err| err.to_string())?;
```

---

### M-4 — Redundant Intermediate Variable in `apply_config_and_model`

**File:** `crates/services/acp/session.rs`, lines 357–362
**Type:** Style — unnecessary allocation and `clone()`

**Current pattern:**
```rust
let mapped = initial_config_options
    .as_ref()
    .map(|o| map_config_options(o));  // Option<Vec<AcpConfigOption>>
let sid = session_id.0.to_string();
if let Some(ref opts) = mapped
    && !opts.is_empty()
{
    latest_config_options = opts.clone();  // clone #1
    emit(
        tx,
        ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::ConfigOptionsUpdate {
                session_id: sid.clone(),   // clone #2
                config_options: opts.clone(), // clone #3
```

**Problem:**
`opts.clone()` is called twice within the same `if let` branch — once to assign to `latest_config_options` and once to build the event. The idiomatic approach uses `map_config_options` inline and assigns before cloning for the event, or uses `.into_iter()` to avoid the first clone when moving is possible.

**Recommended pattern:**
```rust
if let Some(opts) = initial_config_options.as_ref().map(|o| map_config_options(o))
    && !opts.is_empty()
{
    emit(
        tx,
        ServiceEvent::AcpBridge {
            event: AcpBridgeEvent::ConfigOptionsUpdate {
                session_id: sid.clone(),
                config_options: opts.clone(),
            },
        },
    )
    .await;
    latest_config_options = opts;  // move, not clone
}
```

Or use `Option::as_deref_mut` / restructure to move `opts` into `latest_config_options` first and then borrow for the event.

---

### M-5 — `to_string_lossy().to_string()` Double Allocation for Path Conversion

**File:** `crates/services/acp/mapping.rs`, lines 285, 295
**Type:** Performance — unnecessary allocation

**Current pattern:**
```rust
.map(|l| l.path.to_string_lossy().to_string())
```

**Problem:**
`to_string_lossy()` returns a `Cow<str>`. If the path is valid UTF-8 (which is almost always true on Linux), `Cow::Borrowed` is returned. Calling `.to_string()` on `Cow::Borrowed` allocates a new `String` by cloning the borrowed slice. The idiomatic conversion is `l.path.display().to_string()` which correctly handles non-UTF-8 paths with `?` substitution and allocates exactly once.

**Recommended pattern:**
```rust
.map(|l| l.path.display().to_string())
```

If lossless path representation is required, `l.path.to_string_lossy().into_owned()` avoids the double allocation for the borrowed case by only allocating when the path contains non-UTF-8 sequences.

---

### M-6 — `spawn_adapter` and `spawn_adapter_skip_validation` Share Duplicated Command-Building Logic

**File:** `crates/services/acp.rs`, lines 203–275
**Type:** Maintainability — DRY violation

**Current pattern:**
The two methods share an identical block:
```rust
let mut command = tokio::process::Command::new(&self.adapter.program);
command.args(&self.adapter.args);
if let Some(cwd) = &self.adapter.cwd {
    command.current_dir(cwd);
}
apply_env_allowlist(&mut command);
command.stdin(std::process::Stdio::piped());
command.stdout(std::process::Stdio::piped());
command.stderr(std::process::Stdio::piped());
command.kill_on_drop(true);
```

`spawn_adapter` adds preflight logic before this block; `spawn_adapter_skip_validation` does not. The comment in `ACP_ENV_ALLOWLIST` already acknowledges the sync risk: "Kept as a module-level constant so `spawn_adapter` and `spawn_adapter_skip_validation` stay in sync."

**Problem:**
Any change to the command-building logic (adding a new pipe, changing kill-on-drop behavior, adding a new env var setup step) must be replicated in both methods. The risk of divergence grows over time.

**Recommended pattern:**
Extract the shared command-building logic into a private helper:
```rust
fn build_adapter_command(&self) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(&self.adapter.program);
    command.args(&self.adapter.args);
    if let Some(cwd) = &self.adapter.cwd {
        command.current_dir(cwd);
    }
    apply_env_allowlist(&mut command);
    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    command.kill_on_drop(true);
    command
}
```

Both `spawn_adapter` and `spawn_adapter_skip_validation` call `build_adapter_command()` and then call `.spawn()`. This is a purely internal refactor with no behavioral change.

---

## Low Severity Findings

### L-1 — `collect::<Vec<String>>()` Turbofish Redundant Given Return Type Context

**File:** `crates/services/acp/mapping.rs`, lines 286, 296
**Type:** Style — unnecessary turbofish annotation

**Current pattern:**
```rust
.map(|l| l.path.to_string_lossy().to_string())
.collect::<Vec<String>>()
```

The return type is already constrained by the `Option<Vec<String>>` outer type annotation, making the turbofish annotation redundant. Clippy's `clippy::needless_collect_on_cloned` / `clippy::iter_collect_into` may or may not fire depending on the clippy version, but the annotation is unnecessary visual noise.

**Recommended pattern:**
```rust
.map(|l| l.path.display().to_string())
.collect()  // type inferred from context
```

---

### L-2 — `if log_level == LogLevel::Info` Could Use `matches!`

**File:** `crates/services/acp/bridge.rs`, lines 96–100
**Type:** Style — idiomatic pattern preference

**Current pattern:**
```rust
if log_level == LogLevel::Info {
    crate::crates::core::logging::log_info(&msg);
} else {
    crate::crates::core::logging::log_warn(&msg);
}
```

**Recommended pattern:**
```rust
if matches!(log_level, LogLevel::Info) {
    ...
}
```

Or more idiomatic: branch on the enum directly:
```rust
match log_level {
    LogLevel::Info => crate::crates::core::logging::log_info(&msg),
    _ => crate::crates::core::logging::log_warn(&msg),
}
```

The `==` comparison requires `PartialEq` which `LogLevel` does derive, so this is correct; it is purely a style observation. The `match` form is more idiomatic for enum dispatch and scales better if `LogLevel` gains new variants.

---

### L-3 — `map(|o| map_config_options(o))` Can Be Written as `map(map_config_options)`

**File:** `crates/services/acp/session.rs`, line 358–359
**Type:** Style — unnecessary closure wrapping a function

**Current pattern:**
```rust
let mapped = initial_config_options
    .as_ref()
    .map(|o| map_config_options(o));
```

When a closure simply passes its argument to a function with the same signature, the closure is unnecessary. This triggers `clippy::redundant_closure` in most configurations.

**Recommended pattern:**
```rust
let mapped = initial_config_options
    .as_ref()
    .map(|opts| map_config_options(opts));
// Or, since map_config_options takes &[SdkConfigOption] and as_ref() yields &Vec<...>:
// .as_deref()
// .map(map_config_options)
```

Note: whether direct function reference works depends on the exact type coercion (`&Vec<T>` → `&[T]`). Using `as_deref()` first makes the coercion explicit.

---

### L-4 — `adapter_loop` and `adapter_loop_eager` Duplicate the Turn Loop Body

**File:** `crates/services/acp/persistent_conn.rs`, lines 246–267 and 326–347
**Type:** Maintainability — DRY violation in async loop bodies

**Current pattern:**
Both `adapter_loop` and `adapter_loop_eager` end with an identical `loop { tokio::select! { ... } }` block:
```rust
loop {
    tokio::select! {
        msg = rx.recv() => {
            match msg {
                Some(AdapterMessage::RunTurn(turn)) => {
                    turn::run_turn_on_conn(...).await;
                }
                None => {
                    tracing::info!(..., "channel closed");
                    break;
                }
            }
        }
        exit_result = &mut exit_rx => {
            match exit_result {
                Ok(msg) => tracing::error!(...),
                Err(_) => tracing::info!(...),
            }
            break;
        }
    }
}
tracing::info!(..., "adapter loop ended");
```

The only difference is the log messages contain "(eager)" in one variant. The shared `EstablishedSession` fields and the `run_turn_on_conn` call are identical.

**Recommended pattern:**
Extract a shared `run_turn_loop` async function taking `conn`, `session_id`, `session_cwd`, `runtime_state`, `exit_rx`, `rx`, and a label string for log context:

```rust
async fn run_turn_loop(
    conn: &mut ClientSideConnection,
    session_id: &SessionId,
    session_cwd: &std::path::Path,
    runtime_state: &Arc<AcpRuntimeState>,
    mut exit_rx: tokio::sync::oneshot::Receiver<String>,
    mut rx: mpsc::Receiver<AdapterMessage>,
    context_label: &str,
) {
    loop {
        tokio::select! {
            // ... unified body
        }
    }
    tracing::info!(context = "acp_conn", context_label, "adapter loop ended");
}
```

Both `adapter_loop` and `adapter_loop_eager` would delegate their tail to `run_turn_loop`, eliminating ~25 lines of duplication.

---

### L-5 — Stringly-Typed `stop_reason` in `AcpTurnResultEvent`

**File:** `crates/services/types/acp.rs`, line 246–249; `crates/services/acp/bridge.rs`, line 61–70
**Type:** Type safety — stringly-typed field where an enum exists

**Current pattern:**
```rust
pub struct AcpTurnResultEvent {
    pub session_id: String,
    pub stop_reason: String,  // "end_turn", "max_tokens", "refusal", etc.
    pub result: String,
}
```

The `stop_reason` field is constructed from `stop_reason_to_str()` in `bridge.rs`:
```rust
pub(super) fn stop_reason_to_str(reason: StopReason) -> &'static str {
    match reason {
        StopReason::EndTurn => "end_turn",
        // ...
        _ => "unknown",
    }
}
```

**Problem:**
The `AcpSessionUpdateKind` enum already models session update types idiomatically. `stop_reason` could be a typed enum that serializes to the same string form via `#[serde(rename_all = "snake_case")]`. This would eliminate the stringly-typed field for in-process consumers (e.g. `handle_completion_bridge_event` in `acp_llm.rs`) without changing the wire format, and would make it impossible to create an `AcpTurnResultEvent` with an invalid stop reason string.

**Recommended pattern:**
```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpStopReason {
    EndTurn,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Cancelled,
    #[serde(other)]
    Unknown,
}

pub struct AcpTurnResultEvent {
    pub session_id: String,
    pub stop_reason: AcpStopReason,
    pub result: String,
}
```

The wire representation remains identical. The `stop_reason_to_str` helper in `bridge.rs` is no longer needed.

This is Low rather than Medium because the current implementation is correct and complete; the typed enum is purely an ergonomic improvement for in-process consumers.

---

## Positive Patterns Worth Preserving

The following patterns are explicitly called out as good practice for future contributors:

**1. `RefCell` over `Mutex` on the hot streaming path** (`bridge.rs`, `AcpRuntimeState`)
Using `RefCell` for single-thread hot paths inside `LocalSet` is correct and well-documented. The `#[expect(clippy::arc_with_non_send_sync)]` in `session.rs` is the right suppression for this intentional design.

**2. `AdapterGuard` RAII for subprocess cleanup** (`runtime.rs`)
The drop-based kill on all error paths is the correct pattern. The `kill_on_drop(true)` backstop in `spawn_adapter` reinforces this for unexpected future code paths.

**3. `DashMap` for `PermissionResponderMap`** (`acp.rs`)
Using shard-locking over a single `Arc<Mutex<HashMap>>` for concurrent permission inserts/removes is appropriate. The `(session_id, tool_call_id)` composite key preventing cross-session collisions is a correct security measure (SEC-7).

**4. RAII guard for `PermissionResponderMap` cleanup** (`permission.rs`, `PermissionGuard`)
Ensuring the DashMap entry is removed even when the future is cancelled (e.g. on `select!` drop) is critical correctness. The inline `struct PermissionGuard<'a>` + `impl Drop` pattern is idiomatic for this.

**5. `try_recv` drain after turn result in `complete_streaming`** (`acp_llm.rs`, lines 491–498)
Draining any late events after the result channel fires ensures `TurnResult` events queued just before channel close are not lost.

**6. Turn ID tracking to reject stale deltas** (`bridge.rs`, `current_turn_id`)
Using a monotonically increasing turn ID to detect and drop late streaming deltas from previous turns is a correct solution to the race condition described in the SIGKILL-FIX comment.

**7. `validate_model_string` character allowlist** (`adapters.rs`)
Validating the model string against an explicit allowlist before embedding it in a subprocess argument — even though arguments pass via `execvp` — is correct defense in depth. The comment noting the assumption ("args go via execvp, no shell expansion") should be preserved.

**8. Separate `spawn_blocking` + `current_thread` runtimes for `!Send` ACP futures** (`acp.rs`, `persistent_conn.rs`)
This is the correct architectural pattern for adapting a `?Send` async library to a `Send`-requiring multi-thread Tokio runtime. The `run_acp_event_loop` helper encapsulating this once is good DRY design.

---

*End of review. Findings are ordered by severity within each tier, not by file.*
