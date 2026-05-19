# Performance & Scalability Analysis: apps/web ↔ crates/web Bridge
**Date:** 2026-03-13
**Scope:** `crates/web.rs`, `crates/web/ws_handler.rs`, `crates/web/execute/`, `crates/web/docker_stats.rs`, `crates/web/shell.rs`, `apps/web/lib/axon-ws-exec.ts`, `apps/web/hooks/use-axon-ws.ts`, `apps/web/app/api/pulse/chat/`, `apps/web/lib/server/rate-limit.ts`

---

## Table of Contents

1. [WebSocket Channel Backpressure](#1-websocket-channel-backpressure)
2. [Docker Stats Broadcast Fan-Out](#2-docker-stats-broadcast-fan-out)
3. [ACP Session Semaphore Behavior Under Load](#3-acp-session-semaphore-behavior-under-load)
4. [Replay Cache Memory Growth](#4-replay-cache-memory-growth)
5. [Server-Side WS Singleton Pending Map](#5-server-side-ws-singleton-pending-map)
6. [Heartbeat and SSE Stream Cleanup](#6-heartbeat-and-sse-stream-cleanup)
7. [Non-ACP Sync Mode Concurrency — No Server-Side Gate](#7-non-acp-sync-mode-concurrency--no-server-side-gate)
8. [Forward Loop tokio::select! Starvation](#8-forward-loop-tokioselect-starvation)
9. [Memory Allocation in the Hot Path](#9-memory-allocation-in-the-hot-path)
10. [Rate Limiting Coverage Gaps](#10-rate-limiting-coverage-gaps)
11. [Additional Findings](#11-additional-findings)
12. [Summary Table](#12-summary-table)

---

## 1. WebSocket Channel Backpressure

**Severity: High**
**Impact: Message loss under sustained high-throughput commands (crawl + stats)**

### Mechanism

`ws_handler::handle_ws` creates two bounded `mpsc::channel::<String>(256)` channels:

- `exec_tx` / `exec_rx` — execution output from spawned command tasks
- `tracking_tx` / `tracking_rx` — `read_file` responses

A single forward task drives all three sources (`exec_rx`, `tracking_rx`, `stats_rx`) toward the single `ws_tx` WebSocket sink via `tokio::select!`. The WS sink is the rate-limiting bottleneck: every send must round-trip through the OS TCP stack and wait for client-side acknowledgment.

### Failure Mode

When a command produces output faster than the WS client can consume it, `exec_tx.send(v).await` in `ws_send.rs` blocks the producing task. All `ws_send` helpers silently discard the event on channel-full:

```rust
let _ = tx.send(v2).await;  // ws_send.rs, every helper
```

The `_` discard means a full channel does not produce an error — the message is dropped. For short-lived commands this is acceptable; for crawl output that can emit thousands of JSON lines, or for `assistant_delta` streams from ACP sessions, messages are silently lost to the browser with no indication.

The `stats_rx` is a `broadcast::Receiver`. `broadcast::channel(64)` overflow causes `RecvError::Lagged`, which is handled with an `Ok(...)` arm in the select — but a lagged client simply skips stats ticks, which is acceptable.

### Recommendations

- Change `ws_send` helpers to return `bool` and propagate channel-full as a WS `error` event to the client so the UI can display backpressure status.
- Consider increasing the `exec_tx` channel to 1024 for high-throughput modes (crawl).
- For the subprocess `read_stdout` path, the `tokio::spawn` that calls `send_json_owned` already clones `tx`, so all producers share the same 256-slot window; the cumulative pressure from stdout + stderr + stats simultaneously can realistically fill 256 slots during a crawl that emits 50+ lines/second.

---

## 2. Docker Stats Broadcast Fan-Out

**Severity: Medium**
**Impact: Stats delivery degraded or dropped for slow clients; container list re-scanned every 1000ms regardless of client count**

### Mechanism

`docker_stats.rs` uses a `broadcast::channel(64)` created in `web.rs`:

```rust
let (stats_tx, _) = broadcast::channel::<String>(64);
```

Every connected WS client subscribes with `state.stats_tx.subscribe()`. The broadcast channel is unbounded on the sender side: `tx.send(message)` in the stats loop returns `Ok(n_receivers)` or `Err(SendError)` if no receivers exist; it never blocks the sender. Tokio's broadcast channel drops the oldest messages when a receiver is 64 messages behind, emitting `RecvError::Lagged(n)` to that receiver.

### Issues Found

**Issue A — Container list scanned every 1000ms unconditionally.** `run_stats_loop` calls `docker.list_containers(...)` on every `interval.tick()`, regardless of how many WS clients are connected. With 0 clients, this is pure overhead. With 10+ clients, the Docker daemon receives a list-containers call per second that creates O(N) per-container stat streams unconditionally.

**Issue B — Per-container streaming goroutines not cleaned up promptly.** `stream_container_stats` spawns one `tokio::task` per container, which `stream: true` on bollard which runs indefinitely. The `streams.retain(|id, handle| { if !current_ids.contains(id) { handle.abort(); false } })` cleanup only fires on the next `interval.tick()`. If Docker itself restarts a container mid-cycle, there's a 1s window where a zombie stream task is still running and sending metrics to a dead `mpsc::Sender`.

**Issue C — Stats message is a freshly serialized JSON string on every tick**, including full `serde_json::json!` macro construction with nested objects. This is a heap allocation per second per connected client (the string is cloned for each broadcast receiver internally by tokio's broadcast channel). With 10 concurrent WS clients and 5 containers, that is 10 × 5 allocations/second at minimum.

**Issue D — 64-slot broadcast capacity vs 1000ms poll interval.** Each tick emits at most one message. At 1000ms interval, a receiver that is 64 ticks behind has fallen 64 seconds behind — meaning a slow client that can't drain the channel for over a minute will start dropping stats. That interval is fine for normal operation but highlights that the 64 cap is not load-tested for slower clients.

### Recommendations

- Skip the stats loop entirely when `stats_tx.receiver_count() == 0` (broadcast sender exposes this).
- Cache the serialized stats string per poll cycle and share the same `Arc<String>` across all broadcast sends to eliminate per-receiver allocation.
- Reduce the poll interval to 2000ms (stats update every 2s is imperceptible to humans; halves Docker socket pressure).

---

## 3. ACP Session Semaphore Behavior Under Load

**Severity: High**
**Impact: Users wait up to 30s silently with no feedback before receiving a hard error**

### Mechanism

`web.rs` defines:

```rust
pub(crate) static ACP_SESSION_SEMAPHORE: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(max));  // default max=8
```

In `execute.rs`, `acquire_acp_permit` wraps acquisition with a 30-second timeout:

```rust
const ACP_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(30);
match tokio::time::timeout(ACP_ACQUIRE_TIMEOUT, ACP_SESSION_SEMAPHORE.acquire()).await {
    Ok(Ok(permit)) => Ok(Some(permit)),
    Err(_) => {
        send_error_dual(tx, ws_ctx, "ACP session queue full — timed out after 30s...").await;
        Err(())
    }
}
```

### Issues Found

**Issue A — No queue-depth feedback.** A user submitting `pulse_chat` when all 8 permits are held waits 30 seconds, receives a generic `"timed out after 30s"` error, and has no information about current queue depth or estimated wait. The WS client sees silence for the full 30s, which will look like a hang.

**Issue B — 30s timeout is poorly tuned for the UX.** ACP sessions hold a permit for the entire turn duration — up to the 300s CLAUDE_TIMEOUT_MS. If 8 sessions are all running 5-minute tasks, a ninth user waits 30s and then fails, even though a slot may become free in 31 seconds. The timeout should match the median turn duration, not be a fixed 30s.

**Issue C — Permit held across the entire `handle_sync_direct` call.** The `_acp_permit` is dropped after `handle_sync_direct` returns. For `pulse_chat`, this means the permit is held for the entire ACP turn, which is correct. But `pulse_chat_probe` (a lightweight probe) is also subject to this semaphore, which is unnecessarily restrictive — it blocks real chat turns.

**Issue D — No queued-count metric.** The semaphore has no introspection for how many waiters are queued (`tokio::sync::Semaphore` does not expose a waiter count), so the server cannot emit a `queue_position` event to the waiting client.

### Recommendations

- Emit a `{"type":"status","phase":"queued"}` event on the WS immediately when `acquire` blocks, before the 30s countdown starts. This tells the client "I'm waiting for a slot" rather than appearing frozen.
- Give `pulse_chat_probe` its own smaller semaphore (e.g., 4 permits) so probe calls do not contend with real turns.
- Log a warning when semaphore waiter count implies sustained saturation (proxy via a separate atomic counter).

---

## 4. Replay Cache Memory Growth

**Severity: Medium**
**Impact: Bounded but non-obvious dual-layer memory usage; Redis persist queue can silently stall**

### Rust-side replay cache (`session_cache.rs`)

The Rust `SESSION_CACHE` per-session buffer enforces:
- `MAX_REPLAY_BUFFER = 4096` messages (count cap)
- `MAX_REPLAY_BUFFER_BYTES = 4 MiB` (byte cap)

At 8 concurrent sessions each holding 4 MiB, peak committed replay memory is **32 MiB**. This is acceptable, but it is additive on top of the adapter subprocess RSS (Claude CLI is ~200-300 MiB per process). With `AXON_ACP_MAX_CONCURRENT_SESSIONS=8`, worst-case total is approximately 2.5 GiB from ACP alone.

The drain behavior is destructive (`std::mem::take`): once a reconnecting client calls `acp_resume`, the buffer is consumed. A second reconnect of the same client within the same session window sees an empty buffer (found as A-M3 in the phase 1 context). This is a correctness risk — a client that reconnects twice (e.g., due to flaky mobile network) loses the middle segment silently.

### TypeScript-side replay cache (`replay-cache.ts`)

The Next.js route's in-process replay cache enforces:
- `REPLAY_BUFFER_LIMIT = 512` events per entry
- `REPLAY_CACHE_MAX_ENTRIES = 64` concurrent sessions
- `REPLAY_CACHE_MAX_TOTAL_BYTES = 8 MiB` total

At `REPLAY_CACHE_MAX_TOTAL_BYTES = 8 MiB` with `replayCache.set(key, entry)` + `runningTotalBytes`, the eviction in `evictOldestEntries()` correctly bounds total memory. However:

**Issue A — SHA-256 key recomputed on every `pruneReplayCache` call.** `computeReplayKey` is called at request start only, which is correct — but `pruneReplayCache(Date.now())` is O(N) over all entries on every POST to `/api/pulse/chat`. With 64 active entries this is negligible; it is worth noting in case the entry count grows.

**Issue B — Redis persist debounce timer leak.** `schedulePersist` sets a 150ms debounce timer per key, tracked in `persistTimers`. The `pendingPersist` map is capped at 100 entries. If `persistTimers` has an entry for a key and `pendingPersist` is full for a *different* key, `schedulePersist` returns early without scheduling, and that key's entry is never persisted to Redis until the next call. The `pendingPersist.set(key, entry)` call in `upsertReplayEntry` is unconditional (it runs even when `schedulePersist` returned early), so `pendingPersist` grows but the timer that would consume it is never scheduled. This is a subtle logic inversion that can cause entries to accumulate in `pendingPersist` without ever flushing.

**Issue C — `replayFromLastEventId` does not handle duplicate event IDs.** `findIndex` returns the first occurrence; if event IDs are not unique, replay may return incomplete data.

### Recommendations

- Rust: Change `drain_replay_buffer` to take a `from_index` parameter so replay is non-destructive (copy tail instead of drain). This supports multiple reconnects without data loss.
- TS: Audit `schedulePersist` / `pendingPersist` interaction — `pendingPersist.set` should only run when `schedulePersist` actually scheduled a timer, not unconditionally.

---

## 5. Server-Side WS Singleton Pending Map

**Severity: High**
**Impact: Connection failure during any pending request fails ALL concurrent requests simultaneously**

### Mechanism

`axon-ws-exec.ts` maintains a module-level singleton:

```ts
const _pending = new Map<string, PendingRequest>()
let _ws: WsLike | null = null
```

All API routes that call `runAxonCommandWs` or `runAxonCommandWsStream` share this single connection and pending map. This is intentional (multiplexing), but has critical failure implications:

### Issues Found

**Issue A — Single connection failure cascades to all in-flight requests.** When `_ws` closes (`close` event fires), `failAllPending` is called immediately, rejecting every pending request with the connection error. If `cortex/stats`, `cortex/sources`, and a `pulse/chat` are all in flight simultaneously, all three fail at once. The `pulse/chat` route wraps the failure in its SSE stream and emits an error event to the client — but the other two routes propagate a 500 to the UI simultaneously.

```ts
ws.addEventListener('close', (event) => {
    failAllPending(new Error(`WebSocket closed unexpectedly (code ${event.code})`))
    reject(new Error(`WebSocket closed before open (code ${event.code})`))
})
```

**Issue B — Memory footprint per pending entry.** Each `PendingRequest` holds a reference to the full `RunAxonCommandWsStreamOptions` including closures (`onJson`, `onOutputLine`, `onDone`, `onError`). In a Next.js server-side environment, each closure may close over request-specific state (response headers, stream controllers, parsed request bodies). Under 40 req/min rate limit for `pulse/chat` alone, the pending map could hold 40 entries simultaneously at peak, each with closures referencing live SSE streams.

**Issue C — No maximum pending map size.** `_pending` has no cap. Under a thundering-herd scenario (burst of parallel API route calls) and a slow WS or stalled Rust side, the map grows unbounded. Each entry includes a `setTimeout` timer and a promise `resolve`/`reject` pair.

**Issue D — `pulse_chat` requests default to 30s timeout via `runAxonCommandWs`**, but `route.ts` calls `runAxonCommandWsStream` with `CLAUDE_TIMEOUT_MS = 300_000`. The 300s timeout means a pulse_chat entry can sit in `_pending` for up to 5 minutes before being settled, while other commands on the same singleton WS complete in milliseconds. A 5-minute entry's timer holds its closures in memory for that duration.

### Recommendations

- Add a `MAX_PENDING` cap (e.g., 50) — reject with `503` when the map is full, rather than accumulating unbounded.
- Log a warning when `_pending.size > 20` to signal API route pressure before it becomes a problem.
- The cascade-fail behavior on WS close is a known architectural risk (H-3 from Phase 1). The only structural fix is connection-per-request for high-stakes paths (`pulse_chat`) vs. the shared singleton for low-stakes polling routes.

---

## 6. Heartbeat and SSE Stream Cleanup

**Severity: Medium**
**Impact: Leaked `setInterval` timers and unclosed SSE streams on client disconnect**

### Mechanism

`route.ts` starts a `setInterval` heartbeat inside the `ReadableStream.start()` callback:

```ts
const heartbeatInterval = setInterval(() => {
    if (closed) return
    if (Date.now() - lastEmitAt < HEARTBEAT_INTERVAL_MS) return
    emit({ type: 'heartbeat', elapsed_ms: Date.now() - startedAt })
}, HEARTBEAT_INTERVAL_MS)  // 5000ms
```

`cleanup()` is called from `onDone` and `onError` callbacks, which calls `clearInterval(heartbeatInterval)`.

### Issues Found

**Issue A — Cleanup only runs on WS command completion, not on client disconnect.** If the browser disconnects mid-stream (navigation, tab close, network loss), `request.signal.abort()` fires. The `abortHandler` sets `aborted = true` but does NOT call `cleanup()`. This means `heartbeatInterval` continues running after disconnect until the WS command eventually settles (`onDone`/`onError`). In the `pulse_chat` case, that can be up to 300s.

```ts
const abortHandler = () => {
    aborted = true  // does NOT call cleanup()
}
```

With 40 concurrent clients hitting `pulse_chat` and then disconnecting before completion, up to 40 `setInterval` timers remain active at 5s intervals. Each fires `emit()` which attempts `controller.enqueue()` — which catches on `closed = true` — but the timer itself is not cleared.

**Issue B — `enqueueEvent` catches controller close but does not propagate as a full cleanup.** The `closed = true` guard in `enqueueEvent` prevents double-enqueue after disconnect, but the `heartbeatInterval` and `abortHandler` event listener are never cleaned up. The `request.signal.removeEventListener('abort', abortHandler)` is only called from within `finish()`, which is inside `runAxonCommandWsStream`, not from the SSE stream's abort path.

**Issue C — `runAxonCommandWsStream` is launched with `void` (fire-and-forget).** The promise is intentionally not awaited:

```ts
void runAxonCommandWsStream('pulse_chat', { signal: request.signal, ... }).catch(...)
```

After the request's `AbortSignal` fires, the underlying WS command continues running against `_pending` until the 300s timeout expires. The `signal` is passed to `runAxonCommandWsStream` and `onAbort` calls `settlePending(execId, ...)`, which will reject the pending entry. But this only fires when the signal is `aborted` — and the actual heartbeat timer cleanup depends on `cleanup()` being called from `onDone`/`onError`, which in turn depends on the WS command completing.

### Recommendations

- In `abortHandler`, call `cleanup()` directly in addition to setting `aborted = true`. This immediately clears the heartbeat timer on client disconnect regardless of WS command state.
- Add a `finally` block in the `ReadableStream.start` constructor to call `cleanup()` on any exit path.

---

## 7. Non-ACP Sync Mode Concurrency — No Server-Side Gate

**Severity: High**
**Impact: Unbounded concurrent service calls; scrape/query/ask can saturate TEI/Qdrant/LLM under load**

### Mechanism

`execute.rs` routes non-ACP sync modes through `sync_mode::handle_sync_direct` with no concurrency limit:

```rust
// No semaphore check for non-ACP modes
if let Some(params) = sync_mode::classify_sync_direct(&context) {
    // ACP permit only acquired for pulse_chat / pulse_chat_probe
    let permit_result = acquire_acp_permit(&mode, &tx, &ws_ctx).await;
    // acquire_acp_permit returns Ok(None) for all non-ACP modes
    ws_send::send_command_start(&tx, &context).await;
    sync_mode::handle_sync_direct(params, tx, ws_ctx, permission_responders).await;
    return;
}
```

`acquire_acp_permit` returns `Ok(None)` for any mode that is not `pulse_chat` or `pulse_chat_probe`. The `_acp_permit` that is `drop`ped afterwards holds `None` — the semaphore is never touched.

This means:
- 100 concurrent `ask` requests → 100 concurrent Qdrant vector searches + 100 concurrent LLM `POST /chat/completions` calls
- 100 concurrent `scrape` requests → 100 concurrent Chrome/HTTP fetches hitting the same Chrome instance
- 100 concurrent `evaluate` requests → 100 concurrent `std::thread::spawn` calls, each spinning up a new `tokio::runtime::Builder::new_current_thread()`

The `evaluate` case is particularly concerning:

```rust
pub(super) fn call_evaluate(cfg: Arc<Config>, question: String) -> Pin<Box<...>> {
    Box::pin(async move {
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("evaluate runtime");
            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, async move { ... });
        });
        rx.await ...
    })
}
```

Each `evaluate` call spawns a dedicated OS thread and a dedicated single-threaded Tokio runtime. There is no bound on how many can run simultaneously. 50 concurrent `evaluate` calls create 50 OS threads and 50 runtimes, each performing LLM I/O.

The subprocess fallback modes (`suggest`, `screenshot`, `debug`, `sessions`, `dedupe`, `refresh`) also lack any concurrency gate — each `tokio::spawn`s `Command::new(&exe)` without limit. 100 concurrent `dedupe` requests would launch 100 `axon dedupe` subprocesses simultaneously.

### Recommendations

- Add a `SYNC_SERVICE_SEMAPHORE` (configurable via `AXON_SYNC_MAX_CONCURRENT`, suggested default 32) applied to all non-ACP `handle_sync_direct` calls.
- Add a separate `EVALUATE_SEMAPHORE` (default 4) specifically for `evaluate` to cap thread spawning.
- Add a `SUBPROCESS_SEMAPHORE` (default 8) for the subprocess fallback path.
- Rate limiting in the Next.js layer (40 req/min for `pulse/chat`) partially mitigates this, but the WS path has no such protection — a single browser tab can submit rapid-fire `execute` messages.

---

## 8. Forward Loop `tokio::select!` Starvation

**Severity: Low**
**Impact: Stats delivery may starve exec output or vice versa under sustained throughput**

### Mechanism

`ws_handler.rs` forward task:

```rust
loop {
    tokio::select! {
        Some(msg) = exec_rx.recv() => { ... }
        Some(msg) = tracking_rx.recv() => { ... }
        Ok(stats_msg) = stats_rx.recv() => { ... }
        else => break,
    }
}
```

`tokio::select!` without `biased` uses a pseudorandom polling order by default, meaning no arm is systematically preferred. This is correct fairness behavior for equal-priority sources.

### Issue Found

**The `stats_rx` arm uses `Ok(stats_msg) = stats_rx.recv()`** where `stats_rx` is a `broadcast::Receiver`. When `stats_rx.recv()` returns `Err(RecvError::Lagged(n))`, the arm does NOT match (the `Ok(...)` pattern rejects the `Err`). This means a lagged receiver silently falls through to the `else => break` arm only if all other arms also produce no value simultaneously.

In practice, `else => break` only fires when **all** arms are ready with `None`/error simultaneously — which requires `exec_rx`, `tracking_rx`, and `stats_rx` all to be closed at the same time. This is correct. But the lagged stats messages are silently discarded (expected behavior, but worth noting as the stats panel may appear stale during high-throughput crawl output).

### Recommendation

No code change needed. The behavior is correct. Consider adding a `log::debug!` when `RecvError::Lagged(n)` is encountered to aid in tuning the broadcast channel size if stats delivery becomes a user complaint.

---

## 9. Memory Allocation in the Hot Path

**Severity: Medium**
**Impact: Multiple avoidable heap allocations per WS message in the streaming path**

### Allocation hot-spots identified

**9-A: `context.to_ws_ctx()` clones on every message send.**
`ExecCommandContext::to_ws_ctx()` creates a new `CommandContext` by cloning `exec_id`, `mode`, and `input` strings. In `subprocess.rs::read_stdout`, every line of stdout calls `send_json_owned(tx.clone(), ctx.clone(), parsed)` and `send_command_output_line(&tx, &ctx, clean)`. For a 10,000-line crawl output, this is 10,000 `String::clone()` calls for `exec_id`/`mode`/`input` plus 10,000 `mpsc::Sender::clone()` calls.

`CommandContext` should be wrapped in `Arc<CommandContext>` so cloning is a reference count bump, not a string copy.

**9-B: ANSI stripping allocates a new `String` per line.**
`exe::strip_ansi(&line)` calls `console::strip_ansi_codes()` which always returns an owned `String` even when no ANSI codes are present (the no-op case). For the majority of lines with no ANSI codes, this is a full string copy. A fast pre-scan (`memchr` for ESC byte) before stripping would eliminate most allocations.

**9-C: `serialize_v2_event` performs `serde_json::to_string` on every send.**
Every WS message goes through `serialize_v2_event(WsEventV2::CommandOutputLine { ctx: context.clone(), line })` which serializes the full event envelope to JSON on every call. For streaming LLM output (`assistant_delta` tokens), this serialization occurs hundreds of times per second.

**9-D: `serde_json::json!` macro in `emit_log`.**
`async_mode.rs::emit_log` uses `json!({"type": "log", "line": line}).to_string()` which heap-allocates a `serde_json::Map` + serializes it on every log emit. A pre-formatted string template would be significantly faster.

**9-E: `Arc<Config>` clone per request is cheap (refcount), not a deep copy.**
The `context.cfg.apply_overrides(&overrides)` in `async_mode.rs` does create a new `Config` struct per async enqueue — this is intentional and acceptable.

### Recommendations

- Wrap `CommandContext` in `Arc<CommandContext>` in `events.rs`; update `to_ws_ctx()` to return `Arc<CommandContext>`.
- Add a fast-path in `strip_ansi`: `if !line.contains('\x1b') { return line.to_owned(); }`.
- Pre-allocate log event templates as `const` format strings for the common `{"type":"log","line":"..."}` case.

---

## 10. Rate Limiting Coverage Gaps

**Severity: High**
**Impact: WebSocket execution path has zero server-side rate limiting**

### What IS rate-limited

The `enforceRateLimit` function is applied to:
- `POST /api/pulse/chat` — 40 req/min
- `GET /api/logs` — 30 req/min
- `POST /api/pulse/save` — (check applied)
- `GET /api/sessions/[id]` — (check applied)
- `GET /api/sessions/list` — (check applied)

### What is NOT rate-limited

**The `/ws` WebSocket path has no rate limiting at all.** Any authenticated client (valid `AXON_WEB_API_TOKEN`) can:
- Send unlimited `execute` messages over a single WS connection
- Send unlimited rapid-fire `pulse_chat` messages (bypassing the 40/min HTTP rate limit since they go over WS, not via `/api/pulse/chat`)
- Trigger unlimited subprocess spawns, Qdrant queries, LLM calls, and Chrome fetches

The HTTP rate limit on `/api/pulse/chat` only applies to the Next.js-to-Rust WS bridge path. A browser tab connecting directly to `/ws` (or a script) can `pulse_chat` at full speed, limited only by the ACP semaphore (8 slots). Once 8 slots are occupied, additional WS clients wait for 30s and then fail — the semaphore acts as a soft rate limiter but only for ACP modes, not for `scrape`, `query`, `ask`, etc.

**Additional unprotected routes:**
- `/api/cortex/stats` — no rate limit; calls `runAxonCommandWs('stats', 30_000)` on every GET
- `/api/cortex/sources`, `/api/cortex/domains`, `/api/cortex/doctor` — no rate limits
- `/api/cortex/suggest` — no rate limit; calls Qdrant + LLM

The `cortex` routes have `s-maxage=30, stale-while-revalidate=60` cache headers (via `next.config.ts`), which reduces load from CDN-cached clients, but in a self-hosted deployment without a CDN, these headers have no effect — every request hits the route handler.

### Recommendations

- Add `enforceRateLimit` to all `/api/cortex/*` routes (suggested: 60 req/min each).
- Add a per-WS-connection message rate limit in `ws_handler.rs` using a token bucket or leaky bucket. A simple approach: track `last_execute_at` per connection and reject `execute` messages within a minimum interval (e.g., 100ms) with a WS `error` event.
- For the ACP-bypass risk, add a rate limit within `handle_command` before the `ALLOWED_MODES` check that is applied to all WS execute messages.

---

## 11. Additional Findings

### 11-A: Shell PTY — No Concurrency Limit (Medium)

`crates/web/shell.rs` spawns one PTY (full OS process) per `/ws/shell` connection with no limit. Each PTY spawns a `$SHELL` subprocess plus two `tokio::task::spawn_blocking` tasks. There is no `SHELL_SEMAPHORE`. The shell WS upgrade gate requires authentication only for non-loopback IPs — any authenticated remote client can open unlimited shells.

**Recommendation:** Add a `SHELL_SESSION_SEMAPHORE` (default 4) and return a WS close frame (code 1013 "Try Again Later") when the limit is reached.

### 11-B: `send_or_buffer` Dual-Lock Pattern on Hot Path (Low)

In `pulse_chat.rs`, `send_or_buffer` acquires two `std::sync::Mutex` locks sequentially on the buffering path (the disconnect case):

```rust
if tx.send(msg.clone()).await.is_err()
    && let Some(cached) = SESSION_CACHE.get_sync(agent_key)
{
    cached.buffer_event(msg);  // acquires replay_buffer_bytes lock, then replay_buffer lock
}
```

`buffer_event` locks `replay_buffer_bytes` then `replay_buffer` in sequence. These are fine-grained mutexes (one lock per field), not a single coarse lock, which means two mutex acquisitions per buffered event. This is only on the disconnect path, so the performance impact is low. No change needed — the two separate mutexes allow independent inspection of byte size without holding the buffer lock.

### 11-C: `evaluate` Spawns a New Tokio Runtime Per Call (High, covered in §7)

Already covered in Finding 7. Calling `evaluate` concurrently is the highest-risk single-operation for resource exhaustion: OS threads + Tokio runtimes are non-trivial resources and have no pool.

### 11-D: `job_dirs` DashMap — No TTL Eviction (Low)

`AppState.job_dirs: Arc<DashMap<String, PathBuf>>` accumulates crawl job directory entries indefinitely. There is no eviction mechanism. In a long-running server process with many crawl jobs, this map grows without bound (one entry per completed crawl). Each entry is a `String` (UUID key) + `PathBuf` (~64 bytes), so growth is slow but unbounded.

**Recommendation:** Add a reaper that removes entries whose `PathBuf` targets no longer exist on disk (i.e., output directories have been cleaned up), or cap at MAX_JOB_DIRS (e.g., 10,000) with LRU eviction.

### 11-E: `DefaultHasher` for MCP Fingerprint — Not Collision-Resistant (Low)

`pulse_chat.rs::fingerprint_mcp_servers` uses `std::hash::DefaultHasher`:

```rust
fn fingerprint_mcp_servers(mcp_servers: &[AcpMcpServerConfig]) -> u64 {
    let raw = serde_json::to_string(mcp_servers).unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    hasher.write(raw.as_bytes());
    hasher.finish()
}
```

`DefaultHasher` is explicitly documented as not stable across Rust versions and not collision-resistant. Two different MCP server configurations could produce the same fingerprint, causing the wrong cached adapter to be reused. This is a correctness risk, not just a performance risk.

**Recommendation:** Replace with `FxHasher` (faster than SHA, deterministic) or simply use the raw JSON string as the key segment (truncated to 32 chars) since it is only used as a cache key within a single process lifetime.

---

## 12. Summary Table

| # | Finding | Severity | Component | Primary Impact |
|---|---------|----------|-----------|---------------|
| 1 | WS channel backpressure — silent message drop | **High** | `ws_handler.rs`, `ws_send.rs` | Crawl output silently lost to browser |
| 2 | Docker stats — unconditional 1s polling + per-client allocation | **Medium** | `docker_stats.rs` | Docker socket pressure; heap alloc per client/tick |
| 3 | ACP semaphore — 30s silent wait, no feedback | **High** | `execute.rs`, `web.rs` | Users see hang; probe contends with chat |
| 4A | Rust replay buffer drain is destructive | **Medium** | `session_cache.rs` | Double-reconnect loses replay data |
| 4B | TS replay cache `pendingPersist` timer logic | **Medium** | `replay-cache.ts` | Stale entries never flushed to Redis |
| 5 | WS singleton cascade-fail + unbounded pending map | **High** | `axon-ws-exec.ts` | Single WS failure rejects all in-flight API calls |
| 6 | SSE stream heartbeat timer not cleared on client abort | **Medium** | `pulse/chat/route.ts` | Timer leak per abandoned session (up to 300s) |
| 7 | No concurrency gate on non-ACP sync modes | **High** | `execute.rs`, `service_calls.rs` | Unbounded Qdrant/LLM/Chrome concurrent requests |
| 7C | `evaluate` spawns OS thread + Tokio runtime per call | **High** | `service_calls.rs` | Resource exhaustion under concurrent load |
| 8 | `tokio::select!` stats lag silent discard | **Low** | `ws_handler.rs` | Cosmetic: stale stats panel during crawl |
| 9 | Hot-path string clones in output streaming | **Medium** | `subprocess.rs`, `ws_send.rs` | CPU/heap pressure at 1000+ lines/sec |
| 10 | WS execute path has no rate limiting | **High** | `ws_handler.rs`, cortex routes | ACP semaphore bypass; cortex endpoint abuse |
| 11A | Shell PTY — no concurrency limit | **Medium** | `shell.rs` | Unlimited OS processes from authenticated clients |
| 11D | `job_dirs` DashMap — no TTL eviction | **Low** | `web.rs` | Unbounded memory growth in long-running server |
| 11E | `DefaultHasher` for MCP fingerprint | **Low** | `pulse_chat.rs` | Hash collision → wrong cached adapter reused |

### Priority Order for Remediation

1. **Finding 7** — Add sync service semaphore (especially for `evaluate`). Highest blast radius.
2. **Finding 10** — WS execute rate limiting. Prerequisite for safe public deployment.
3. **Finding 3** — Emit `queued` status event before 30s timeout; split probe semaphore.
4. **Finding 5** — Cap `_pending` map size; document cascade-fail risk.
5. **Finding 1** — Return error on channel-full instead of silent discard.
6. **Finding 6** — Call `cleanup()` in `abortHandler`.
7. **Finding 4B** — Fix `schedulePersist` / `pendingPersist` ordering.
8. **Finding 9** — Wrap `CommandContext` in `Arc`; fast-path ANSI strip.
9. **Finding 2** — Skip stats polling when no clients; cache serialized message.
10. **Findings 11A, 11D, 11E** — Shell semaphore; `job_dirs` TTL; replace `DefaultHasher`.
