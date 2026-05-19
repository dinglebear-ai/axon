# Performance & Scalability Analysis: `crates/web` WebSocket Execution Bridge

**Date:** 2026-03-15
**Analyst:** Performance Engineering Review
**Scope:** Full read of all 24 source files in `crates/web`

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Memory Management](#2-memory-management)
3. [Async Performance](#3-async-performance)
4. [Concurrency & Synchronization](#4-concurrency--synchronization)
5. [I/O Patterns](#5-io-patterns)
6. [Docker Stats Pipeline](#6-docker-stats-pipeline)
7. [Channel Backpressure](#7-channel-backpressure)
8. [Resource Limits](#8-resource-limits)
9. [Hot Path Optimizations](#9-hot-path-optimizations)
10. [Subprocess Execution](#10-subprocess-execution)
11. [ZIP / Archive Assembly](#11-zip--archive-assembly)
12. [Summary Table](#12-summary-table)

---

## 1. Executive Summary

The `crates/web` bridge is generally well-engineered. Most of the obvious DoS vectors from Phase 1 context are now patched: connection limits, shell frame size cap, ACP semaphore, and rate limiting are all present. The remaining findings are mostly in the medium-to-low range — no critical, one high. The codebase will handle its expected single-user homelab workload without issue; findings become meaningful under multi-user load or when edge-case inputs are sent.

---

## 2. Memory Management

### 2.1 `send_or_sentinel` — Clone on Every Non-Terminal Message
**Severity:** Medium
**Impact:** Minor to moderate on high-throughput crawl output
**File:** `crates/web/execute/ws_send.rs:17`

`send_or_sentinel` receives `msg: String` by value and immediately clones it before passing to `try_send`:

```rust
fn send_or_sentinel(tx: &mpsc::Sender<String>, msg: String) {
    match tx.try_send(msg.clone()) {  // ← clone here
```

Every non-terminal WS event (command start, output lines, log lines from subprocesses) takes this path. On a verbose crawl emitting thousands of output lines, this is one extra `String` heap allocation per message that is immediately dropped on the `Ok(())` arm. The clone only serves the `Full` arm, which is the rare path.

**Recommendation:** Pass ownership to `try_send` directly. Reconstruct the original for the sentinel path using the error's inner value:

```rust
fn send_or_sentinel(tx: &mpsc::Sender<String>, msg: String) {
    match tx.try_send(msg) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_dropped)) => {
            // msg was moved into the error; we no longer have it.
            // Send the sentinel instead.
            let sentinel = …;
            let _ = tx.try_send(sentinel);
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {}
    }
}
```

`TrySendError::Full(T)` holds the moved value, so the original message could even be re-attempted or logged without an extra clone.

---

### 2.2 `output_dir()` — Env Var Reads on Every Call
**Severity:** Medium
**Impact:** Minor (syscall overhead, not alloc)
**File:** `crates/web/execute/files.rs:38-51`

`output_dir()` calls `std::env::var("AXON_OUTPUT_DIR")` and `std::env::var("AXON_DATA_DIR")` on every invocation. It is called in:
- `serve_output_file` (every file-serve request)
- `send_scrape_file` (after every scrape command)
- `send_crawl_manifest` (dead code currently, but still compiled)

`std::env::var` on Linux requires a lock on the environment (`libc::getenv` is not thread-safe, so glibc uses a lock). Under concurrent scrape/serve load this adds contention on a process-global lock.

**Recommendation:** Cache in a `std::sync::OnceLock<PathBuf>` or `LazyLock<PathBuf>` at module level — identical to the pattern already used in `params.rs` for `ACP_ADAPTER_CMD`:

```rust
static OUTPUT_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    std::env::var("AXON_OUTPUT_DIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| …)
        .unwrap_or_else(|| PathBuf::from(".cache/axon-rust/output"))
});

pub fn output_dir() -> &'static Path {
    &OUTPUT_DIR
}
```

---

### 2.3 `load_all_files` — All File Contents in Memory Simultaneously
**Severity:** Medium
**Impact:** Significant for large crawls (500 MB limit, all held at once)
**File:** `crates/web/download/manifest.rs:58-116`

`load_all_files` reads every crawl file into a `Vec<(String, String, String)>` — all in memory at the same time — before passing the entire collection to `build_pack_md` / `build_pack_xml` / `build_zip`. Each of those functions then builds another full `String` buffer (pack.rs uses pre-sized capacity estimation, but the total peak allocation is `2× total_content_bytes`).

For a crawl at the 500 MB `AXON_DOWNLOAD_MAX_BYTES` limit: peak RSS from a single download request could reach ~1 GB (all content + assembled output).

**Recommendation:** For the ZIP path, stream files directly into the `ZipWriter` rather than loading all content first. For the pack formats, write incrementally to a `tokio::io::BufWriter` backed by an HTTP body stream (using axum's `StreamBody`). This reduces peak to O(single_file_size) for ZIP, or O(chunk_size) for streaming pack.

The byte pre-check in `load_all_files` (lines 68–88) is correct and necessary, but it does not reduce memory once the loading begins.

---

### 2.4 `track_crawl_files` — Double Deserialization on Crawl Output Messages
**Severity:** Low
**Impact:** Negligible for homelab; minor on high-volume crawl
**File:** `crates/web/ws_handler.rs:208-232`

Every message passing through the forward task is first partially deserialized as `MsgType` (for the `"type"` field), and if it matches `"crawl_files"`, deserialized again as a full `serde_json::Value`. The first deserialization is cheap (struct with one field), but the second is a full parse of the entire message again. This runs on every message through the forward loop.

**Recommendation:** A single deserialization directly to `serde_json::Value` on the `"crawl_files"` path avoids the second parse. Since `crawl_files` messages are rare (emitted once per crawl job), the practical impact is negligible — but the pattern is slightly inconsistent with stated concern about avoiding extra work on the hot path.

---

### 2.5 `handle_execute_msg` — `Arc<Mutex<Option<String>>>` per Execute Task
**Severity:** Low
**Impact:** Negligible (small allocation per command)
**File:** `crates/web/ws_handler.rs:330`

Each `execute` message allocates a new `Arc<Mutex<Option<String>>>` as `per_task_job_id`. This is correct for its purpose (allowing the spawned task to write a job ID back), but given commands are user-initiated and infrequent (rate-limited to 120/min), the overhead is immaterial. Noted for completeness.

---

## 3. Async Performance

### 3.1 `call_evaluate` — Raw OS Thread + New Tokio Runtime per Invocation
**Severity:** High
**Impact:** Significant — up to 8 MiB stack + full runtime boot per call
**File:** `crates/web/execute/sync_mode/service_calls.rs:268-296`

`call_evaluate` works around a `!Send` constraint in `query_svc::evaluate` by spawning a raw OS thread, building a new `tokio::runtime::Builder::new_current_thread()` runtime, creating a `LocalSet`, and running the evaluation on it. The one-shot channel bridges the result back.

Cost per invocation:
- OS thread creation (typically 8 MiB default stack on Linux)
- New Tokio runtime construction (`enable_all()` sets up I/O driver, timer wheel, signal handlers)
- `LocalSet` bookkeeping
- Thread join on return

Under the ACP semaphore of 8, `call_evaluate` is NOT in `ACP_MODES` and is therefore not bounded by the semaphore. The only limit is the `check_rate_limit` (120 executes/min/IP). With 100 WS connections each sending `evaluate` at rate limit, this could spawn up to 12,000 threads/minute.

**Recommendation (short term):** Add `"evaluate"` to `ACP_MODES` to bound it with the ACP semaphore, or create a dedicated `EVALUATE_SEMAPHORE` with a small limit (e.g., 4).

**Recommendation (long term):** Fix the `!Send` constraint in `query_svc::evaluate` so it can be awaited directly in the Tokio runtime. If that is not feasible, use a `once_cell`-cached dedicated single-threaded runtime (created once at startup) and dispatch evaluate calls to it via a channel — eliminating the per-call runtime construction cost:

```rust
static EVALUATE_RT: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("evaluate runtime")
});
```

---

### 3.2 `dispatch_subprocess_fallback` — `spawn_blocking(resolve_exe)` on Every Invocation
**Severity:** Low
**Impact:** Minor overhead for subprocess-fallback modes
**File:** `crates/web/execute.rs:299-311`

`resolve_exe` is correctly moved to `spawn_blocking` (P2-3 comment acknowledges this). However, the result is not cached — every subprocess invocation rediscovers the binary path. `resolve_exe` calls `Path::exists()` on multiple candidates, which is 1–3 blocking filesystem `stat` calls per execution. For the infrequent modes that use the subprocess fallback (suggest, screenshot, evaluate, sessions, dedupe, debug, refresh), this is acceptable.

**Recommendation:** Cache the resolved path in a `OnceLock<PathBuf>`. Startup path discovery runs once; subsequent calls read from the lock:

```rust
static RESOLVED_EXE: OnceLock<PathBuf> = OnceLock::new();
```

This eliminates the `spawn_blocking` for all but the first call.

---

### 3.3 `run_forward_task` — Mutex Lock Inside `sender_task` in `shell.rs`
**Severity:** Medium
**Impact:** Minor; every PTY output chunk acquires a `tokio::sync::Mutex`
**File:** `crates/web/shell.rs:85-92`

```rust
let sender_task = tokio::spawn(async move {
    while let Some(msg) = pty_out_rx.recv().await {
        let mut tx = ws_tx_clone.lock().await;  // ← lock on every PTY chunk
        if tx.send(Message::Text(msg.into())).await.is_err() {
            break;
        }
    }
});
```

`ws_tx` is wrapped in `Arc<Mutex<_>>` so the ping sender in the timeout arm can also write. But the sender task holds the only receiver; the timeout arm only writes on timeout (every 30s). This is an unconditional `Mutex` acquisition on every PTY output byte chunk (4096 bytes at a time).

**Recommendation:** Use a `tokio::sync::mpsc` channel between the sender task and the ping timer, eliminating the `Mutex`. The sender task `select!`s between `pty_out_rx` and a `ping_rx`:

```rust
let (ping_tx, mut ping_rx) = mpsc::channel::<()>(1);
// sender_task: select! { msg = pty_out_rx => ..., _ = ping_rx => ws_tx.send(Ping) }
// timeout arm: let _ = ping_tx.try_send(());
```

This keeps the WS sink single-owner (no mutex needed) and ping delivery is still best-effort.

---

### 3.4 `handle_acp_resume` — Sequential `tx.send` per Replay Message
**Severity:** Low
**Impact:** Negligible in practice; minor at large replay buffers
**File:** `crates/web/ws_handler.rs:455-459`

```rust
for msg in buffered {
    let _ = tx.send(msg).await;
}
```

With `MAX_REPLAY_BUFFER = 4096` messages, replaying a full buffer requires 4096 sequential `.await`s. Each `send` on a bounded channel (`capacity=256`) can block if the consumer is slow. For the typical case (small replay buffer, fast consumer), this is fine. For a 4096-message replay on a slow WS connection, this serializes all sends through one coroutine. The forward task will drain as fast as the WS sink allows, so this is naturally bounded — noted for awareness.

---

## 4. Concurrency & Synchronization

### 4.1 `MCP_SERVER_CACHE` — `std::sync::Mutex` May Block Async Task
**Severity:** Medium
**Impact:** Minor; blocks async task thread while holding cache lock
**File:** `crates/web/execute/mcp_config.rs:20-21, 36-58`

`MCP_SERVER_CACHE` uses a `std::sync::Mutex<Option<McpServerCache>>`. The lock is correctly dropped before the async `fetch_axon_mcp_servers_from_disk` call (line 45). However, on the cache-miss path, the lock is acquired twice (lines 38 and 52), and between the two acquisitions the code does async I/O. This pattern is correct in that the lock is never held across an `.await`, but:

1. On a cache miss, two separate lock acquisitions with a gap creates a TOCTOU window: two concurrent callers can both observe a miss, both fetch from disk, and both write to the cache — the second write clobbers the first (which is idempotent here, so functionally harmless but wasteful).
2. Using `std::sync::Mutex` on an async task thread is correct only when held for nanoseconds. The lock is indeed held briefly here, so this is fine in practice.

**Recommendation:** Use a `tokio::sync::RwLock` to allow concurrent readers during cache-hit (no lock contention for the common read path), and handle the TOCTOU by re-checking after re-acquiring for write:

```rust
static MCP_SERVER_CACHE: LazyLock<RwLock<Option<McpServerCache>>> =
    LazyLock::new(|| RwLock::new(None));
// read lock for hit check, write lock for update only
```

---

### 4.2 `session_ownership` DashMap — O(N) Retain on Disconnect
**Severity:** Low
**Impact:** Negligible for typical session counts; noted for scale
**File:** `crates/web/ws_handler.rs:156-158`

```rust
conn.session_ownership
    .retain(|_, owner| owner.as_str() != cid.as_str());
```

On every WS disconnect, the entire `session_ownership` map is scanned. With 100 concurrent connections each owning multiple sessions, this is an O(N_sessions) scan on every disconnect. At typical homelab usage (< 10 sessions), this is completely negligible.

**Recommendation:** No action needed for current scale. If multi-user deployment is expected, a reverse index `conn_id → Vec<session_id>` would allow O(N_owned) cleanup.

---

### 4.3 `rate_limiter` DashMap — Contended `entry()` on Every Execute/ReadFile Message
**Severity:** Low
**Impact:** Negligible at homelab scale; notable at high connection count
**File:** `crates/web/ws_handler/rate_limiter.rs:47`

Every `execute` and `read_file` message acquires a DashMap shard lock via `entry(ip)`. DashMap shards by hash, so this contention is spread across N shards (default 16 on DashMap). At 100 concurrent users, the effective contention per shard is low. The amortized eviction (compare-exchange at most once per window) is well-designed.

No action needed.

---

### 4.4 `fingerprint_mcp_servers` — SHA-256 Computed on Every `pulse_chat` Turn
**Severity:** Low
**Impact:** Minor CPU cost on every ACP turn
**File:** `crates/web/execute/sync_mode/pulse_chat.rs:241-248`

```rust
fn fingerprint_mcp_servers(mcp_servers: &[AcpMcpServerConfig]) -> String {
    let json = serde_json::to_string(mcp_servers).unwrap_or_default();
    let hash = Sha256::digest(json.as_bytes());
    format!("{hash:x}")
}
```

This runs on every `pulse_chat` invocation before the cache lookup. For a typical session (2–5 MCP servers), the JSON serialization and SHA-256 are cheap (~microseconds). The `caps_fingerprint` string below it uses `format!` with debug output. Both are negligible at homelab scale.

No action needed unless MCP server lists become large (>100 entries).

---

## 5. I/O Patterns

### 5.1 `serve_output_file` — Full File Read into Memory for All Served Files
**Severity:** Medium
**Impact:** Significant for large files (screenshots, HTML output)
**File:** `crates/web.rs:273-276`

```rust
let bytes = match tokio::fs::read(&canonical_file).await {
    Ok(b) => b,
    …
};
(resp_headers, bytes).into_response()
```

`tokio::fs::read` reads the entire file into a `Vec<u8>` before sending. For markdown files, this is fine. For PNG screenshots or large HTML files that may be hundreds of MB, this holds the entire content in heap before axum begins writing the HTTP response.

**Recommendation:** Use `tokio::fs::File::open` + axum's `tokio_util::io::ReaderStream` to stream the file body incrementally:

```rust
let file = tokio::fs::File::open(&canonical_file).await?;
let stream = tokio_util::io::ReaderStream::new(file);
let body = axum::body::Body::from_stream(stream);
(resp_headers, body).into_response()
```

This caps peak memory at the OS read buffer size (~64 KB) regardless of file size.

---

### 5.2 `load_all_files` — N Sequential `canonicalize` + `metadata` + `read_to_string` Calls
**Severity:** Medium
**Impact:** Moderate for large crawls with many files; sequential I/O
**File:** `crates/web/download/manifest.rs:70-108`

The pre-check loop (size totalling) and the loading loop are both sequential. For a 2000-file crawl:
- 2000 × `fs::canonicalize` calls
- 2000 × `fs::metadata` calls
- 2000 × `fs::read_to_string` calls

All are async but executed one after the other. On a local SSD this completes quickly, but on network filesystems or slow storage it serializes what could be parallelized.

**Recommendation:** Use `tokio::task::JoinSet` or `futures::stream::iter(...).buffer_unordered(N)` to execute file reads in parallel:

```rust
let mut join_set = tokio::task::JoinSet::new();
for (url, rel_path, canonical_file) in resolved_entries {
    join_set.spawn(async move {
        tokio::fs::read_to_string(&canonical_file).await
            .map(|content| (url, rel_path, content))
    });
}
```

Limit concurrency to 32–64 to avoid EMFILE errors.

---

### 5.3 `handle_read_file` — Double `canonicalize` on Every Read
**Severity:** Low
**Impact:** Minor; two `stat`/`realpath` syscalls per `read_file` message
**File:** `crates/web/execute/files.rs:272-283`

`handle_read_file` calls `tokio::fs::canonicalize(base_dir)` on every invocation. The base directory does not change between calls. This adds one `realpath` syscall on every `read_file` WS message.

**Recommendation:** Canonicalize `base_dir` once when `crawl_base_dir` is first set (in `track_crawl_files`) and store the canonical path. Then `handle_read_file` only needs to canonicalize the target file.

---

## 6. Docker Stats Pipeline

### 6.1 Stats Broadcast Even When No Client Has Subscribed
**Severity:** Low
**Impact:** Wasted JSON serialization + broadcast when subscribers = 0
**File:** `crates/web/docker_stats.rs:106-109` and `crates/web/ws_handler.rs:193-199`

`build_stats_message` is called and `tx.send(message)` is executed on every 500ms tick regardless of whether any WS client has subscribed. The `broadcast::Sender::send` returns `Err(SendError)` when there are no receivers, but the allocation and JSON serialization still happens before the send attempt.

The opt-in `M-12` flag (`stats_subscribed`) only gates the _forwarding_ inside `run_forward_task`. The `run_stats_loop` still builds and broadcasts unconditionally.

**Recommendation:** Skip `build_stats_message` when `tx.receiver_count() == 0`:

```rust
if !latest_metrics.is_empty() && tx.receiver_count() > 0 {
    let message = build_stats_message(&latest_metrics);
    let _ = tx.send(message);
}
```

`broadcast::Sender::receiver_count()` is an atomic load — negligible overhead.

---

### 6.2 Docker Container List Polled Every 500ms
**Severity:** Low
**Impact:** Minor; 2 Docker API calls/second whether containers change or not
**File:** `crates/web/docker_stats.rs:43-62`

The stats loop calls `docker.list_containers(...)` every 500ms to detect new/stopped containers. Docker API calls go over a Unix socket and are cheap but not free. The stream management logic (spawning/aborting per-container tasks) runs on every tick.

**Recommendation:** Decouple container discovery from stats broadcasting. Poll `list_containers` at a slower interval (e.g., 5s) for container lifecycle events, while the per-container `stream_container_stats` tasks push metrics at native Docker streaming frequency (~1s). This reduces Docker API overhead by 10×.

---

### 6.3 `build_stats_message` — `serde_json::Map` Insertion by Name Clone
**Severity:** Low
**Impact:** Negligible
**File:** `crates/web/docker_stats.rs:303-332`

```rust
containers.insert(name.clone(), json!({…}));
```

`name` is cloned from `HashMap` iteration. Since `name` is also used for aggregation totals, it cannot be moved here. The clone is unavoidable with the current structure.

No action needed.

---

### 6.4 Per-Container Streaming Task: `name.clone()` on Every Stats Frame
**Severity:** Low
**Impact:** Negligible
**File:** `crates/web/docker_stats.rs:158`

```rust
let metric = ContainerMetrics {
    name: name.clone(),
    …
};
```

Inside `stream_container_stats`, `name` is cloned into every `ContainerMetrics` struct. With 5 containers at Docker's ~1s streaming rate, this is 5 clones/second of a short string. Negligible.

**Recommendation (optional):** Replace `name: String` with `name: Arc<str>` in `ContainerMetrics` to make the clone a reference-count bump instead of a heap allocation.

---

## 7. Channel Backpressure

### 7.1 `exec_tx` and `tracking_tx` Capacity of 256 — May Drop Output Under Load
**Severity:** Medium
**Impact:** Visible to user: output truncation sentinel shown in browser
**File:** `crates/web/ws_handler.rs:99-100`

```rust
let (exec_tx, exec_rx) = mpsc::channel::<String>(256);
let (tracking_tx, tracking_rx) = mpsc::channel::<String>(256);
```

The `send_or_sentinel` fast path drops non-terminal output when the channel fills. With 256-slot capacity and the forward task also consuming from two other receivers (`tracking_rx`, `stats_rx`) in a biased select, a high-volume crawl emitting thousands of output lines per second can fill the 256-slot buffer and cause visible "output truncated" messages in the browser.

The biased select correctly prioritizes `exec_rx` over stats, but under extreme subprocess output volume, the 256-cap means approximately 256 × (typical JSON message size ~200 bytes) = ~50 KB of in-flight output before backpressure kicks in.

**Recommendation:** Increase `exec_tx` capacity to 1024 or more. The memory cost is bounded by `capacity × average_message_size` which is ~200 KB at 1024 capacity — acceptable. Alternatively, use an unbounded channel for `exec_rx` since terminal events use `send_reliable` (guaranteed delivery) and non-terminal events already have a sentinel fallback. An unbounded channel removes the truncation risk at the cost of unbounded memory growth during pathological output — add an explicit high-water-mark check if unbounded is used.

---

### 7.2 ACP Event Channel Capacity of 256
**Severity:** Low
**Impact:** Minor; events are typically low-volume
**File:** `crates/web/execute/sync_mode/pulse_chat.rs:336`

```rust
let (event_tx, event_rx) = mpsc::channel::<ServiceEvent>(256);
```

ACP events are `ServiceEvent` (log lines, bridge events, editor writes). For a long LLM turn emitting many assistant delta tokens, 256 slots could fill. Unlike the subprocess path, there is no sentinel here — if the event channel fills, the ACP service layer's `service_tx.send()` would block at the backpressure point inside `drive_turn_events`. This is not a data-loss risk (blocking vs dropping), but it could cause the event loop to stall.

**Recommendation:** Increase to 1024. Memory cost is bounded by `1024 × sizeof(ServiceEvent)`.

---

## 8. Resource Limits

### 8.1 No WS Frame Size Limit on Main `/ws` Endpoint
**Severity:** Medium (confirmed from Phase 1 context)
**Impact:** Moderate DoS vector — large frames allocated before dispatch
**File:** `crates/web.rs:363-370`

The `/ws/shell` endpoint applies `ws.max_message_size(MAX_SHELL_WS_MSG)` (64 KiB). The main `/ws` endpoint at line 363 does not set a frame size limit:

```rust
ws.on_upgrade(move |socket| {
    let _guard = pre_guard;
    async move {
        ws_handler::handle_ws(socket, state, conn_id, client_ip).await;
```

An authenticated client (or one connecting before the rate limiter fires) can send a frame of arbitrary size. axum's default frame limit is typically 64 MiB. A client could send 100 × 64 MiB frames = 6.4 GB of allocations before the rate limiter has a chance to reject. (Rate limiting only applies after successful WS dispatch and JSON parsing.)

**Recommendation:** Apply the same frame size cap as the shell endpoint:

```rust
const MAX_WS_MSG: usize = 1_048_576; // 1 MiB — generous for any legitimate command
ws.max_message_size(MAX_WS_MSG).on_upgrade(move |socket| {
```

The largest legitimate payload is a `pulse_chat` message with a long system prompt. 1 MiB is more than sufficient for any command input.

---

### 8.2 Shell PTY — No Idle Timeout
**Severity:** Medium (confirmed from Phase 1 context)
**Impact:** Up to 10 shell connections × idle PTY processes = file descriptor drain
**File:** `crates/web/shell.rs:96-143`

The shell WS loop has a 30-second no-message keepalive mechanism that sends pings and breaks after 2 unacknowledged pings (total ~60s). However, this timeout resets on **any message received** (line 99: `unacked_pings = 0`). A client sending a single byte every 55 seconds can hold a PTY open indefinitely.

With `AXON_MAX_SHELL_CONNECTIONS = 10`, 10 idle-but-active shell sessions each hold:
- 1 PTY pair (2 file descriptors)
- 3 spawned tasks (reader, writer, sender)
- 1 shell subprocess (`/bin/bash`)

At 10 connections: 20 PTY fds + 10 bash processes. This is the intended limit, but the PTY processes themselves have no max-lifetime cap.

**Recommendation:** Add an absolute maximum session lifetime (e.g., 1 hour), independent of keepalive:

```rust
const MAX_SHELL_SESSION_SECS: u64 = 3600;
let session_start = tokio::time::Instant::now();
// In the outer loop, check:
if session_start.elapsed() > Duration::from_secs(MAX_SHELL_SESSION_SECS) {
    break;
}
```

---

### 8.3 `crawl_job_ids` Vec — Unbounded Growth per Connection
**Severity:** Low
**Impact:** Minor; each job ID is a UUID string (~36 bytes)
**File:** `crates/web/ws_handler.rs:103, 337-339`

```rust
job_ids_vec.lock().await.push(id);
```

Every async job enqueued on a connection appends a UUID to `crawl_job_ids`. There is no trim or cap on this Vec. A user running 120 crawls/minute (at rate limit) for 60 minutes accumulates 7,200 strings before disconnect. At ~36 bytes each, that's ~260 KB — immaterial but growing.

**Recommendation:** Cap at a reasonable limit (e.g., 1000) and evict the oldest entries, or clear the Vec after a successful implicit cancel-all. Not urgent.

---

## 9. Hot Path Optimizations

### 9.1 Flag Validation — `HashSet` Allocation on Every Execute Message
**Severity:** Medium
**Impact:** Minor; one allocation + N insertions per execute message
**File:** `crates/web/execute.rs:177-192`

```rust
let allowed_keys: std::collections::HashSet<&str> =
    ALLOWED_FLAGS.iter().map(|(k, _)| *k).collect();
```

This creates a fresh `HashSet` from `ALLOWED_FLAGS` on every `execute` message. `ALLOWED_FLAGS` has 43 entries. The set is used only for `obj.keys().filter(...)` and then dropped.

**Recommendation:** Use a `LazyLock<HashSet<&'static str>>` to build the set once at startup:

```rust
static ALLOWED_FLAG_KEYS: LazyLock<std::collections::HashSet<&'static str>> =
    LazyLock::new(|| ALLOWED_FLAGS.iter().map(|(k, _)| *k).collect());
```

Then `handle_command` references `&*ALLOWED_FLAG_KEYS`. This eliminates 43 hash insertions per execute message.

---

### 9.2 `check_rate_limit` — Both `Instant::now()` and `SystemTime::now()` on Every Call
**Severity:** Low
**Impact:** Minor; two time syscalls per execute/read_file message
**File:** `crates/web/ws_handler/rate_limiter.rs:45-76`

`check_rate_limit` calls `Instant::now()` for the sliding window and `SystemTime::now()` for the eviction timestamp. These are separate syscalls. On Linux with `VDSO`, `clock_gettime` is a userspace operation (~1 ns), so the overhead is negligible. Worth noting for completeness.

No action needed.

---

### 9.3 CORS Middleware — `web_cors_middleware` on Every Request Including WS
**Severity:** Low
**Impact:** Negligible for homelab; present on all routes
**File:** `crates/web.rs:180-183`

The CORS middleware (`web_cors_middleware`) wraps all routes including `/ws`, `/output/*`, and download routes. CORS headers on WS upgrades are evaluated but not applied (browsers don't enforce CORS on WS). The middleware overhead per request is O(N_allowed_origins) header comparisons. With 1–3 configured origins, this is a few string comparisons per request. Negligible at any realistic load.

No action needed.

---

## 10. Subprocess Execution

### 10.1 `handle_sync_command` — Two `tokio::spawn` Tasks per Subprocess
**Severity:** Low
**Impact:** Minor; two task allocations per subprocess invocation
**File:** `crates/web/execute/sync_mode/subprocess.rs:162-163`

```rust
let stdout_task = tokio::spawn(read_stdout(…));
let stderr_task = tokio::spawn(read_stderr(…));
let (stdout_result, _) = tokio::join!(stdout_task, stderr_task);
```

Spawning two tasks for stdout/stderr reading is appropriate here since both must be read concurrently to avoid pipe buffer deadlocks (a classic POSIX subprocess issue). The spawn overhead (one allocation per task) is a one-time cost per command invocation. Correct design.

---

### 10.2 `read_stderr` — `json!` Macro on Every Stderr Line
**Severity:** Low
**Impact:** Minor; one `serde_json::Value` allocation per stderr line
**File:** `crates/web/execute/sync_mode/subprocess.rs:97-100`

```rust
if tx
    .send(json!({"type": "log", "line": clean}).to_string())
    .await
    .is_err()
```

`json!` constructs a `serde_json::Value` dynamically, then immediately serializes it to `String`. For a verbose subprocess emitting many stderr lines, this allocates a `Value` tree just to immediately throw it away.

**Recommendation:** Use `format!` directly:

```rust
let msg = format!(r#"{{"type":"log","line":{}}}"#,
    serde_json::to_string(&clean).unwrap_or_default());
```

Or use a pre-built format string with a single `serde_json::to_string` call for the escaped line value. This avoids the intermediate `Value` allocation.

---

### 10.3 `ACP Probe` (`handle_pulse_chat_probe`) — Spawns New Adapter on Every Call
**Severity:** Low
**Impact:** One subprocess spawn per probe invocation
**File:** `crates/web/execute/sync_mode/pulse_chat.rs:382-416`

`handle_pulse_chat_probe` always spawns a fresh adapter subprocess (no session cache reuse) and does NOT register the result in `SESSION_CACHE`. This is intentional — probes are diagnostic and stateless. However, the `tokio::spawn` wrapping `scaffold.start_session_probe` (line 407) means a probe that fails during the session setup phase may not cleanly abort the spawned adapter task. The `run_acp_event_loop` eventually awaits the task, so cleanup is correct in the happy path.

No action needed.

---

## 11. ZIP / Archive Assembly

### 11.1 `build_zip` — Full Content in Memory Before Writing
**Severity:** Medium
**Impact:** Peak RSS = all file content + ZIP-compressed output
**File:** `crates/web/download/archive.rs:28-49`

```rust
let buf = Vec::with_capacity(entries.iter().map(|(_, _, c)| c.len()).sum::<usize>());
let cursor = std::io::Cursor::new(buf);
let mut zip = zip::ZipWriter::new(cursor);
```

The initial capacity estimation sums uncompressed content. Compressed output (Deflated) will be smaller, so the allocation overshoots. However, all file content must already be in memory (passed as `&[(String, String, String)]`), so the ZIP writer is only one additional buffer on top of the already-loaded content.

The `zip::ZipWriter::new(cursor)` writes into an in-memory `Vec<u8>`. For a 500 MB download limit, peak allocation is:
- `loaded_content`: up to 500 MB (held in the `Vec` from `load_all_files`)
- `zip_buf`: up to 500 MB (usually less after Deflate compression)
- Total peak: up to ~1 GB

`spawn_blocking` in `serve_zip` (line 156) correctly moves the CPU-intensive Deflate compression off the async thread.

**Recommendation (medium term):** Implement streaming ZIP assembly using `async-zip` crate, piping output directly into the HTTP response body stream. This eliminates the ZIP buffer entirely and allows the download to begin while compression is still in progress.

---

### 11.2 `build_pack_md` / `build_pack_xml` — Single Large String Buffer
**Severity:** Low
**Impact:** Same as 11.1 — content in memory twice (source + output)
**File:** `crates/web/pack.rs:10-31, 37-61`

Both functions pre-calculate capacity estimates and build a single `String`:
```rust
let mut out = String::with_capacity(entries.iter().map(|(_, _, c)| c.len() + 120).sum());
```

The capacity estimate is good (avoids reallocations). The functions are run synchronously on the async thread (`serve_pack_md` and `serve_pack_xml` do not use `spawn_blocking`). For a 500 MB pack, this is significant CPU and allocator time on the Tokio runtime thread.

**Recommendation:** Wrap pack assembly in `spawn_blocking` the same way `serve_zip` does for ZIP assembly. The pattern is already established in the same file.

---

## 12. Summary Table

| # | Description | Severity | Impact | File | Lines |
|---|-------------|----------|--------|------|-------|
| 3.1 | `call_evaluate` spawns OS thread + new Tokio runtime per call | **High** | Significant | `sync_mode/service_calls.rs` | 268–296 |
| 8.1 | No WS frame size limit on main `/ws` endpoint | **Medium** | Moderate DoS | `web.rs` | 363–370 |
| 2.1 | `send_or_sentinel` clones every non-terminal message | **Medium** | Minor–moderate | `execute/ws_send.rs` | 17 |
| 2.2 | `output_dir()` reads env vars on every call | **Medium** | Minor | `execute/files.rs` | 38–51 |
| 2.3 | `load_all_files` holds all content in memory simultaneously | **Medium** | Significant for large crawls | `download/manifest.rs` | 58–116 |
| 3.3 | Shell PTY sender acquires Mutex on every output chunk | **Medium** | Minor | `shell.rs` | 85–92 |
| 5.1 | `serve_output_file` reads entire file into memory | **Medium** | Significant for large files | `web.rs` | 273–276 |
| 5.2 | `load_all_files` sequential file reads | **Medium** | Moderate for large crawls | `download/manifest.rs` | 70–108 |
| 7.1 | `exec_tx`/`tracking_tx` capacity 256 — can truncate verbose output | **Medium** | User-visible output loss | `ws_handler.rs` | 99–100 |
| 8.2 | Shell PTY no absolute max-lifetime cap | **Medium** | FD leak under long-lived shells | `shell.rs` | 96–143 |
| 9.1 | `HashSet` allocation from `ALLOWED_FLAGS` on every execute | **Medium** | Minor | `execute.rs` | 177–192 |
| 4.1 | `MCP_SERVER_CACHE` std Mutex TOCTOU on concurrent miss | **Medium** | Minor correctness | `execute/mcp_config.rs` | 20–58 |
| 3.2 | `resolve_exe` not cached, `spawn_blocking` every subprocess | **Low** | Minor | `execute.rs` | 299–311 |
| 5.3 | `handle_read_file` canonicalizes base_dir on every call | **Low** | Minor | `execute/files.rs` | 272 |
| 6.1 | Stats broadcast serialized when no subscribers | **Low** | Wasted CPU/alloc at idle | `docker_stats.rs` | 106–109 |
| 6.2 | Container list polled every 500ms | **Low** | Minor Docker API overhead | `docker_stats.rs` | 43–62 |
| 7.2 | ACP event channel capacity 256 | **Low** | Minor | `sync_mode/pulse_chat.rs` | 336 |
| 8.3 | `crawl_job_ids` Vec grows unboundedly per connection | **Low** | Minor memory leak over time | `ws_handler.rs` | 337–339 |
| 9.2 | Two time syscalls per rate limit check | **Low** | Negligible | `rate_limiter.rs` | 45–76 |
| 10.2 | `json!` macro per stderr line in subprocess | **Low** | Minor | `sync_mode/subprocess.rs` | 97–100 |
| 11.1 | ZIP built fully in-memory (up to 500 MB) | **Medium** | Significant for large archives | `download/archive.rs` | 28–49 |
| 11.2 | Pack assembly (MD/XML) on async thread, no `spawn_blocking` | **Low** | Minor for small crawls | `web.rs` / `pack.rs` | — |

---

## Prioritized Action List

**Do now (correctness + meaningful performance):**
1. **3.1** — Add `"evaluate"` to `ACP_MODES` or add a dedicated `EVALUATE_SEMAPHORE`. A single missing guard allows unbounded OS thread creation.
2. **8.1** — Apply `ws.max_message_size(1_048_576)` to the main `/ws` endpoint.
3. **2.1** — Fix `send_or_sentinel` to not clone on the hot success path.
4. **9.1** — Cache the `ALLOWED_FLAGS` `HashSet` in a `LazyLock`.

**Do soon (quality / robustness):**
5. **2.2** — Cache `output_dir()` in a `LazyLock`.
6. **5.1** — Stream `serve_output_file` responses instead of buffering.
7. **8.2** — Add absolute max lifetime to shell PTY sessions.
8. **6.1** — Skip `build_stats_message` when `receiver_count() == 0`.
9. **11.2** — Wrap `build_pack_md` / `build_pack_xml` in `spawn_blocking`.

**Future / nice-to-have:**
10. **2.3 / 5.2 / 11.1** — Streaming download pipeline (ZIP streaming, parallel file loads, incremental pack output).
11. **3.3** — Replace shell Mutex with a message channel.
12. **4.1** — Migrate `MCP_SERVER_CACHE` to `RwLock`.
