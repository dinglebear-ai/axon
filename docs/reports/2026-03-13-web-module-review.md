# crates/web — Comprehensive Code Review
**Date:** 2026-03-13
**Branch:** feat/web-integration-review-fixes
**Reviewers:** Two parallel rust-reviewer agents (async path + sync/auth path)
**Skills applied:** rust-async-patterns, rust-best-practices, acp

---

## Summary

| Severity | Count | CI Blocking |
|----------|-------|-------------|
| **P0 Critical** | 6 | Yes — CI/pre-commit will fail |
| **P1 High** | 9 | Should not merge |
| **P2 Medium** | 9 | Fix soon |
| **P3 Low** | 7 | Nice to have |
| **Security** | 6 | SEC-1 is P0-equivalent |

---

## P0 — Critical (must fix before merge)

---

### P0-1 · `MutexGuard` held across `.await` in `read_file` — deadlock risk

**File:** `crates/web/ws_handler.rs` ~line 249–261

`read_file` branch: `base.lock().await` is held while `execute::handle_read_file(&path, base_dir, tx).await` runs — which performs multiple `tokio::fs::canonicalize` and `tokio::fs::read_to_string` calls (unbounded I/O latency). Any concurrent `crawl_files` message arriving at the forward task will contend on the same lock, stalling message forwarding for the entire WS connection.

**Fix:** Clone the `PathBuf` out before the await, then release:
```rust
let base_dir_opt = base.lock().await.clone();
if let Some(base_dir) = base_dir_opt {
    execute::handle_read_file(&path, &base_dir, tx).await;
}
```

---

### P0-2 · `dispatch_search_and_info_modes` exceeds 120-line hard limit

**File:** `crates/web/execute/sync_mode/dispatch.rs` ~line 110–236 (127 lines)

7 lines over the project hard-fail threshold. No `.monolith-allowlist` entry. CI gate will fail.

**Fix:** Extract `screenshot` / `evaluate` / `dedupe` / `debug` arms (lines ~190–236) into a private `dispatch_diagnostic_modes` helper. Reduces each to under 80 lines.

---

### P0-3 · `ws_handler.rs` file length exceeds 500-line hard limit

**File:** `crates/web/ws_handler.rs` (510 lines, limit 500)

No allowlist entry. Pre-commit and CI will fail.

**Fix:** Move the test module (lines 412–510, ~99 lines) to `crates/web/ws_handler/tests.rs`. Brings file to ~411 lines. Alternatively, extract `route_permission_response` + `handle_acp_resume` (lines 272–410) into `crates/web/ws_handler/message_routing.rs`.

---

### P0-4 · `.expect()` on `HeaderValue` construction in `serve_output_file`

**File:** `crates/web.rs` lines 213, 216

```rust
resp_headers.insert(header::CONTENT_TYPE, content_type.parse().unwrap());
resp_headers.insert(header::CACHE_CONTROL, "public, max-age=300".parse().unwrap());
```

Not inside `#[cfg(test)]`. Violates project unwrap policy.

**Fix:** Use `HeaderValue::from_static` — compile-verified for static strings, no runtime failure:
```rust
resp_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
resp_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=300"));
```

---

### P0-5 · `.expect()` on evaluate runtime builder in production path

**File:** `crates/web/execute/sync_mode/service_calls.rs` ~line 267

```rust
let rt = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .expect("evaluate runtime");
```

Called on any `evaluate` WS request. Under OS resource exhaustion, this panics the entire Axum server process and takes down all active WS connections.

**Fix:** Propagate the error through the oneshot channel:
```rust
let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
    Ok(rt) => rt,
    Err(e) => {
        let _ = tx.send(Err(format!("evaluate runtime build failed: {e}").into()));
        return;
    }
};
```

---

### P0-6 · SEC-1: Shell endpoint loopback auth bypass

**File:** `crates/web.rs` lines 276–301

The `shell_ws_upgrade` handler skips `http_auth` entirely when `addr.ip().is_loopback()`. This means any process on the same host (not just the browser) can open a full PTY shell **regardless of `AXON_WEB_API_TOKEN` being set**. This is a critical auth bypass.

**Fix:** Remove the `is_loopback` short-circuit entirely. Run `http_auth` unconditionally. If local-dev convenience is needed, gate it behind `AXON_SHELL_ALLOW_LOOPBACK_UNAUTHENTICATED=true` which is off by default.

---

## P1 — High Priority (should not merge)

---

### P1-1 · Spawned execute/cancel tasks not cancelled on WS disconnect — task leak

**File:** `crates/web/ws_handler.rs` lines 201–213, 224–234

`forward.abort()` is called on disconnect but the spawned execute and cancel tasks are fully detached (JoinHandles discarded). Long-running operations (e.g. `evaluate`, which spawns an OS thread) continue running indefinitely after client disconnects, holding `SYNC_MODE_SEMAPHORE` permits and consuming CPU.

**Fix:** Use a `JoinSet` per connection:
```rust
let mut tasks = tokio::task::JoinSet::new();
// in execute arm:
tasks.spawn(async move { handle_command(...).await; });
// on disconnect:
tasks.shutdown().await;
```

---

### P1-2 · Rate limit resets on reconnect — trivially bypassed

**File:** `crates/web/ws_handler.rs` lines 140–141, 177–192

`execute_count` and `rate_window_start` are local to `handle_ws`, reinitialised on every WS connection. A client can exhaust the 120-execute limit, close and reopen the WebSocket, and resume with a fresh counter — indefinitely.

**Fix:** Move the rate-limit state to a process-wide structure keyed by authenticated identity or IP, stored in `AppState`.

---

### P1-3 · `session_ownership` map never cleaned up on WS disconnect — memory leak

**File:** `crates/web/ws_handler.rs` and `crates/web.rs`

`session_ownership` (`Arc<DashMap<String, String>>`) accumulates entries on every `acp_resume` but never removes them when the owning WS connection closes. Over long uptime with many reconnects, this grows without bound.

**Fix:** On `handle_ws` exit, before `forward.abort()`:
```rust
state.session_ownership.retain(|_, owner| owner != &conn_id);
```

---

### P1-4 · `shutdown_signal` uses `.expect()` in production code

**File:** `crates/web.rs` line 146

```rust
tokio::signal::ctrl_c().await.expect("failed to listen for ctrl+c");
```

Signal registration can fail in sandboxed environments. Panics rather than gracefully degrading.

**Fix:**
```rust
async fn shutdown_signal() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        log_warn(&format!("failed to register ctrl+c handler: {e}; no graceful shutdown"));
        std::future::pending::<()>().await;
    }
}
```

---

### P1-5 · SEC-2: `enable_fs`/`enable_terminal` ACP capability flags silently ignored

**File:** `crates/web/execute/sync_mode/acp_adapter.rs` lines ~44–45
**Cross-ref:** `crates/web/execute/sync_mode/dispatch.rs` lines ~259–260

`DirectParams` correctly captures `enable_fs` and `enable_terminal` from WS flags but they are never forwarded to `AcpAdapterCommand`. The adapter **always** receives `enable_fs: true, enable_terminal: true`, regardless of what the client requested.

**Fix:** Thread `enable_fs` and `enable_terminal` from `DirectParams` → `handle_pulse_chat` → `get_or_create_acp_connection` → `AcpAdapterCommand`. The fields already exist on the struct.

---

### P1-6 · SEC-3: `DefaultHasher` for ACP session cache keying — collision risk

**File:** `crates/web/execute/sync_mode/pulse_chat.rs` lines 2–3, 228–234

`fingerprint_mcp_servers` uses `std::hash::DefaultHasher` (non-deterministic across process restarts, collision-prone for security-relevant keys). A collision maps the wrong ACP adapter subprocess to a different MCP configuration.

**Fix:** Use the serialized JSON directly as the key component — it's already computed, zero collision risk:
```rust
fn fingerprint_mcp_servers(mcp_servers: &[AcpMcpServerConfig]) -> String {
    serde_json::to_string(mcp_servers).unwrap_or_default()
}
// agent_key becomes: format!("Claude:mcp={}", fingerprint_mcp_servers(&mcp_servers))
```

---

### P1-7 · `#![allow(dead_code)]` suppresses entire `session_guard` module — unused production code

**File:** `crates/web/execute/session_guard.rs` lines 1–103

Module-level `#![allow(dead_code)]` means `poll_session_file` has no production callers anywhere in `crates/web/`. Dead code in production binary; suppresses future compiler warnings.

**Fix:** Either wire `poll_session_file` to its call site in `pulse_chat.rs`, or delete the module. Remove `#![allow(dead_code)]` regardless.

---

### P1-8 · `crawl_files` detection via string scan on every forwarded message — fragile and costly

**File:** `crates/web/ws_handler.rs` lines 96–107

Every forwarded message is scanned with `.contains("\"crawl_files\"")` and conditionally re-parsed as JSON. False positives on any message containing this substring; O(n) scan on high-frequency crawl output.

**Fix:** Emit `crawl_files` events via a separate typed channel rather than re-parsing forwarded JSON strings.

---

### P1-9 · Rate limit errors not in `WsEventV2` envelope — invisible to structured frontend handler

**File:** `crates/web/ws_handler.rs` lines 183–192

When rate limit is exceeded, a plain `{"type": "error", ...}` JSON is sent — not a `WsEventV2::CommandError` envelope. Clients expecting V2 events cannot associate this error with the originating command.

**Fix:** Construct a proper `WsEventV2::CommandError` with a synthetic `CommandContext` using the partially-parsed `client_msg` data.

---

## P2 — Medium Priority (fix soon)

---

### P2-1 · `docker_stats` task fully detached — silently stops on bollard failure

**File:** `crates/web.rs` line 96

`tokio::spawn(docker_stats::run_stats_loop(stats_tx))` — JoinHandle discarded. If `run_stats_loop` panics or returns, the task disappears with no log and no restart. All WS clients stop receiving stats silently.

**Fix:** Wrap in a restart loop:
```rust
tokio::spawn(async move {
    loop {
        if let Err(e) = docker_stats::run_stats_loop(stats_tx.clone()).await {
            log_warn(&format!("docker stats loop failed: {e}; restarting in 5s"));
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
});
```

---

### P2-2 · `handle_command` has 8 parameters — suppressed `too_many_arguments` clippy lint

**File:** `crates/web/execute.rs` lines 165–175

`#[allow(clippy::too_many_arguments)]` suppresses a valid signal. `ExecCommandContext` is constructed from these 8 values at line 179 anyway — pass it directly.

**Fix:** Accept `ExecCommandContext` as the parameter instead of constructing it inside `handle_command`.

---

### P2-3 · Blocking `path.exists()` calls in async context inside `resolve_exe`

**File:** `crates/web/execute/exe.rs` lines 13, 41

`resolve_exe()` is a synchronous fn called from `dispatch_subprocess_fallback` (async). It calls `path.exists()` and `candidate.exists()` — blocking syscalls. Iterates 6+ path candidates per call.

**Fix:** Either use `tokio::fs::try_exists` (async), or call `resolve_exe` inside `tokio::task::spawn_blocking`.

---

### P2-4 · `acp_bridge_event_payload` is test-only but guarded by `cfg_attr` not `cfg(test)`

**File:** `crates/web/execute/events.rs` lines 136–142

Function is only called in tests. `#[cfg_attr(not(test), allow(dead_code))]` produces dead code in production builds.

**Fix:**
```rust
#[cfg(test)]
pub(super) fn acp_bridge_event_payload(event: &AcpBridgeEvent) -> Value { ... }
```

---

### P2-5 · `handle_ws` and `handle_ws_message` exceed 80-line warning threshold

**File:** `crates/web/ws_handler.rs`

`handle_ws`: 92 lines, `handle_ws_message`: 97 lines. Not hard fails but reduce readability and testability.

**Fix:** Extract `run_forward_task` as a named helper from `handle_ws`. Extract `handle_execute_message` from the `"execute"` arm of `handle_ws_message`.

---

### P2-6 · `handle_pulse_chat` exceeds 80-line warning threshold

**File:** `crates/web/execute/sync_mode/pulse_chat.rs` ~line 240–335 (96 lines)

**Fix:** Extract the one-shot vs. persistent dispatch decision into a `select_acp_execution_mode` helper.

---

### P2-7 · `docker_stats.rs` memory accounting includes page cache — inflated RSS

**File:** `crates/web/docker_stats.rs` lines 216–233

Uses raw `memory_stats.usage` which in cgroup v1 includes page cache. Docker's own `docker stats` subtracts `stats["cache"]`.

**Fix:**
```rust
let cache = stats.memory_stats.as_ref()
    .and_then(|m| m.stats.as_ref())
    .and_then(|s| s.get("cache").copied())
    .unwrap_or(0);
let mem_actual = mem_usage.saturating_sub(cache);
```

---

### P2-8 · `params.rs` reads env vars on every request

**File:** `crates/web/execute/sync_mode/params.rs` lines 32–46

`derive_cfg` calls `env::var("AXON_ACP_ADAPTER_CMD")` and `env::var("AXON_ACP_ADAPTER_ARGS")` on every WS request (120/min per connection). Unnecessary OS-level lock acquisition.

**Fix:** Cache these in `LazyLock<Option<String>>` statics or source them once during `Config` initialization.

---

### P2-9 · SEC-4: Empty `session_id` bypasses ACP system prompt injection

**File:** `crates/web/execute/sync_mode/pulse_chat.rs` lines 282–286

System prompt is injected when `session_id.is_none()`. Sending `session_id: ""` produces `Some("")`, which is treated as "continuing a session" and skips the system prompt without actually connecting to an existing one.

**Fix:** In `params.rs`, filter empty strings to `None`:
```rust
let session_id = flags.get("session_id")
    .and_then(serde_json::Value::as_str)
    .filter(|s| !s.is_empty())
    .map(ToString::to_string);
```

---

## P3 — Low Priority (nice to have)

---

### P3-1 · `select!` in forward task lacks `biased` — stats messages can starve output under load

**File:** `crates/web/ws_handler.rs` lines 94–124

Without `biased;`, stats broadcast (2/sec) can preempt crawl output messages (potentially thousands/sec) randomly.

**Fix:** Add `biased;` prioritising `exec_rx` → `tracking_rx` → `stats_rx`.

---

### P3-2 · `WsEventV2::JobStatus` and `WsEventV2::JobProgress` are dead variants

**File:** `crates/web/execute/events.rs` lines 99–111

Both variants carry `// NOTE: not emitted at runtime`. Binary bloat; confusing for frontend developers.

**Fix:** Gate with `#[cfg(feature = "job-push-events")]` or remove and re-add when implemented.

---

### P3-3 · `ASYNC_SUBPROCESS_MODES` is an empty `#[allow(dead_code)]` constant — misleading

**File:** `crates/web/execute/constants.rs` lines 89–91

An empty `&[&str]` with `allow(dead_code)` provides no value. Suggests unresolved design.

**Fix:** Remove entirely. Document the routing tier concept in a comment block if needed.

---

### P3-4 · `read_file` messages not rate-limited

**File:** `crates/web/ws_handler.rs` lines 244–263

`read_file` bypasses the rate limiter. Each message spawns a task doing `canonicalize` + `read_to_string` on user-supplied paths. Potential DoS via filesystem I/O amplification.

**Fix:** Apply the per-connection rate limiter to `read_file` messages (or a separate lower-rate cap, e.g. 60/min).

---

### P3-5 · `POLL_INTERVAL_MS` is 1000ms but docs say 500ms

**File:** `crates/web/docker_stats.rs` line 10

Constant is 1000ms; CLAUDE.md says "every 500ms". Pick one and update the other.

---

### P3-6 · SEC-5: Token visible in WS upgrade URL query string

**File:** `crates/web.rs` (shell WS upgrade path)

`?token=<value>` appears in server access logs and proxy logs for browser WebSocket clients that cannot set headers. Known trade-off noted in comments; flagged for operational awareness.

---

### P3-7 · SEC-6: `constant_time_eq` leaks token length via early-return on length mismatch

**File:** `crates/web/tailscale_auth.rs` lines 30–37

`if a.len() != b.len() { return false; }` leaks the expected token length through response timing. For a 32-char hex token the risk is low but the fix is trivial.

**Fix:** Use the `subtle` crate's `ConstantTimeEq` which handles length-mismatch correctly without branching.

---

## Monolith Policy Violations

| File | Lines | Limit | Status |
|------|-------|-------|--------|
| `crates/web/ws_handler.rs` | 510 | 500 | **FAIL** |
| `dispatch_search_and_info_modes` fn | 127 | 120 | **FAIL** |
| `handle_ws` fn | 92 | 80 | WARN |
| `handle_ws_message` fn | 97 | 80 | WARN |
| `handle_pulse_chat` fn | 96 | 80 | WARN |
| `dispatch_service` fn | ~110 | 80 | WARN |

---

## Security Summary

| ID | Severity | Description |
|----|----------|-------------|
| SEC-1 | **P0** | Shell loopback auth bypass — any local process gets PTY shell with no token |
| SEC-2 | P1 | `enable_fs`/`enable_terminal` ACP flags silently discarded |
| SEC-3 | P1 | `DefaultHasher` for session cache keying — collision maps wrong adapter |
| SEC-4 | P2 | Empty `session_id` bypasses system prompt injection |
| SEC-5 | P3 | Token in WS URL query string appears in access logs |
| SEC-6 | P3 | Timing side-channel leaks token length via length-mismatch early return |

---

## Correctly Implemented — No Action Required

- All file I/O uses `tokio::fs::*` — no blocking `std::fs` in async contexts
- No `std::thread::sleep` anywhere in `crates/web/` — correct `tokio::time::sleep` usage
- No `mod.rs` files — Rust 2018 file-per-module layout correct throughout
- `Box<dyn Error>` only at `start_server` (CLI boundary) — internal helpers return `String`/typed errors
- `send_or_sentinel` pattern correctly surfaces truncation rather than silently losing messages
- `serialize_raw_output_event` string-concatenation avoids double-serialization on hot streaming path
- Dual-semaphore guard (ACP + `SYNC_MODE_SEMAPHORE`) with `is_acp_mode` exclusion is correct
- `crawl_job_id` mutex held briefly (acquire/assign/release) — no guard across await points
- `session_ownership` H-8 binding via `DashMap::entry(...).or_insert_with(...)` is correct atomic test-and-set
- `spawn_blocking` in `shell.rs` blocking send/recv is the correct pattern for PTY I/O
- ACP `biased` select in `drive_turn_events` drains events before result — correct streaming behavior
- `AXON_ACP_ADAPTER_ARGS` pipe-delimiter design avoids shell quoting issues
- `cors.rs` avoids reflecting `Access-Control-Request-Headers` — CWE-942 protection is correct
