# Prewarm Observability Tightening Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add structured logging with elapsed timing, proper error context chaining, and traceability to the ACP prewarm module.

**Architecture:** Single-file refactor of `prewarm.rs`. Switch from `Result<String, String>` to `anyhow::Result<String>` with `.context()` chains (matching `crates/jobs/` pattern). Add `Instant::now()` + `elapsed().as_millis()` timing (matching jobs worker pattern). Add tracing span via manual `tracing::info_span!` (consistent with codebase's manual tracing style — no `#[tracing::instrument]` exists anywhere). Fix silent fallbacks, drain join error swallowing, and turn-failure-masking.

**Tech Stack:** `anyhow`, `tracing`, `std::time::Instant`

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/web/execute/sync_mode/prewarm.rs` | Modify | All 6 fixes live here — self-contained module |

No new files. No changes to callers — `spawn_prewarm_task` already matches on `Ok`/`Err` and logs, and `anyhow::Error` implements `Display` so the `{e}` format in the caller's log line will show the full context chain automatically.

---

### Task 1: Switch to anyhow::Result with .context() chains

**Files:**
- Modify: `crates/web/execute/sync_mode/prewarm.rs:1-165`

This task converts the error handling from `Result<String, String>` with `map_err(|e| e.to_string())` / `format!()` to `anyhow::Result<String>` with `.context()` chains. This preserves the causal error chain instead of flattening it to a string.

**Pattern reference** (from `crates/jobs/common/amqp.rs`):
```rust
use anyhow::{Context, Result};
// ...
.context("amqp connect failed")?;
```

- [ ] **Step 1: Write the failing test**

No new test needed — this is a signature change. Existing tests (`default_prewarm_caps_are_permissive`, `default_config_has_prewarm_enabled`) don't exercise the error paths. The type change is verified by `cargo check`.

- [ ] **Step 2: Add anyhow import and change prewarm_adapter signature**

Change:
```rust
use std::sync::Arc;
```
To:
```rust
use std::sync::Arc;
use anyhow::Context as _;
```

Change the function signature:
```rust
async fn prewarm_adapter(cfg: &Arc<Config>, agent: PulseChatAgent) -> Result<String, String> {
```
To:
```rust
async fn prewarm_adapter(cfg: &Arc<Config>, agent: PulseChatAgent) -> anyhow::Result<String> {
```

- [ ] **Step 3: Convert all error sites to anyhow chains**

**IMPORTANT:** `resolve_acp_adapter_command` returns `Result<_, String>` and both
`prepare_initialize` / `prepare_session_setup` return `Result<_, Box<dyn std::error::Error>>`
(without `Send + Sync`). Neither satisfies `Into<anyhow::Error>` directly, so all three
must use `.map_err(|e| anyhow::anyhow!("{e}"))` before `.context()`. Same pattern as
`crates/jobs/embed/worker.rs:251`.

**3a.** Replace `resolve_acp_adapter_command` call (line 53):
```rust
    let adapter = resolve_acp_adapter_command(cfg, agent, caps)?;
```
With:
```rust
    let adapter = resolve_acp_adapter_command(cfg, agent, caps)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("resolve adapter command")?;
```

**3b.** Replace `prepare_initialize` call (line 68):
```rust
    let initialize = scaffold.prepare_initialize().map_err(|e| e.to_string())?;
```
With:
```rust
    let initialize = scaffold
        .prepare_initialize()
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("prepare_initialize failed")?;
```

**3c.** Replace `prepare_session_setup` call (lines 81-83 — the full expression is `scaffold.prepare_session_setup(&minimal_req, &cwd).map_err(|e| e.to_string())?`):
```rust
    let session_setup = scaffold
        .prepare_session_setup(&minimal_req, &cwd)
        .map_err(|e| e.to_string())?;
```
With:
```rust
    let session_setup = scaffold
        .prepare_session_setup(&minimal_req, &cwd)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("prepare_session_setup failed")?;
```

**3d.** Replace `run_turn` dispatch error (line 122 — the full expression is `handle.run_turn(...).await.map_err(|e| format!(...))?`):
```rust
        .map_err(|e| format!("prewarm turn dispatch failed: {e}"))?;
```
With:
```rust
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("prewarm turn dispatch failed")?;
```

- [ ] **Step 4: Update spawn_prewarm_task to handle anyhow::Error**

The caller already does:
```rust
Err(e) => {
    tracing::warn!(context = "acp_prewarm", error = %e, "prewarm failed ...")
}
```
`anyhow::Error` implements `Display` with the full context chain, so `%e` will print the chain automatically. **No change needed here** — but verify this compiles.

- [ ] **Step 5: Run cargo check**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors

- [ ] **Step 6: Run tests**

Run: `cargo test -p axon --lib prewarm 2>&1 | tail -10`
Expected: 2 tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/web/execute/sync_mode/prewarm.rs
git commit -m "refactor(prewarm): switch to anyhow::Result with .context() chains

Replaces Result<String, String> with anyhow::Result<String> for proper
error context chaining. Matches crates/jobs/ error handling pattern.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

### Task 2: Add elapsed timing to prewarm_adapter

**Files:**
- Modify: `crates/web/execute/sync_mode/prewarm.rs:39-164`

Add `Instant::now()` at function entry and log `elapsed_ms` at every exit point. Matches the established pattern in `crates/jobs/embed/worker.rs` and `crates/core/health/doctor.rs`.

- [ ] **Step 1: Add Instant import and start timer**

Add to imports:
```rust
use std::time::Instant;
```

Add as first line inside `prewarm_adapter()`, before `let caps = ...`:
```rust
    let start = Instant::now();
```

- [ ] **Step 2: Add elapsed_ms to the "already cached" early return log**

Change:
```rust
        tracing::info!(
            context = "acp_prewarm",
            agent_key = %agent_key,
            "adapter already cached — skipping prewarm",
        );
```
To:
```rust
        tracing::info!(
            context = "acp_prewarm",
            agent_key = %agent_key,
            elapsed_ms = start.elapsed().as_millis() as u64,
            "adapter already cached — skipping prewarm",
        );
```

- [ ] **Step 3: Add elapsed_ms to the success log**

Change:
```rust
            tracing::info!(
                context = "acp_prewarm",
                agent_key = %agent_key,
                program = %adapter_name,
                "adapter pre-warmed successfully",
            );
```
To:
```rust
            tracing::info!(
                context = "acp_prewarm",
                agent_key = %agent_key,
                program = %adapter_name,
                elapsed_ms = start.elapsed().as_millis() as u64,
                "adapter pre-warmed successfully",
            );
```

- [ ] **Step 4: Add elapsed_ms to the turn-failure warn log**

Change:
```rust
            tracing::warn!(
                context = "acp_prewarm",
                agent_key = %agent_key,
                error = %e,
                "prewarm turn failed (adapter may still be usable)",
            );
```
To:
```rust
            tracing::warn!(
                context = "acp_prewarm",
                agent_key = %agent_key,
                error = %e,
                elapsed_ms = start.elapsed().as_millis() as u64,
                "prewarm turn failed (adapter may still be usable)",
            );
```

- [ ] **Step 5: Add elapsed_ms to spawn_prewarm_task caller logs**

In `spawn_prewarm_task`, add timing around the `prewarm_adapter` call:

Change:
```rust
    tokio::spawn(async move {
        // Small delay to let the server bind first.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        match prewarm_adapter(&cfg, PulseChatAgent::Claude).await {
            Ok(key) => {
                tracing::info!(context = "acp_prewarm", agent_key = %key, "prewarm complete")
            }
            Err(e) => {
                tracing::warn!(context = "acp_prewarm", error = %e, "prewarm failed (will cold-start on first request)")
            }
        }
    });
```
To:
```rust
    tokio::spawn(async move {
        // Small delay to let the server bind first.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let start = std::time::Instant::now();
        match prewarm_adapter(&cfg, PulseChatAgent::Claude).await {
            Ok(key) => {
                tracing::info!(
                    context = "acp_prewarm",
                    agent_key = %key,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "prewarm complete",
                )
            }
            Err(e) => {
                tracing::warn!(
                    context = "acp_prewarm",
                    error = %e,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "prewarm failed (will cold-start on first request)",
                )
            }
        }
    });
```

- [ ] **Step 6: Run cargo check + tests**

Run: `cargo check 2>&1 | tail -3 && cargo test -p axon --lib prewarm 2>&1 | tail -5`
Expected: compiles, 2 tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/web/execute/sync_mode/prewarm.rs
git commit -m "feat(prewarm): add elapsed_ms timing to all log lines

Logs wall-clock duration at every exit point (success, cached skip,
turn failure, total prewarm). Matches crates/jobs/ timing pattern.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

### Task 3: Add tracing span to prewarm_adapter

**Files:**
- Modify: `crates/web/execute/sync_mode/prewarm.rs:39-45`

Add a manual `tracing::info_span!` so all log lines within `prewarm_adapter` inherit structured context. Uses manual span entry (consistent with the codebase — no `#[tracing::instrument]` exists anywhere).

- [ ] **Step 1: Add span at top of prewarm_adapter, after start timer**

After `let start = Instant::now();`, add:
```rust
    let span = tracing::info_span!(
        "acp_prewarm",
        agent = %format!("{agent:?}"),
    );
    let _guard = span.enter();
```

This means every `tracing::info!` / `tracing::warn!` inside the function will automatically include `span.name = "acp_prewarm"` and `agent = "Claude"` in JSON output. The nested calls to `scaffold.prepare_initialize()`, `AcpConnectionHandle::spawn()`, etc. will also inherit the span if they emit tracing events.

- [ ] **Step 2: Run cargo check + tests**

Run: `cargo check 2>&1 | tail -3 && cargo test -p axon --lib prewarm 2>&1 | tail -5`
Expected: compiles, 2 tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/web/execute/sync_mode/prewarm.rs
git commit -m "feat(prewarm): add tracing span for structured log correlation

All log lines within prewarm_adapter now inherit the acp_prewarm span
with agent field, enabling correlation in JSON log output.

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

### Task 4: Fix silent fallbacks, drain swallow, and turn-failure masking

**Files:**
- Modify: `crates/web/execute/sync_mode/prewarm.rs`

Three small fixes in one task — they're tightly coupled (all affect the same function's control flow) and individually trivial.

**IMPORTANT:** This task assumes Tasks 1-3 have already been applied. The "Change from" snippets reflect the post-Task-2 state (with `elapsed_ms` fields and `anyhow` types).

- [ ] **Step 1: Log the /tmp fallback in resolve_prewarm_working_dir**

Change:
```rust
async fn resolve_prewarm_working_dir() -> Result<std::path::PathBuf, String> {
    let base = std::env::var("AXON_DATA_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        format!("{home}/.local/share")
    });
```
To:
```rust
async fn resolve_prewarm_working_dir() -> anyhow::Result<std::path::PathBuf> {
    let base = std::env::var("AXON_DATA_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| {
            tracing::warn!(
                context = "acp_prewarm",
                "neither AXON_DATA_DIR nor HOME set, falling back to /tmp",
            );
            "/tmp".to_string()
        });
        format!("{home}/.local/share")
    });
```

Also update the `create_dir_all` error:
```rust
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|e| format!("failed to create prewarm dir: {e}"))?;
```
To:
```rust
    tokio::fs::create_dir_all(&path)
        .await
        .context("failed to create prewarm working dir")?;
```

- [ ] **Step 2: Handle drain join error instead of swallowing**

Change:
```rust
    let _ = drain_handle.await;
```
To:
```rust
    if let Err(e) = drain_handle.await {
        tracing::warn!(
            context = "acp_prewarm",
            error = %e,
            "prewarm event drain task panicked",
        );
    }
```

- [ ] **Step 3: Make turn failure propagate as Err**

The current code returns `Ok(agent_key)` even when the turn failed, which causes the caller (`spawn_prewarm_task`) to log "prewarm complete" at info level — masking the failure.

Change the turn_result match (lines 145-164):
```rust
    match turn_result {
        Ok(()) => {
            tracing::info!(
                context = "acp_prewarm",
                agent_key = %agent_key,
                program = %adapter_name,
                elapsed_ms = start.elapsed().as_millis() as u64,
                "adapter pre-warmed successfully",
            );
            Ok(agent_key)
        }
        Err(e) => {
            tracing::warn!(
                context = "acp_prewarm",
                agent_key = %agent_key,
                error = %e,
                elapsed_ms = start.elapsed().as_millis() as u64,
                "prewarm turn failed (adapter may still be usable)",
            );
            Ok(agent_key)
        }
    }
```
To:
```rust
    match turn_result {
        Ok(()) => {
            tracing::info!(
                context = "acp_prewarm",
                agent_key = %agent_key,
                program = %adapter_name,
                elapsed_ms = start.elapsed().as_millis() as u64,
                "adapter pre-warmed successfully",
            );
            Ok(agent_key)
        }
        Err(e) => {
            // Log with structured fields at the module level (agent_key, program,
            // elapsed_ms) so these are searchable in JSON logs, then propagate
            // as Err so the caller logs at warn level instead of info.
            tracing::warn!(
                context = "acp_prewarm",
                agent_key = %agent_key,
                program = %adapter_name,
                error = %e,
                elapsed_ms = start.elapsed().as_millis() as u64,
                "prewarm turn failed (adapter may still be usable)",
            );
            anyhow::bail!("prewarm turn failed for {agent_key}: {e}")
        }
    }
```

- [ ] **Step 4: Run cargo check + tests**

Run: `cargo check 2>&1 | tail -3 && cargo test -p axon --lib prewarm 2>&1 | tail -5`
Expected: compiles, 2 tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/web/execute/sync_mode/prewarm.rs
git commit -m "fix(prewarm): eliminate silent fallbacks and error masking

- Log warning when falling back to /tmp for working dir
- Handle drain task panic instead of swallowing with let _
- Propagate turn failure as Err so caller logs correctly

Co-authored-by: Claude <noreply@anthropic.com>"
```

---

## Notes

**Why no `#[tracing::instrument]`:** Zero instances exist in the codebase. Introducing it here would be a new pattern. Manual `info_span!` achieves the same structured context while staying consistent.

**Why `anyhow` in `sync_mode`:** The `sync_mode` layer uses `Result<T, String>` everywhere else, but `prewarm.rs` is self-contained — its errors never cross into the dispatch path. They're consumed by `spawn_prewarm_task` which logs them. `anyhow::Error` implements `Display` with the full context chain, so the existing `error = %e` format works without any caller changes. This matches the `crates/jobs/` error handling pattern.

**Why turn failure is now `Err`:** Previously, a failed ping turn returned `Ok(agent_key)`, and the caller logged "prewarm complete" at info level — masking the failure. Now the module-level `tracing::warn!` fires with full structured fields (`agent_key`, `program`, `elapsed_ms`, `error`), AND the function returns `Err` so the caller also logs at warn level. Two log lines for the same failure is intentional: the inner one has structured fields for dashboards/alerts, the outer one has the anyhow context chain for debugging. The adapter session is still in the cache and may work for real requests — the session setup (`establish_acp_session`) may have succeeded even if the ping turn timed out or errored.

**What `elapsed_ms` looks like in JSON logs:**
```json
{"timestamp":"...","level":"INFO","context":"acp_prewarm","agent_key":"claude|normal|...","elapsed_ms":43217,"message":"adapter pre-warmed successfully"}
```
This is greppable, alertable, and dashboardable.
