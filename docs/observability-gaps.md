# axon_rust Observability Gap Report

**Generated:** 2026-04-29  
**Auditor:** Automated static analysis + subagent synthesis  
**Scope:** All `crates/**/*.rs` files, excluding test files where gaps are test-only

## Summary

| Metric | Value |
|--------|-------|
| Total .rs source files | 416 |
| Files with **any** tracing/logging | 43 (10.3%) |
| Files with **no** logging (>20 lines) | ~250+ |
| Total gaps identified | 69 |
| Critical severity | 12 |
| High severity | 28 |
| Medium severity | 22 |
| Low severity | 7 |

**Core finding:** The ACP subsystem and nearly all business logic crates have migrated toward a custom `ServiceEvent::Log` channel + `log_warn`/`log_info` wrappers, but these are WS-client-facing channels that can be dropped on disconnect. Zero `tracing::*` spans are emitted for most critical paths. The 10% tracing coverage rate means structured log aggregators (Loki, Datadog, CloudWatch) receive almost nothing from production runs.

---

## CRITICAL Severity Gaps

---

### GAP-C01
**FILE:** `crates/services/acp.rs`  
**FUNCTION:** `build_adapter_command`  
**GAP TYPE:** Security  
**SEVERITY:** Critical  
**DESCRIPTION:** `LAB_SPAWN_DEPTH=1` is injected as the sole recursion guard preventing `axon → claude-agent-acp → lab serve mcp --stdio → axon → ...` infinite recursion loops. This injection fires silently with zero log. Operators cannot distinguish "guard fired and prevented recursion" from "guard was never relevant." A misconfigured environment that should have triggered the guard leaves no observable signal.  
**SUGGESTED FIX:**
```rust
command.env("LAB_SPAWN_DEPTH", "1");
tracing::debug!(
    program = %self.adapter.program,
    cwd = ?self.adapter.cwd,
    "acp: LAB_SPAWN_DEPTH=1 injected — recursion guard active"
);
```

---

### GAP-C02
**FILE:** `crates/services/acp/session.rs`  
**FUNCTION:** `spawn_adapter_with_io`  
**GAP TYPE:** ACP lifecycle  
**SEVERITY:** Critical  
**DESCRIPTION:** After `spawn_adapter()` succeeds and the child is wrapped in `AdapterGuard`, the child PID is never logged. The exit watcher fires when the adapter exits — `Ok(status) if !status.success()` sends a crash string over `exit_tx` but does NOT call `tracing::error!`. An adapter OOM, signal kill, or non-zero exit leaves no structured log entry anywhere.  
**SUGGESTED FIX:**
```rust
// After AdapterGuard wraps child:
tracing::info!(pid = guard.child_pid(), program = %program, "acp: adapter subprocess spawned");
// In exit watcher Ok(status) arm:
if !status.success() {
    tracing::error!(exit_status = %status, pid = %pid, "acp: adapter exited with failure");
}
```

---

### GAP-C03
**FILE:** `crates/services/acp/session_cache.rs`  
**FUNCTION:** `reap_expired` (spawned as `reaper_loop`)  
**GAP TYPE:** ACP lifecycle / Resource exhaustion  
**SEVERITY:** Critical  
**DESCRIPTION:** `tokio::spawn(reaper_loop())` has 0 log lines before the spawn and 0 log lines inside the reaper for eviction events. Sessions evicted by TTL (30 min) or hung-turn threshold (5 min) are silently killed. Hung-turn eviction is a sign of adapter health problems. Operators have zero visibility into eviction frequency or which adapters are being killed.  
**SUGGESTED FIX:**
```rust
tracing::info!(session_ttl_secs = SESSION_TTL.as_secs(), "acp: session cache reaper starting");
tokio::spawn(reaper_loop());
// Inside reap_expired, per eviction:
tracing::warn!(agent_key = %key, reason = "ttl_expired", age_secs = %age, "acp: session evicted");
tracing::warn!(agent_key = %key, reason = "hung_turn", turn_age_secs = %turn_age, "acp: session evicted (hung turn)");
```

---

### GAP-C04
**FILE:** `crates/web/ws_handler.rs`  
**FUNCTION:** `handle_ws`  
**GAP TYPE:** ACP lifecycle  
**SEVERITY:** Critical  
**DESCRIPTION:** Zero log on WebSocket connection open or close. `conn_id` and `client_ip` are available but never logged. There is no audit trail of which clients connect, when they disconnect (clean vs. error), or how long sessions last. This makes security auditing and support debugging impossible from logs alone.  
**SUGGESTED FIX:**
```rust
tracing::info!(conn_id = %conn.conn_id, client_ip = %client_ip, "ws: connection opened");
// after tasks.shutdown():
tracing::info!(conn_id = %conn.conn_id, client_ip = %client_ip, duration_ms = %elapsed, "ws: connection closed");
```

---

### GAP-C05
**FILE:** `crates/web/execute.rs`  
**FUNCTION:** `acquire_acp_permit`  
**GAP TYPE:** Resource exhaustion  
**SEVERITY:** Critical  
**DESCRIPTION:** When the ACP semaphore times out after 30 seconds (all ACP slots occupied), an error is sent to the client but zero `tracing::warn!` is emitted. Semaphore exhaustion is a production capacity event — ops teams need to see it immediately in logs and alert on it.  
**SUGGESTED FIX:**
```rust
Err(_) => {
    tracing::warn!(exec_id = %ws_ctx.exec_id, timeout_secs = 30, "execute: ACP semaphore exhausted — request rejected");
    send_error_dual(...).await;
    Err(())
}
```

---

### GAP-C06
**FILE:** `crates/web/execute/sync_mode/subprocess.rs`  
**FUNCTION:** `finalize_exit`  
**GAP TYPE:** Subprocess  
**SEVERITY:** Critical  
**DESCRIPTION:** Subprocess exit codes never reach server logs. Non-zero exits are forwarded to the WS client as an error string but emit nothing to tracing. On `wait()` failure (`Err(e)`) there is also no server log. Subprocess crash forensics are impossible from server logs alone.  
**SUGGESTED FIX:**
```rust
match exit_status {
    Ok(s) if s.success() => tracing::debug!(exec_id = %id, exit_code = 0, "subprocess exited ok"),
    Ok(s) => tracing::warn!(exec_id = %id, exit_code = s.code().unwrap_or(-1), "subprocess exited non-zero"),
    Err(e) => tracing::error!(exec_id = %id, error = %e, "subprocess wait() failed"),
}
```

---

### GAP-C07
**FILE:** `crates/services/acp_llm/runner.rs`  
**FUNCTION:** `run_completion_on_blocking_thread`  
**GAP TYPE:** ACP lifecycle  
**SEVERITY:** Critical  
**DESCRIPTION:** The entire ACP completion lifecycle — spawn, timeout, result — has zero tracing. The 300-second timeout fires with no `tracing::warn!`. Thread panics return `"ACP completion blocking thread panicked"` with no log. There is no way to correlate a completion request to its outcome in logs.  
**SUGGESTED FIX:**
```rust
tracing::debug!(model = ?req.model, streaming = delta_tx.is_some(), "acp_llm: completion started");
// on timeout:
tracing::warn!(timeout_secs = ACP_TIMEOUT_SECS, "acp_llm: completion timed out");
// on join error:
tracing::error!(error = %err, "acp_llm: blocking thread panicked");
```

---

### GAP-C08
**FILE:** `crates/services/acp_llm/ws_runner.rs`  
**FUNCTION:** `run_ws_completion`  
**GAP TYPE:** ACP lifecycle  
**SEVERITY:** Critical  
**DESCRIPTION:** The entire WS-backed ACP completion lifecycle has zero logging — no connect attempt, no connect success, no timeout, no `WsIncomingEvent::Error`, no clean completion. The `ws_url` (remote host) is never logged. Remote ACP failures are completely invisible in server logs.  
**SUGGESTED FIX:**
```rust
tracing::debug!(ws_url = %ws_url, model = ?req.model, "acp_llm: WS completion connecting");
// on connect error:
tracing::error!(ws_url = %ws_url, error = %e, "acp_llm: WS connect failed");
// on WsIncomingEvent::Error:
tracing::warn!(ws_url = %ws_url, server_error = %msg, "acp_llm: WS server returned error");
// on timeout:
tracing::warn!(ws_url = %ws_url, timeout_secs = %t, "acp_llm: WS completion timed out");
```

---

### GAP-C09
**FILE:** `crates/jobs/common/watchdog.rs`  
**FUNCTION:** `reclaim_stale_running_jobs`  
**GAP TYPE:** Error path  
**SEVERITY:** Critical  
**DESCRIPTION:** The entire watchdog sweep has zero tracing. The three DB operations (`batch_retry_jobs`, `batch_fail_exhausted_jobs`, `batch_mark_candidates`) propagate errors via `?` but emit nothing. `WatchdogSweepStats` is computed but never logged. A DB failure during a watchdog sweep is completely silent.  
**SUGGESTED FIX:**
```rust
tracing::debug!(table = %table, job_kind = %job_kind, "watchdog: sweep start");
// after complete:
tracing::info!(table = %table, reclaimed = %stats.reclaimed_jobs, exhausted = %stats.exhausted_jobs, "watchdog: sweep complete");
// on error:
tracing::error!(table = %table, error = %e, "watchdog: batch operation failed");
```

---

### GAP-C10
**FILE:** `crates/jobs/common/watchdog.rs`  
**FUNCTION:** `batch_fail_exhausted_jobs`  
**GAP TYPE:** ACP lifecycle / Error path  
**SEVERITY:** Critical  
**DESCRIPTION:** Jobs permanently marked `failed` after exhausting reclaim attempts have no `tracing::error!` per job_id. A permanently lost job — the worst outcome in the job system — leaves no structured log entry with job_id, job_kind, or attempt count.  
**SUGGESTED FIX:**
```rust
for id in &exhausted_ids {
    tracing::error!(
        job_id = %id, table = %table, job_kind = %job_kind,
        max_attempts = %MAX_WATCHDOG_RECLAIM_ATTEMPTS,
        "watchdog: job permanently failed after exhausting reclaim attempts"
    );
}
```

---

### GAP-C11
**FILE:** `crates/web.rs`  
**FUNCTION:** (web server `tokio::spawn` at line ~158)  
**GAP TYPE:** Startup/shutdown  
**SEVERITY:** Critical  
**DESCRIPTION:** The axum/hyper listener spawn has 0 preceding log lines. There is no "web server starting on addr:port" log before the spawn and no "web server exited" log on termination. Server startup is the highest-value observability event in the entire process.  
**SUGGESTED FIX:**
```rust
tracing::info!(addr = %bind_addr, "web server starting");
tokio::spawn(async move {
    if let Err(e) = server.await {
        tracing::error!(error = %e, "web server exited with error");
    } else {
        tracing::info!("web server shut down cleanly");
    }
});
```

---

### GAP-C12
**FILE:** (global — binary entrypoint)  
**FUNCTION:** `main()` (or equivalent)  
**GAP TYPE:** Startup/shutdown  
**SEVERITY:** Critical  
**DESCRIPTION:** No `tracing_subscriber` initialization is visible in any scanned file. If `tracing_subscriber` is not initialized before `main` starts work, **all `tracing::*!()` calls are silently dropped.** This means fixing every other gap in this report would have zero effect until the subscriber is wired up. Also: no evidence of `RUST_LOG` env-var-based log level configuration or runtime log level reload.  
**SUGGESTED FIX:**
```rust
// In main(), before tokio runtime starts:
tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .with_target(true)
    .with_thread_ids(true)
    .json()
    .init();
tracing::info!(version = env!("CARGO_PKG_VERSION"), "axon starting");
```

---

## HIGH Severity Gaps

### GAP-H01
**FILE:** `crates/services/acp.rs` | **FUNCTION:** `build_adapter_command`  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** High  
**DESCRIPTION:** Resolved program path, working directory, and env allowlist never logged before command is built. On spawn failure, there is no prior context about what binary was attempted.  
**SUGGESTED FIX:** `tracing::debug!(program = %self.adapter.program, cwd = ?self.adapter.cwd, "acp: building adapter command");`

### GAP-H02
**FILE:** `crates/services/acp.rs` | **FUNCTION:** `spawn_adapter`  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** High  
**DESCRIPTION:** Child PID never logged after successful spawn. Debugging adapter hangs or zombie processes requires correlating RAII drop with OS process tables manually.  
**SUGGESTED FIX:** `tracing::info!(program = %self.adapter.program, pid = child.id(), "acp: adapter spawned");`

### GAP-H03
**FILE:** `crates/services/acp.rs` | **FUNCTION:** `run_acp_event_loop` (timeout arm)  
**GAP TYPE:** Error path | **SEVERITY:** High  
**DESCRIPTION:** ACP adapter timeout (N-second wall clock) returns `Err(String)` with no `tracing::error!`. A 5-minute timeout expiry leaves no trace entry.  
**SUGGESTED FIX:** `tracing::error!(timeout_secs = timeout.as_secs(), "acp: adapter timed out");`

### GAP-H04
**FILE:** `crates/services/acp.rs` | **FUNCTION:** `run_acp_event_loop` (join error)  
**GAP TYPE:** Error path | **SEVERITY:** High  
**DESCRIPTION:** `.map_err(|err| format!("failed to join ACP runtime worker: {err}"))` swallows a `JoinError` (thread panic/cancel) without logging. A panic in the ACP runtime thread is invisible.  
**SUGGESTED FIX:** `.map_err(|err| { tracing::error!(error = %err, "acp: spawn_blocking worker panicked"); ... })?;`

### GAP-H05
**FILE:** `crates/services/acp/runtime.rs` | **FUNCTION:** `run_prompt_turn` (adapter crash arm)  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** High  
**DESCRIPTION:** When adapter crashes mid-session (exit_rx fires), the function returns `Err(String)` with zero `tracing::error!`. An adapter crash — the most important failure mode — leaves no structured log entry.  
**SUGGESTED FIX:** `tracing::error!(session_id = %session_id, crash_msg = ?msg, "acp: adapter crashed before returning prompt result");`

### GAP-H06
**FILE:** `crates/services/acp/session.rs` | **FUNCTION:** `initialize_connection` (auth failure)  
**GAP TYPE:** Security | **SEVERITY:** High  
**DESCRIPTION:** Authentication failure emits only a `ServiceEvent::Log` (WS-client-facing, droppable) but never `tracing::error!`. Auth failures are security-relevant and must appear in server-side structured logs.  
**SUGGESTED FIX:** `tracing::error!(method = ?method.id(), error = %err, "acp: adapter authentication failed");`

### GAP-H07
**FILE:** `crates/services/acp/session_cache.rs` | **FUNCTION:** `AcpSessionCache::insert`  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** High  
**DESCRIPTION:** Session cache insertions not logged. Replacement of an existing handle (orphaned adapter drop) happens silently.  
**SUGGESTED FIX:** `tracing::info!(agent_key = %key, cache_size = self.len(), "acp: session inserted into cache");`

### GAP-H08
**FILE:** `crates/services/acp/bridge/state.rs` | **FUNCTION:** `finalize_successful_turn`  
**GAP TYPE:** Missing metrics | **SEVERITY:** High  
**DESCRIPTION:** No metrics for turn completion — no counter, histogram, or gauge for turn count, stop reason distribution, assistant text length, or duration. SLO tracking for ACP turn success rates is impossible.  
**SUGGESTED FIX:** `tracing::info!(session_id = %session_id, stop_reason = %stop_reason, response_bytes = text.len(), "acp: turn finalized");`

### GAP-H09
**FILE:** `crates/services/acp/persistent_conn/turn.rs` | **FUNCTION:** `run_turn_on_conn`  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** High  
**DESCRIPTION:** No log at turn start, no log of turn_id increment, no log when `ensure_turn_session` fails. A turn failure at session-setup stage is completely silent.  
**SUGGESTED FIX:** `tracing::info!(session_id = ?session_id.0, turn_id = %next_turn, "acp: starting persistent-connection turn");`

### GAP-H10
**FILE:** `crates/services/acp/persistent_conn/turn.rs` | **FUNCTION:** `run_prompt` (cancel / error paths)  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** High  
**DESCRIPTION:** Cancel-before-prompt, cancel-timeout, and prompt-error paths all return `Err(String)` with no `tracing::warn!/error!`.  
**SUGGESTED FIX:** `tracing::warn!(session_id = %id, "acp: turn cancelled before prompt"); tracing::error!(session_id = %id, error = %e, "acp: prompt failed");`

### GAP-H11
**FILE:** `crates/web/ws_handler.rs` | **FUNCTION:** `handle_ws_message` (unknown type)  
**GAP TYPE:** Error path | **SEVERITY:** High  
**DESCRIPTION:** Unknown message types are silently discarded. No way to detect client bugs, protocol drift, or probe attacks.  
**SUGGESTED FIX:** `tracing::warn!(conn_id = %conn.conn_id, msg_type = %t, "ws: unknown message type discarded");`

### GAP-H12
**FILE:** `crates/web/ws_handler.rs` | **FUNCTION:** JSON parse failure  
**GAP TYPE:** Error path | **SEVERITY:** High  
**DESCRIPTION:** Malformed JSON frames send an error frame to client but log nothing server-side.  
**SUGGESTED FIX:** `tracing::warn!(conn_id = %conn.conn_id, client_ip = %client_ip, "ws: invalid JSON frame received");`

### GAP-H13
**FILE:** `crates/web/ws_handler.rs` | **FUNCTION:** `handle_execute_msg` + `handle_read_file_msg` (rate limit)  
**GAP TYPE:** Security | **SEVERITY:** High  
**DESCRIPTION:** Rate limit hits are sent to client only. No server-side log — attacker traffic is invisible.  
**SUGGESTED FIX:** `tracing::warn!(conn_id = %conn.conn_id, client_ip = %ip, category = "execute", "ws: rate limit exceeded");`

### GAP-H14
**FILE:** `crates/web/execute.rs` | **FUNCTION:** `handle_command` (unknown mode)  
**GAP TYPE:** Security | **SEVERITY:** High  
**DESCRIPTION:** Unknown modes rejected without server-side log. Attacker probing allowed modes leaves no trace.  
**SUGGESTED FIX:** `tracing::warn!(exec_id = %id, mode = %mode, "execute: rejected unknown mode");`

### GAP-H15
**FILE:** `crates/web/execute.rs` | **FUNCTION:** `acquire_acp_permit` (closed semaphore)  
**GAP TYPE:** Error path | **SEVERITY:** High  
**DESCRIPTION:** "Should never happen" closed-semaphore branch sends error to client with no `tracing::error!`. Signals server-state corruption.  
**SUGGESTED FIX:** `tracing::error!(exec_id = %id, "execute: ACP semaphore closed — server state corrupted");`

### GAP-H16
**FILE:** `crates/web/execute.rs` | **FUNCTION:** `dispatch_subprocess_fallback`  
**GAP TYPE:** Subprocess | **SEVERITY:** High  
**DESCRIPTION:** Child PID never logged after subprocess spawn. Spawn failure not in tracing.  
**SUGGESTED FIX:** `tracing::info!(exec_id = %id, exe = %exe.display(), pid = child.id().unwrap_or(0), "subprocess spawned");`

### GAP-H17
**FILE:** `crates/services/acp_llm/runner.rs` | **FUNCTION:** `run_completion_local` (None channel)  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** High  
**DESCRIPTION:** No log when event channel closes (adapter finished) or when "did not emit a turn result" error fires.  
**SUGGESTED FIX:** `tracing::debug!("acp_llm: event channel closed — adapter finished"); tracing::error!("acp_llm: completion finished without a turn result");`

### GAP-H18
**FILE:** `crates/services/acp_llm/runner.rs` | **FUNCTION:** `resolve_adapter_command`  
**GAP TYPE:** Startup/shutdown | **SEVERITY:** High  
**DESCRIPTION:** If `AXON_ACP_ADAPTER_CMD` is empty, `Err` is returned with no log. Configuration failure at startup is silent.  
**SUGGESTED FIX:** `tracing::error!("AXON_ACP_ADAPTER_CMD is not set; ACP completions will fail");`

### GAP-H19
**FILE:** `crates/services/acp_llm/ws_runner.rs` | **FUNCTION:** `AcpWsCompletionRunner::from_config`  
**GAP TYPE:** Startup/shutdown | **SEVERITY:** High  
**DESCRIPTION:** If `AXON_ACP_WS_URL` is missing, `Err` is returned with no log.  
**SUGGESTED FIX:** `tracing::error!("AXON_ACP_WS_URL is not configured; WS-mode ACP completions will fail");`

### GAP-H20
**FILE:** `crates/services/acp_llm/ws_runner.rs` | **FUNCTION:** `run_ws_completion` (read/send errors)  
**GAP TYPE:** Error path | **SEVERITY:** High  
**DESCRIPTION:** WS read error and WS send failure both return `Err(String)` with no log. Network drops are invisible.  
**SUGGESTED FIX:** `tracing::warn!(ws_url = %url, error = %e, "acp_llm: WS read/send error");`

### GAP-H21
**FILE:** `crates/services/acp_llm/ws_runner.rs` | **FUNCTION:** `run_ws_completion` (missing result)  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** High  
**DESCRIPTION:** Server sends `Done` without prior `Result` — protocol violation — returned as `Err` with no log.  
**SUGGESTED FIX:** `tracing::error!(ws_url = %url, "acp_llm: WS server sent Done without Result — protocol violation");`

### GAP-H22
**FILE:** `crates/jobs/common/watchdog.rs` | **FUNCTION:** `batch_retry_jobs`  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** High  
**DESCRIPTION:** Jobs reset to `pending` for reclaim are not individually logged. Reclaim events should have traceable log entries with job_id and attempt count.  
**SUGGESTED FIX:** `tracing::warn!(job_id = %id, table = %table, "watchdog: job reclaimed and reset to pending");`

### GAP-H23
**FILE:** `crates/jobs/common/heartbeat.rs` | **FUNCTION:** `spawn_heartbeat_task` (inner loop)  
**GAP TYPE:** Error path | **SEVERITY:** High  
**DESCRIPTION:** `touch_running_job` errors silently discarded (`let _ = ...`). Heartbeat failure → watchdog reclaim → job loss chain is untraceable.  
**SUGGESTED FIX:** `if let Err(e) = touch_running_job(...).await { tracing::warn!(job_id = %id, error = %e, "heartbeat: touch failed — watchdog may reclaim job"); }`

### GAP-H24
**FILE:** `crates/cli/commands/serve_supervisor/runtime.rs` | **FUNCTION:** `run_supervisor`  
**GAP TYPE:** Startup/shutdown | **SEVERITY:** High  
**DESCRIPTION:** `tokio::spawn` for each child spec has zero preceding log. No log of supervisor config, child count, or child names at startup.  
**SUGGESTED FIX:** `tracing::info!(child_count = %specs.len(), "supervisor: starting"); for spec in &specs { tracing::info!(child = %spec.name, "supervisor: registering child"); }`

### GAP-H25
**FILE:** `crates/cli/commands/serve_supervisor/runtime.rs` | **FUNCTION:** `supervise_child` (restart loop)  
**GAP TYPE:** Retry | **SEVERITY:** High  
**DESCRIPTION:** Restart log does not include attempt number. Cannot tell from logs whether child is on restart #1 or #8.  
**SUGGESTED FIX:** `tracing::warn!(child = %spec.name, attempt = %unstable_restarts, backoff_secs = %delay, "supervisor: child restarting");`

### GAP-H26
**FILE:** `crates/cli/commands/serve_supervisor/runtime.rs` | **FUNCTION:** `supervise_child` (spawn failure)  
**GAP TYPE:** Error path / Subprocess | **SEVERITY:** High  
**DESCRIPTION:** `spawn_child` errors use only `eprintln!` (not `tracing::error!`). No structured fields captured.  
**SUGGESTED FIX:** `tracing::error!(child = %spec.name, program = %spec.program.display(), error = %err, attempt = %n, "supervisor: spawn failed");`

### GAP-H27
**FILE:** `crates/cli/commands/serve_supervisor/runtime.rs` | **FUNCTION:** `supervise_child` / `run_supervisor`  
**GAP TYPE:** Subprocess | **SEVERITY:** High  
**DESCRIPTION:** All supervisor logging uses raw `eprintln!` — invisible to tracing-based log sinks (JSON file, OpenTelemetry, Loki). Zero supervisor events appear in structured logs.  
**SUGGESTED FIX:** Replace `log_child_event`/`log_supervisor` with `tracing::info!/warn!/error!` macro calls with structured fields.

### GAP-H28
**FILE:** `crates/jobs/lite/workers.rs` | **FUNCTION:** (lite worker launcher)  
**GAP TYPE:** Startup/shutdown | **SEVERITY:** High  
**DESCRIPTION:** Six `tokio::spawn` calls for lite workers (crawl, embed, extract, ingest, refresh, graph) have 0 preceding log lines and no span. No confirmation which workers started or when they exit/panic.  
**SUGGESTED FIX:** `tracing::info!(worker = "crawl", "spawning lite crawl worker"); tokio::spawn(crawl_worker(...));`

---

## MEDIUM Severity Gaps

### GAP-M01
**FILE:** `crates/services/acp/runtime.rs` | **FUNCTION:** `run_session_probe`  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** Medium  
**DESCRIPTION:** Probe returns `Ok(())` with zero log — impossible to distinguish successful probe from silent failure. Session_id never logged.  
**SUGGESTED FIX:** `tracing::info!(session_id = %id, "acp: session probe completed successfully");`

### GAP-M02
**FILE:** `crates/services/acp/runtime.rs` | **FUNCTION:** `AdapterGuard::drop`  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** Medium  
**DESCRIPTION:** RAII kill fires silently. No log of which error path triggered it or what the child PID was.  
**SUGGESTED FIX:** `tracing::warn!(pid = child.id(), "acp: AdapterGuard dropped with live child — sending kill");`

### GAP-M03
**FILE:** `crates/services/acp/runtime.rs` | **FUNCTION:** `apply_mcp_capability_filter`  
**GAP TYPE:** Missing metrics | **SEVERITY:** Medium  
**DESCRIPTION:** MCP servers silently filtered out when adapter lacks HTTP/SSE transport. Operators cannot tell which MCP servers were dropped.  
**SUGGESTED FIX:** `tracing::warn!(dropped = %n, http = %http, sse = %sse, "acp: filtered MCP servers due to adapter capability mismatch");`

### GAP-M04
**FILE:** `crates/services/acp/session.rs` | **FUNCTION:** stderr reader loop break  
**GAP TYPE:** Error path | **SEVERITY:** Medium  
**DESCRIPTION:** Pipe read error (`Err(_)`) and EOF (`Ok(0)`) both silently break. Cannot distinguish clean exit from pipe error.  
**SUGGESTED FIX:** `Err(e) => { tracing::warn!(error = %e, "acp: stderr reader error — stopping capture"); break; }`

### GAP-M05
**FILE:** `crates/services/acp/session.rs` | **FUNCTION:** `initialize_connection` (auth token missing)  
**GAP TYPE:** Security | **SEVERITY:** Medium  
**DESCRIPTION:** `AXON_ACP_AUTH_TOKEN` missing emits `ServiceEvent::Log` (WS-facing) but not `tracing::warn!`. Structured log aggregators miss this security event.  
**SUGGESTED FIX:** `tracing::warn!("acp: adapter requires auth but AXON_ACP_AUTH_TOKEN is not set");`

### GAP-M06
**FILE:** `crates/services/acp/session.rs` | **FUNCTION:** `setup_session` (CWD validation)  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** Medium  
**DESCRIPTION:** `validate_cwd_usable` failure propagated via `?` with no `tracing::error!`.  
**SUGGESTED FIX:** `tracing::error!(cwd = %cwd.display(), error = %err, "acp: session CWD validation failed");`

### GAP-M07
**FILE:** `crates/services/acp/session.rs` | **FUNCTION:** `setup_session` (session_id)  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** Medium  
**DESCRIPTION:** Assigned `session_id` from adapter response never logged at tracing level. New session IDs are opaque in structured logs.  
**SUGGESTED FIX:** `tracing::info!(session_id = %r.session_id.0, cwd = %cwd.display(), "acp: new session created");`

### GAP-M08
**FILE:** `crates/services/acp/session_cache.rs` | **FUNCTION:** `AcpSessionCache::remove`  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** Medium  
**DESCRIPTION:** Cache removals not logged. Full session lifecycle (insert → active → remove) invisible in structured logs.  
**SUGGESTED FIX:** `tracing::info!(agent_key = %key, "acp: session removed from cache");`

### GAP-M09
**FILE:** `crates/services/acp/session_cache.rs` | **FUNCTION:** `AcpSessionCache::new` / `LazyLock`  
**GAP TYPE:** Startup/shutdown | **SEVERITY:** Medium  
**DESCRIPTION:** Cache initialization and reaper start have no startup log. TTL and threshold config never logged at boot.  
**SUGGESTED FIX:** `tracing::info!(ttl_secs = SESSION_TTL.as_secs(), hung_threshold_secs = SESSION_HUNG_TURN_THRESHOLD.as_secs(), "acp: session cache initialized");`

### GAP-M10
**FILE:** `crates/services/acp/session_cache/entry.rs` | **FUNCTION:** `CachedSession::buffer_event`  
**GAP TYPE:** Resource exhaustion | **SEVERITY:** High (moved up)  
**DESCRIPTION:** Replay buffer cap reached — events silently dropped. Client reconnecting will silently receive truncated replay with no indication data was lost.  
**SUGGESTED FIX:** `tracing::warn!(agent_key = %key, count = %n, byte_cap = MAX_REPLAY_BUFFER_BYTES, "acp: replay buffer cap reached — dropping event");`

### GAP-M11
**FILE:** `crates/services/acp/bridge/state.rs` | **FUNCTION:** `finalize_successful_turn` (stop_reason)  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** Medium  
**DESCRIPTION:** `MaxTokens`, `Refusal`, `Cancelled` stop reasons logged via `ServiceEvent::Log` but not `tracing::warn!`. `Refusal` has security implications.  
**SUGGESTED FIX:** `tracing::warn!(session_id = %id, stop_reason = %reason, "acp: turn ended with non-nominal stop reason");`

### GAP-M12
**FILE:** `crates/services/acp/persistent_conn/turn.rs` | **FUNCTION:** `run_turn_on_conn` (apply_requested_options)  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** Medium  
**DESCRIPTION:** Model/mode change failure silently downgraded — turn proceeds with wrong configuration. No `tracing::warn!`.  
**SUGGESTED FIX:** `tracing::warn!(session_id = %id, error = %err, "acp: failed to apply requested options — proceeding with current state");`

### GAP-M13
**FILE:** `crates/services/acp/persistent_conn/turn.rs` | **FUNCTION:** `create_new_session` / `load_or_fallback_session`  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** Medium  
**DESCRIPTION:** New session assignment and existing session load not logged at tracing level.  
**SUGGESTED FIX:** `tracing::info!(session_id = %id, "acp: persistent-conn: session created/loaded");`

### GAP-M14
**FILE:** `crates/web/ws_handler.rs` | **FUNCTION:** multiple `let _ = exec_tx.send(...)`  
**GAP TYPE:** Error path | **SEVERITY:** Medium  
**DESCRIPTION:** `SendError` from closed channel silently discards error frames to client.  
**SUGGESTED FIX:** `if conn.exec_tx.send(msg).await.is_err() { tracing::warn!(conn_id = %id, "ws: exec_tx closed — dropped frame"); }`

### GAP-M15
**FILE:** `crates/web/ws_handler.rs` | **FUNCTION:** spawned tasks (no instrument)  
**GAP TYPE:** Missing metrics | **SEVERITY:** Medium  
**DESCRIPTION:** `tasks.spawn(async move {...})` without `.instrument(span)` — spawned tasks lose conn_id and exec_id context.  
**SUGGESTED FIX:** `.instrument(tracing::info_span!("ws_task", conn_id = %conn.conn_id, exec_id = %id))`

### GAP-M16
**FILE:** `crates/web/execute/sync_mode/subprocess.rs` | **FUNCTION:** `read_stderr` (channel close)  
**GAP TYPE:** Error path | **SEVERITY:** Medium  
**DESCRIPTION:** Closed tx channel silently breaks stderr reader. Client disconnect mid-stream leaves no diagnostic.  
**SUGGESTED FIX:** `tracing::debug!("stderr tx closed — stopping reader");`

### GAP-M17
**FILE:** `crates/services/acp_llm/runner.rs` | **FUNCTION:** `handle_completion_bridge_event` (`_` arm)  
**GAP TYPE:** ACP lifecycle | **SEVERITY:** Medium  
**DESCRIPTION:** Unrecognized `AcpBridgeEvent` variants silently discarded. New protocol events silently dropped.  
**SUGGESTED FIX:** `_ => { tracing::debug!(event = ?std::mem::discriminant(event), "acp_llm: bridge event ignored"); }`

### GAP-M18
**FILE:** `crates/services/runtime/full.rs` | **FUNCTION:** `run_worker` + worker start/stop  
**GAP TYPE:** Startup/shutdown | **SEVERITY:** High (multiple Medium sub-gaps)  
**DESCRIPTION:** Worker thread spawn unlogged, tokio runtime build failure silent, worker exit unlogged.  
**SUGGESTED FIX:** `tracing::info!(kind = %kind, "run_worker: spawning thread"); tracing::info!(kind = %kind, "worker: exited");`

### GAP-M19
**FILE:** `crates/services/system.rs` | **FUNCTION:** `full_status` / `load_status_jobs`  
**GAP TYPE:** Missing metrics | **SEVERITY:** Medium  
**DESCRIPTION:** `count_jobs` failures silently return `unwrap_or(0)` — status looks healthy when DB is down. No periodic queue-depth summary log.  
**SUGGESTED FIX:** `unwrap_or_else(|e| { tracing::warn!(error = %e, "load_status_jobs: count failed, defaulting 0"); 0 })`

### GAP-M20
**FILE:** `crates/jobs/common/heartbeat.rs` | **FUNCTION:** `spawn_content_aware_heartbeat` (start/stop)  
**GAP TYPE:** Startup/shutdown | **SEVERITY:** Medium  
**DESCRIPTION:** Heartbeat start and stop not logged. Cannot determine from logs if heartbeat was running for a given job during a time window.  
**SUGGESTED FIX:** `tracing::debug!(job_id = %id, interval_secs = %n, "heartbeat: starting"); // on stop: tracing::debug!(job_id = %id, "heartbeat: stopped");`

### GAP-M21
**FILE:** `crates/jobs/common/heartbeat.rs` | **FUNCTION:** `spawn_content_aware_heartbeat` (kill threshold)  
**GAP TYPE:** Missing metrics | **SEVERITY:** Medium  
**DESCRIPTION:** Forced job kill logged at `warn` level (via `log_warn` wrapper) instead of `tracing::error!`. Kill is an exceptional outcome.  
**SUGGESTED FIX:** `tracing::error!(job_id = %id, streak = %n, "heartbeat: kill threshold reached, cancelling job");`

### GAP-M22
**FILE:** (global) | **FUNCTION:** N/A — missing background telemetry task  
**GAP TYPE:** Missing metrics | **SEVERITY:** Medium  
**DESCRIPTION:** No periodic summary log events anywhere (e.g., every 60s: active sessions, pending job counts per kind, queue depth). Log-based monitoring has no baseline signal.  
**SUGGESTED FIX:** Add a background task emitting `tracing::info!(crawl = N, embed = M, ..., "job queue summary")` every 60 seconds.

---

## LOW Severity Gaps

### GAP-L01
**FILE:** `crates/services/acp/session.rs` | **FUNCTION:** `apply_model_config`  
Set-session-config-option failure propagated with no `tracing::error!`.

### GAP-L02
**FILE:** `crates/services/acp/bridge/state.rs` | **FUNCTION:** `apply_config_option_update`  
Config option updates in event loop have no `tracing::debug!` entry for development tracing.

### GAP-L03
**FILE:** `crates/services/runtime/full.rs` | **FUNCTION:** `cancel_job` / Graph arm  
Unsupported Graph operations return static error string with no log.

### GAP-L04
**FILE:** `crates/services/system.rs` | **FUNCTION:** `filter_and_view`  
Filter drops (reclaimed jobs excluded from status) not traced at debug level.

### GAP-L05
**FILE:** `crates/web/execute/sync_mode/subprocess.rs` | **FUNCTION:** `read_stdout` (None handle)  
None stdout handle silently returns empty vec — should be a debug-level warning.

### GAP-L06
**FILE:** `crates/services/acp_llm/runner.rs` | **FUNCTION:** `complete_streaming` (delta callback error)  
`on_delta(&delta)?` error propagated with no warning — caller can't distinguish delta-callback vs. ACP failure.

### GAP-L07
**FILE:** `crates/services/acp_llm/ws_runner.rs` | **FUNCTION:** `extract_event` (JSON parse)  
Non-JSON WS frames silently return `Ignore`. Should be `tracing::debug!` for protocol diagnostics.

---

## Subsystem Priority Matrix

| Subsystem | Critical | High | Medium | Low | Priority |
|-----------|----------|------|--------|-----|----------|
| ACP core (`services/acp/`) | 3 | 8 | 10 | 2 | **P0** |
| WebSocket + Execute (`web/`) | 2 | 6 | 3 | 1 | **P0** |
| ACP LLM runner (`services/acp_llm/`) | 2 | 4 | 2 | 2 | **P0** |
| Job watchdog + heartbeat (`jobs/common/`) | 2 | 3 | 2 | 0 | **P1** |
| Tracing infrastructure (global) | 1 | 0 | 0 | 0 | **P0** |
| Supervisor (`cli/serve_supervisor/`) | 0 | 4 | 1 | 0 | **P1** |
| Runtime/workers (`services/runtime/`, `jobs/lite/`) | 1 | 3 | 2 | 1 | **P1** |
| System service (`services/system.rs`) | 0 | 0 | 3 | 1 | **P2** |

## Top 5 Fixes by Operational Impact

1. **GAP-C12** — Initialize `tracing_subscriber` before main starts work. Without this, every other fix is a no-op.
2. **GAP-C04** — Log WebSocket connection open/close with `conn_id` and `client_ip`. Required for any security audit trail.
3. **GAP-C02/C03** — Log ACP adapter spawn PID and session cache evictions. Core ACP lifecycle visibility.
4. **GAP-C09/C10** — Add tracing to watchdog sweep and per-job exhaustion events. Permanently lost jobs are currently invisible.
5. **GAP-C05** — Log ACP semaphore exhaustion. Capacity alerts depend on this signal.

