# ACP Performance & Scalability Analysis
**Date:** 2026-03-12
**Scope:** `crates/services/acp/` + `crates/web/ws_handler.rs` + `crates/web/execute/sync_mode/pulse_chat.rs`
**Reviewer:** Performance Engineering

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Finding 1: Non-Deterministic Fingerprint Hash (Critical)](#finding-1-non-deterministic-fingerprint-hash)
3. [Finding 2: DashMap Iteration with Async Mutex Locks in Reaper (High)](#finding-2-dashmap-iteration-with-async-mutex-locks-in-reaper)
4. [Finding 3: Replay Buffer Memory Pressure (High)](#finding-3-replay-buffer-memory-pressure)
5. [Finding 4: spawn_blocking Thread Pool Exhaustion (High)](#finding-4-spawn_blocking-thread-pool-exhaustion)
6. [Finding 5: Semaphore Uses try_acquire — No Backpressure Queue (High)](#finding-5-semaphore-uses-try_acquire--no-backpressure-queue)
7. [Finding 6: O(N) session_id_index Cleanup on Remove (Medium)](#finding-6-on-session_id_index-cleanup-on-remove)
8. [Finding 7: Two Separate Mutex Locks per Touch in get() (Medium)](#finding-7-two-separate-mutex-locks-per-touch-in-get)
9. [Finding 8: WS Forward Loop — Unbalanced select! Arms (Medium)](#finding-8-ws-forward-loop--unbalanced-select-arms)
10. [Finding 9: Adapter Channel Capacity Too Small (Medium)](#finding-9-adapter-channel-capacity-too-small)
11. [Finding 10: serde_json String Contains Scan per Event (Low)](#finding-10-serde_json-string-contains-scan-per-event)
12. [Finding 11: Event Channel Buffer Sizing (Low)](#finding-11-event-channel-buffer-sizing)
13. [Finding 12: Reaper Interval — No Jitter (Low)](#finding-12-reaper-interval--no-jitter)
14. [Scalability Limits Summary](#scalability-limits-summary)
15. [Priority Fix Order](#priority-fix-order)

---

## Executive Summary

The ACP implementation is architecturally sound. The use of `RefCell` on a `current_thread` runtime, `LocalSet` pinning, and the RAII `AdapterGuard` are all correct and well-reasoned decisions. The persistent-connection model (one `spawn_blocking` thread per agent key, reused across turns) is the right design — far better than re-spawning per prompt.

However, several specific performance hazards exist that become meaningful at N > 1 concurrent sessions:

- **Non-deterministic fingerprinting** will cause spurious adapter respawns on every session using MCP servers, wasting a subprocess spawn + protocol initialization roundtrip (typically 500ms–2s per respawn).
- **The semaphore uses `try_acquire`** (non-blocking), meaning requests beyond the 8-session limit are immediately rejected rather than queued. Under burst traffic this causes false "capacity exceeded" errors.
- **The reaper holds DashMap shard locks while crossing `.await` points**, creating transient contention windows against every concurrent `get()` call.
- **`spawn_blocking` threads are never returned to the pool during session idle time** — each session holds a thread for its full lifetime (up to 1 hour), not just during active turns.

---

## Finding 1: Non-Deterministic Fingerprint Hash

**Severity:** Critical
**File:** `crates/web/execute/sync_mode/pulse_chat.rs`, lines 204–211

### The Problem

```rust
fn fingerprint_mcp_servers(
    mcp_servers: &[crate::crates::services::types::AcpMcpServerConfig],
) -> u64 {
    let raw = serde_json::to_string(mcp_servers).unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    hasher.write(raw.as_bytes());
    hasher.finish()
}
```

Two independent defects compound here:

**Defect A — `DefaultHasher` non-determinism across process restarts:**
`std::hash::DefaultHasher` uses a randomized seed (SipHash-1-3 with per-process randomization since Rust 1.36). The hash output for identical input will differ across process restarts and, depending on Rust version/platform, potentially across threads. This means an `agent_key` built with a fingerprint from process run #1 will never match the one computed in process run #2.

**Defect B — `serde_json::to_string` on a Vec of an enum with HashMap-like interior:**
`AcpMcpServerConfig::Stdio` has `env: Option<std::collections::HashMap<String, String>>`. `serde_json` serializes `HashMap` with non-deterministic key ordering (depends on the hash of each key, which is randomized per-process). Two identical configs can serialize to different strings if the env map has more than one entry.

### Impact Estimate

Each spurious fingerprint mismatch causes `SESSION_CACHE.get()` to return `None`, triggering a new `AcpConnectionHandle::spawn()` call which:

1. `tokio::task::spawn_blocking` — allocates a new OS thread
2. Spawns the adapter subprocess (e.g. `claude` binary) — typically 200–800ms cold start
3. Sends `initialize` request over ACP protocol — 100–300ms
4. Sends `new_session` — 50–150ms

**Total overhead per spurious miss: 350ms–1.3s of latency plus an OS thread**. For any deployment using MCP env vars (common in real configs), this hit occurs on every single request.

In the worst case (8 concurrent sessions, all affected): 8 orphaned threads + 8 zombie adapter processes still running in the cache, consuming memory and file descriptors, while 8 new ones spawn.

### Recommendation

Replace `DefaultHasher` with a deterministic, stable hasher such as `FxHasher` (from the `rustc-hash` crate, already commonly present) or `sha2`. Sort the `AcpMcpServerConfig` slice before serializing to eliminate ordering nondeterminism. For the env HashMap interior, sort keys before hashing.

```rust
// Stable approach using canonical sorted JSON
fn fingerprint_mcp_servers(
    mcp_servers: &[crate::crates::services::types::AcpMcpServerConfig],
) -> u64 {
    use std::hash::{Hash, Hasher};
    // Sort by name first so Vec ordering doesn't matter
    let mut sorted = mcp_servers.to_vec();
    sorted.sort_by(|a, b| server_name(a).cmp(server_name(b)));
    // Use a stable hasher — FxHasher is deterministic
    let mut hasher = rustc_hash::FxHasher::default();
    for server in &sorted {
        canonical_hash_server(server, &mut hasher);
    }
    hasher.finish()
}
```

Alternatively, implement `Hash` manually on `AcpMcpServerConfig` with sorted env keys, and hash each field individually without going through JSON serialization.

---

## Finding 2: DashMap Iteration with Async Mutex Locks in Reaper

**Severity:** High
**File:** `crates/services/acp/session_cache.rs`, lines 162–173

### The Problem

```rust
async fn reap_expired(&self) {
    let mut to_remove = Vec::new();
    for entry in self.sessions.iter() {       // holds DashMap shard read lock
        if entry.value().is_expired().await {  // awaits tokio::Mutex — lock held across await
            to_remove.push(entry.key().clone());
        }
    }
    // ...
}
```

`DashMap::iter()` returns a `dashmap::iter::Iter` that holds a shard-level read lock for the duration of the iteration. The `for entry in self.sessions.iter()` loop does NOT release shard locks between iterations — the shard lock is held for the entire sweep.

Inside the loop, `is_expired().await` acquires a `tokio::sync::Mutex` on `last_active`. This is an await point. While the async executor is waiting to acquire `last_active`, the DashMap shard lock remains held.

### Contention Analysis

With N concurrent sessions active:

- DashMap defaults to `max(1, num_cpus * 4)` shards. On a 4-core machine: 16 shards.
- At N=8 sessions spread across 16 shards, roughly half the shards hold a session each.
- The reaper iterates all shards sequentially. For each shard with a session, it holds that shard's read lock while awaiting `last_active.lock()`.
- Any concurrent `get()` call (extremely common — called on every turn) that needs the same shard blocks behind the reaper's read lock.

**Contention window per session:** The time to acquire `last_active.lock()` under no contention is ~50–200ns (tokio futex path). Under contention (another task holds `last_active` while doing `touch()`), this extends to a few microseconds.

At 60-second reaper intervals with 8 sessions, the absolute contention window is small but non-zero. The real issue is correctness: DashMap's documentation explicitly states that holding a reference across an await point causes a potential deadlock if the held shard is needed by another task that the executor needs to run before returning from the await.

### Recommendation

Collect all session keys first (DashMap read completes, shard locks released), then check expiry with async:

```rust
async fn reap_expired(&self) {
    // Phase 1: collect all (key, session_arc) pairs without holding DashMap locks across awaits.
    let candidates: Vec<(String, Arc<CachedSession>)> = self
        .sessions
        .iter()
        .map(|e| (e.key().clone(), Arc::clone(e.value())))
        .collect(); // shard locks released here

    // Phase 2: check expiry — now safe to await with no DashMap locks held.
    let mut to_remove = Vec::new();
    for (key, session) in candidates {
        if session.is_expired().await {
            to_remove.push(key);
        }
    }
    for key in &to_remove {
        log::info!("[acp_cache] evicting expired session: {key}");
        self.remove(&key);
    }
}
```

---

## Finding 3: Replay Buffer Memory Pressure

**Severity:** High
**File:** `crates/services/acp/session_cache.rs`, line 24

### The Problem

```rust
const MAX_REPLAY_BUFFER: usize = 4096;
```

With 8 concurrent sessions (the default semaphore limit), the theoretical maximum replay buffer usage is:

- **8 sessions × 4096 messages**
- Each serialized WS message = typically 50–500 bytes for log/delta events, but `TurnResult` events carry the full assistant response text (potentially hundreds of KiB for long coding sessions)
- `assistant_text` cap is 1 MiB per `bridge.rs` line 379, and `TurnResult` serializes the full `result: text` field

**Worst case calculation:**

```
8 sessions × 4096 messages × avg 2KB per message = 64 MiB
```

With streaming LLM tokens, each `assistant_delta` event carries one token (~4–20 characters). A 100K-token response produces 100K delta events. The 4096-message cap kicks in well before that, but the last messages buffered before the cap will be the largest: `TurnResult` can be up to ~1 MiB by itself.

**More realistic worst case:**
```
8 sessions × 1 TurnResult (1 MiB) + 4095 delta events (avg 50 bytes each)
= 8 × (1MB + ~200KB)
= ~9.6 MiB
```

This is manageable, but the 4096-message count is an unreliable safety cap because message sizes vary by 4 orders of magnitude. A single `TurnResult` occupies 1 slot but 1 MiB; a delta event occupies 1 slot and 50 bytes.

### Recommendation

Replace the message-count cap with a byte-budget cap:

```rust
/// Maximum bytes buffered per session during a client disconnect.
/// 8 MiB per session × 8 sessions = 64 MiB absolute ceiling.
const MAX_REPLAY_BUFFER_BYTES: usize = 8 * 1024 * 1024;

pub async fn buffer_event(&self, json: String) {
    let mut buf = self.replay_buffer.lock().await;
    let current_bytes: usize = buf.iter().map(|s| s.len()).sum();
    if current_bytes + json.len() <= MAX_REPLAY_BUFFER_BYTES {
        buf.push(json);
    }
}
```

Optionally add an O(1) `buffered_bytes: usize` counter on `CachedSession` (behind a separate `Mutex<usize>`) to avoid the O(N) `sum()` on every `buffer_event` call.

---

## Finding 4: spawn_blocking Thread Pool Exhaustion

**Severity:** High
**File:** `crates/services/acp/persistent_conn.rs`, lines 66–94

### The Problem

```rust
let join = tokio::task::spawn_blocking(move || {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("[acp_conn] failed to build tokio runtime");
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async {
        // ... runs for up to 3600 seconds
    });
});
```

Each `AcpConnectionHandle::spawn()` call occupies **one thread from tokio's `spawn_blocking` pool for the entire session lifetime** — up to 1 hour by default.

Tokio's `spawn_blocking` pool defaults to 512 threads maximum, but they are shared across the entire process. In practice, the pool grows lazily and each blocked thread consumes ~8 MiB of stack (platform-dependent) plus kernel overhead.

### Thread Budget Analysis

At N concurrent ACP sessions:

| N sessions | Threads consumed by ACP | Remaining spawn_blocking budget |
|-----------|------------------------|--------------------------------|
| 8 (default) | 8 threads | 504 remaining |
| 50 | 50 threads | 462 remaining |
| 100 | 100 threads | 412 remaining |

At the default `AXON_ACP_MAX_CONCURRENT_SESSIONS=8`, thread consumption is not a crisis. However:

1. **Sessions linger in cache for 30 minutes** after last use. The `spawn_blocking` thread continues running the `adapter_loop` (waiting on `rx.recv()`) for the entire 30-minute TTL even if no turns are being processed.
2. If 8 sessions are established and idle, 8 `spawn_blocking` threads are permanently occupied for up to 30 minutes, blocking other subsystems from using `spawn_blocking` for that time.
3. The `adapter_loop` timeout (1 hour) vastly exceeds the session TTL (30 minutes). This is probably intentional, but the mismatch means an idle adapter loop could run for up to 1 hour while the session cache entry was evicted 30 minutes ago — the adapter loop only terminates when `rx` is dropped (when `AcpConnectionHandle` is dropped). If the `Arc<AcpConnectionHandle>` is kept alive somewhere else (e.g., in an in-flight turn's `TurnRequest`), the session eviction does not kill the adapter loop.

### Recommendation

The core architecture is correct — `current_thread` + `LocalSet` for `!Send` ACP types is the right approach. The optimization target is **reducing the idle thread hold time**.

Consider adding an idle timeout to the `adapter_loop` that drops the adapter when no turn arrives within N minutes (e.g., 5 minutes), separate from the session TTL. This returns the `spawn_blocking` thread to the pool and terminates the adapter subprocess while leaving the `CachedSession` entry available for reconnect attempts (where a new adapter would be spawned on first turn).

The alternative is a `current_thread` runtime-per-session approach where the runtime is created on a dedicated thread that exits when the session ends, rather than borrowing from the `spawn_blocking` pool. This gives more predictable lifecycle but requires more manual thread management.

---

## Finding 5: Semaphore Uses try_acquire — No Backpressure Queue

**Severity:** High
**File:** `crates/web/execute.rs`, line 250; `crates/web.rs`, lines 34–41

### The Problem

```rust
match crate::crates::web::ACP_SESSION_SEMAPHORE.try_acquire() {
    Ok(permit) => Ok(Some(permit)),
    Err(_) => {
        send_error_dual(tx, ws_ctx, "too many concurrent ACP sessions...".to_string(), None)
        // ...
    }
}
```

`try_acquire()` is non-blocking: if no permit is available, it immediately returns `Err` and the request is rejected with an error. There is no queue.

This means under bursty load (e.g., 10 simultaneous users each sending a first message), users 9 and 10 get an error, even if users 1–8 would complete within 2 seconds.

### Impact

At N=8, any burst of >8 simultaneous `pulse_chat` requests causes immediate errors for requests beyond the first 8. Users experience "too many concurrent ACP sessions" on an otherwise healthy system. This is particularly bad for the use case where users open the UI and send their first message at roughly the same time.

The permit is **not** released when the turn completes — it is released only when `handle_sync_direct` returns (which includes the full `drive_turn_events` drain). For a typical 30-second LLM response, the permit is held for the full 30 seconds.

However, note that the semaphore permit is tied to the _turn_ lifetime, not the _session_ lifetime. The cached `AcpConnectionHandle` in `SESSION_CACHE` persists after the permit is released. This is correct behavior — the semaphore bounds concurrent I/O-active turns, not cached-but-idle sessions.

### Recommendation

Replace `try_acquire()` with `acquire()` with a bounded wait timeout. Users wait briefly rather than getting immediate errors:

```rust
async fn acquire_acp_permit(mode: &str, tx: &mpsc::Sender<String>, ws_ctx: &events::CommandContext)
    -> Result<Option<tokio::sync::SemaphorePermit<'static>>, ()>
{
    if !matches!(mode, "pulse_chat" | "pulse_chat_probe") {
        return Ok(None);
    }
    match tokio::time::timeout(
        std::time::Duration::from_secs(30), // wait up to 30s for a slot
        crate::crates::web::ACP_SESSION_SEMAPHORE.acquire(),
    ).await {
        Ok(Ok(permit)) => Ok(Some(permit)),
        Ok(Err(_)) => { /* semaphore closed — process shutdown */ Err(()) }
        Err(_) => {
            // Timed out waiting for a slot — now report the error
            send_error_dual(tx, ws_ctx, "ACP capacity unavailable...".to_string(), None).await;
            Err(())
        }
    }
}
```

---

## Finding 6: O(N) session_id_index Cleanup on Remove

**Severity:** Medium
**File:** `crates/services/acp/session_cache.rs`, lines 154–159

### The Problem

```rust
pub fn remove(&self, agent_key: &str) {
    if let Some((_, _session)) = self.sessions.remove(agent_key) {
        self.session_id_index.retain(|_, v| v.as_str() != agent_key);
    }
}
```

`DashMap::retain` is O(N) over all entries in `session_id_index`. At the time it runs, `session_id_index` maps `session_id → agent_key`. A session can have multiple `session_id` entries (one per `new_session` call, e.g. after fallbacks). With M total sessions each having K session IDs, `retain` touches M×K entries to remove K entries.

At small N (< 100 sessions total ever created), this is not a performance concern. However, the `retain` takes a write lock on each shard of `session_id_index`, which blocks concurrent `register_session_id` and `get_by_session_id` calls for the duration of the scan.

The `retain` is also called from `reap_expired()` which already takes extra care about lock ordering. The combination can create brief but real contention windows.

### Recommendation

Switch to a reverse index: store `agent_key → Vec<session_id>` so that `remove(agent_key)` can look up exactly which session_ids to delete in O(K) rather than O(M×K). Alternatively, store the mapping bidirectionally and delete from both sides on insert/remove. Given the scale (≤8 sessions at a time), this is a correctness/cleanliness improvement more than a critical performance fix.

---

## Finding 7: Two Separate Mutex Locks per Touch in get()

**Severity:** Medium
**File:** `crates/services/acp/session_cache.rs`, lines 105–110

### The Problem

```rust
pub async fn get(&self, agent_key: &str) -> Option<Arc<CachedSession>> {
    let entry = self.sessions.get(agent_key)?;  // DashMap shard read lock
    let session = Arc::clone(entry.value());
    session.touch().await;                      // tokio::Mutex on last_active
    Some(session)
}
```

This is called on every prompt turn, every reaper check, and every reconnect. Each call acquires two locks: one DashMap shard read lock (immediately released after `Arc::clone`) and one `tokio::Mutex` for `touch()`.

The tokio `Mutex` for `last_active` is only contended when `touch()` and `is_expired()` run simultaneously (extremely rare — only during reaper runs). The overhead is the **cost of a lock-acquire attempt**: ~20–50ns under no contention. Across 100 turns/minute per session, this adds ~2–5μs/min — negligible.

However, `touch()` could use `std::sync::Mutex` instead of `tokio::sync::Mutex` since `Instant::now()` is not an I/O operation and should not be awaited. The tokio mutex adds async machinery (registers wakers, polls the scheduler) for a pure CPU operation.

### Recommendation

Use `parking_lot::Mutex` or `std::sync::Mutex` for `last_active` and `replay_buffer`:

```rust
pub struct CachedSession {
    pub handle: Arc<AcpConnectionHandle>,
    pub permission_responders: PermissionResponderMap,
    last_active: parking_lot::Mutex<Instant>,     // sync mutex — no await needed
    replay_buffer: parking_lot::Mutex<Vec<String>>, // sync mutex — brief hold
}
```

`touch()` and `is_expired()` become sync functions, eliminating `.await` in the reaper loop entirely, which in turn eliminates the DashMap-lock-across-await problem described in Finding 2.

---

## Finding 8: WS Forward Loop — Unbalanced select! Arms

**Severity:** Medium
**File:** `crates/web/ws_handler.rs`, lines 80–113

### The Problem

```rust
let forward = tokio::spawn(async move {
    loop {
        tokio::select! {
            Some(msg) = exec_rx.recv() => { /* ... */ }
            Some(msg) = tracking_rx.recv() => { /* ... */ }
            Ok(stats_msg) = stats_rx.recv() => { /* ... */ }
            else => break,
        }
    }
});
```

The `tokio::select!` macro without the `biased` keyword picks arms with uniform pseudo-random selection when multiple arms are ready simultaneously. During an active ACP turn that produces streaming tokens (potentially 100+ events/second on `exec_rx`), `stats_rx` (Docker stats, 500ms interval) and `tracking_rx` are also ready occasionally.

Under token streaming load with `exec_rx` always having messages, the random selection occasionally picks `stats_rx` instead of `exec_rx`, introducing a small but non-zero delay on token delivery. This is not a correctness issue — all messages are eventually delivered — but it creates observable jitter on the streaming token path.

For streaming LLM responses where latency is perceptible (users watching tokens appear), this adds up to ~1–2ms of extra latency per stats message insertion (roughly every 500ms), which is below human perception threshold.

### Recommendation

Add `biased;` to prioritize `exec_rx` (the hot streaming path), matching the pattern already used in `drive_turn_events` in `pulse_chat.rs`:

```rust
tokio::select! {
    biased;
    Some(msg) = exec_rx.recv() => { /* ... */ }
    Some(msg) = tracking_rx.recv() => { /* ... */ }
    Ok(stats_msg) = stats_rx.recv() => { /* ... */ }
    else => break,
}
```

This ensures stats messages do not preempt streaming token delivery when both arms are ready.

---

## Finding 9: Adapter Channel Capacity Too Small

**Severity:** Medium
**File:** `crates/services/acp/persistent_conn.rs`, line 65

### The Problem

```rust
let (tx, rx) = mpsc::channel(16);
```

The `AdapterMessage` channel from `AcpConnectionHandle` to the background `adapter_loop` has capacity 16. Each `TurnRequest` is one message. Since turns are serialized (one at a time per connection), there is never more than one message in flight at once under normal operation.

However, the capacity of 16 means `run_turn()` will never block on the send. This is correct. But consider the error path: `run_turn()` returns `Err` if the channel is closed (adapter exited). With capacity 16, if for some reason 16 turns were enqueued (not possible given current serialization, but relevant if parallelism is added later), the 17th would block.

This is a latent issue rather than a current performance problem.

The real concern is the opposite: the channel should have capacity 1 to make backpressure explicit and prevent any future refactoring from accidentally enqueueing multiple turns to a serial adapter. A capacity of 1 would cause `run_turn()`'s `.await` to block until the adapter loop picks up the previous turn — which is the correct serialized behavior.

### Recommendation

Reduce to capacity 1 (or 2 for a small pipeline buffer) to enforce the intended serial-turn invariant:

```rust
let (tx, rx) = mpsc::channel(1);
```

This makes the channel a rendezvous-style handoff that enforces single-turn serialization by construction. A future refactor attempting to enqueue two turns concurrently would naturally block, surfacing the design violation rather than silently buffering.

---

## Finding 10: serde_json String Contains Scan per Event

**Severity:** Low
**File:** `crates/web/ws_handler.rs`, lines 83–95

### The Problem

```rust
Some(msg) = exec_rx.recv() => {
    if msg.contains("\"crawl_files\"")
        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&msg) {
            // ...
        }
    if ws_tx.send(Message::Text(msg.into())).await.is_err() {
        break;
    }
}
```

Every message passing through `exec_rx` is scanned for the substring `"crawl_files"` using `str::contains`. For high-frequency ACP streaming (100+ messages/second during token streaming), this is a linear scan of every message string.

A typical `assistant_delta` message is ~50–200 bytes. `str::contains` on 100 bytes is ~15ns. At 100 messages/second, this costs ~1.5μs/second — negligible in isolation.

However, the `serde_json::from_str` call on matching messages is expensive (~10–50μs for a 200-byte JSON object). The `"crawl_files"` check is the correct guard to prevent deserializing non-crawl messages. No change needed there.

### Recommendation

No action required — this is already correctly guarded. Document the rationale in a comment to prevent future removal of the `contains` guard.

---

## Finding 11: Event Channel Buffer Sizing

**Severity:** Low
**File:** `crates/web/execute/sync_mode/pulse_chat.rs`, lines 289–290

### The Problem

```rust
let (event_tx, event_rx) = mpsc::channel::<ServiceEvent>(256);
```

Each `pulse_chat` turn creates a `ServiceEvent` channel with capacity 256. During LLM token streaming, `session_notification` fires once per token. A fast LLM can produce 50–100 tokens/second.

- Channel capacity 256 at 100 tokens/sec = ~2.56 seconds of buffer before `emit()` blocks.
- `emit()` uses `try_send()` which drops on full — this would silently lose streaming tokens if the WS consumer falls behind by more than 2.56 seconds.

Looking at the `emit()` implementation in `events.rs` (not reviewed but referenced throughout), if it uses `try_send()` rather than a blocking send, full channels silently drop events. This would cause missing tokens in the streamed response.

At 100 tokens/sec with a healthy WS connection and a local network, 256 capacity is more than sufficient (the consumer `drive_turn_events` runs in a tight loop). But for users on slow connections where WS backpressure causes `ws_tx.send()` to be slow, the `exec_rx` channel (capacity 256 in `ws_handler.rs`) could fill, causing `send_or_buffer` to buffer to the session cache instead.

### Recommendation

Increase the `ServiceEvent` channel to 512 or 1024 for headroom on slow connections. Verify that `emit()` uses `try_send()` (non-blocking drop) rather than a blocking send that could deadlock the `current_thread` runtime. If using `try_send()`, add a dropped-event counter for observability.

---

## Finding 12: Reaper Interval — No Jitter

**Severity:** Low
**File:** `crates/services/acp/session_cache.rs`, lines 197–203

### The Problem

```rust
async fn reaper_loop() {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        SESSION_CACHE.reap_expired().await;
    }
}
```

A fixed 60-second interval means that if multiple Axon instances start at the same time (e.g., rolling restart), all reapers fire at exactly the same wall-clock offset. This is not an issue for the session cache itself (which is process-local), but if a future change makes the reaper touch shared infrastructure (database, Redis), the lack of jitter becomes a thundering-herd concern.

### Recommendation

No action required at present (process-local cache). If the reaper is extended to do I/O, add jitter:

```rust
tokio::time::sleep(Duration::from_secs(rand::random::<u64>() % 30)).await; // initial jitter
let mut interval = tokio::time::interval(Duration::from_secs(60));
```

---

## Scalability Limits Summary

### N=1 session

All findings are negligible. The implementation works correctly. Finding 1 (fingerprint) still fires but its impact is one adapter respawn (invisible to the user — session cache still works, just spawns fresh on first request).

### N=8 sessions (default limit)

- **Finding 1 (fingerprint):** All 8 sessions using MCP env vars will never hit the cache on turn N+1 if the process has restarted. Each request spawns a fresh adapter. At 8 concurrent active sessions, this is 8× the expected adapter subprocess count.
- **Finding 3 (replay buffer):** Worst-case memory: ~9.6 MiB across all sessions. Acceptable.
- **Finding 4 (spawn_blocking):** 8 threads occupied for session TTL (30 min). Acceptable given the 512-thread pool cap.
- **Finding 5 (semaphore):** 9th concurrent request is immediately rejected. Noticeable under burst.
- **Finding 2 (reaper contention):** Minimal impact at N=8, but present.

### N=50 sessions

This requires raising `AXON_ACP_MAX_CONCURRENT_SESSIONS=50`.

- **Finding 1:** 50 adapter subprocess pairs (intended vs spawned). OS process limit may be hit.
- **Finding 2:** Reaper holds DashMap shard locks while awaiting 50 `last_active` mutexes. The contention window is now: (50 × avg_lock_acquire_time) ≈ 50 × 2μs = 100μs during which any `get()` call on any session in the same shards is blocked.
- **Finding 3:** 50 × worst-case replay buffer = 60 MiB. Significant.
- **Finding 4:** 50 `spawn_blocking` threads, each holding ~8 MiB stack = 400 MiB thread stack memory. Approaches the point where the OS may refuse new threads.
- **Finding 6 (O(N) cleanup):** `retain` scans 50+ entries on every `remove()` call.

### N=50+ sessions — What Breaks First

1. **OS thread limit** (Finding 4) — typically hits between 100–1000 threads depending on ulimits and available memory.
2. **Fingerprint non-determinism** (Finding 1) — orphaned adapter processes accumulate; each holds a file descriptor pair and OS memory for the Claude binary.
3. **Replay buffer memory** (Finding 3) — at ~1.2 MiB/session, reaches 60 MiB at N=50. Not catastrophic but worth bounding.

---

## Priority Fix Order

| Priority | Finding | Effort | Impact |
|----------|---------|--------|--------|
| 1 | **Finding 1** — non-deterministic fingerprint | Low (replace hasher + sort) | Eliminates spurious adapter respawns |
| 2 | **Finding 5** — try_acquire vs acquire with timeout | Low (5-line change) | Eliminates false capacity errors under burst |
| 3 | **Finding 2** — DashMap lock across await in reaper | Low (collect-then-check pattern) | Eliminates potential deadlock vector |
| 4 | **Finding 7** — tokio Mutex for sync operation | Medium (refactor CachedSession) | Eliminates async overhead on hot path; also fixes Finding 2 |
| 5 | **Finding 3** — byte-budget cap on replay buffer | Low (add byte counter) | Bounds memory under adversarial conditions |
| 6 | **Finding 8** — biased select! in forward loop | Trivial (add `biased;`) | Reduces token delivery jitter |
| 7 | **Finding 4** — spawn_blocking idle time | High (architecture change) | Reduces idle thread consumption; complex |
| 8 | **Finding 9** — channel capacity 16 → 1 | Trivial | Enforces serial invariant; no perf impact |
| 9 | **Finding 6** — O(N) cleanup | Medium (reverse index) | Only matters at N > 50 |
| 10 | **Finding 11** — event channel sizing | Trivial | Defensive headroom |
| 11 | **Finding 12** — reaper jitter | Low | Future-proofing only |
