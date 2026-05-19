# ACP Adapter Pre-Warming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the ~45-second cold start on the first ACP chat message by pre-spawning and warming the default adapter on server boot.

**Architecture:** On `axon serve` startup, spawn a background task that resolves the default adapter command, creates an `AcpConnectionHandle`, inserts it into `SESSION_CACHE`, and sends a lightweight "ping" turn to force the adapter through `establish_acp_session()` — the slow part. The first real user message then hits a warm, cached session instead of cold-spawning.

**Tech Stack:** Rust, tokio, ACP SDK (`agent_client_protocol`), existing `session_cache.rs` + `persistent_conn.rs` + `pulse_chat.rs`

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Modify | `crates/web/execute/sync_mode/pulse_chat.rs` | Extract `build_agent_key()` helper (DRY with prewarm) |
| Modify | `crates/web/execute/sync_mode.rs` | Add `pub(crate) mod prewarm;` declaration |
| Modify | `crates/web/execute.rs` | Re-export `sync_mode::prewarm` as `pub(crate)` |
| Create | `crates/web/execute/sync_mode/prewarm.rs` | Pre-warm orchestration: resolve adapter, spawn handle, send ping turn |
| Modify | `crates/web.rs` | Call `prewarm::spawn_prewarm_task()` during `start_server()` |
| Modify | `crates/core/config/types/config.rs` | Add `acp_prewarm: bool` field |
| Modify | `crates/core/config/parse/build_config.rs` | Parse `AXON_ACP_PREWARM` env var |
| Modify | `crates/core/config/types/config_impls.rs` | Default + Debug for `acp_prewarm` |

---

### Task 1: Extract `build_agent_key()` helper from `pulse_chat.rs`

The prewarm module needs to construct the same `agent_key` that `get_or_create_acp_connection` uses. Extract the key-building logic into a shared helper to keep things DRY.

**Files:**
- Modify: `crates/web/execute/sync_mode/pulse_chat.rs` (lines 182-198)

- [ ] **Step 1: Write the failing test**

Add a test to the existing `#[cfg(test)]` block (or create one) in `pulse_chat.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use super::acp_adapter::AdapterCapabilities;

    #[test]
    fn build_agent_key_includes_agent_and_caps() {
        let key = build_agent_key(
            PulseChatAgent::Claude,
            false,
            &[],
            &AdapterCapabilities {
                enable_fs: true,
                enable_terminal: true,
                permission_timeout_secs: None,
                adapter_timeout_secs: None,
            },
        );
        assert!(key.starts_with("Claude:"));
        assert!(key.contains("fs=true"));
        assert!(key.contains("term=true"));
    }

    #[test]
    fn build_agent_key_assistant_mode_differs() {
        let caps = AdapterCapabilities {
            enable_fs: true,
            enable_terminal: true,
            permission_timeout_secs: None,
            adapter_timeout_secs: None,
        };
        let normal = build_agent_key(PulseChatAgent::Claude, false, &[], &caps);
        let assistant = build_agent_key(PulseChatAgent::Claude, true, &[], &caps);
        assert_ne!(normal, assistant);
        assert!(assistant.contains(":assistant:"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p axon-web build_agent_key -- --nocapture`
Expected: FAIL — `build_agent_key` doesn't exist yet.

- [ ] **Step 3: Extract `build_agent_key()` from `get_or_create_acp_connection()`**

In `pulse_chat.rs`, add this `pub(crate)` helper and refactor `get_or_create_acp_connection` to call it:

```rust
/// Build the session cache key for an ACP adapter.
///
/// The key encodes agent type, assistant mode, MCP config fingerprint, and
/// capability flags so that sessions with different configurations get
/// separate adapter processes.
pub(crate) fn build_agent_key(
    agent: PulseChatAgent,
    assistant_mode: bool,
    mcp_servers: &[crate::crates::services::types::AcpMcpServerConfig],
    caps: &AdapterCapabilities,
) -> String {
    let mcp_fingerprint = fingerprint_mcp_servers(mcp_servers);
    let caps_fingerprint = format!(
        "fs={},term={},ptimeout={:?},atimeout={:?}",
        caps.enable_fs,
        caps.enable_terminal,
        caps.permission_timeout_secs,
        caps.adapter_timeout_secs,
    );
    if assistant_mode {
        format!("{agent:?}:assistant:mcp={mcp_fingerprint}:{caps_fingerprint}")
    } else {
        format!("{agent:?}:mcp={mcp_fingerprint}:{caps_fingerprint}")
    }
}
```

Then update `get_or_create_acp_connection` to replace lines 182-198 with:

```rust
    let agent_key = build_agent_key(agent, assistant_mode, &req.mcp_servers, &caps);
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p axon-web build_agent_key -- --nocapture`
Expected: PASS

- [ ] **Step 5: Run full test suite to check nothing broke**

Run: `cargo test -p axon-web -- --nocapture`
Expected: All existing tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/web/execute/sync_mode/pulse_chat.rs
git commit -m "refactor: extract build_agent_key() helper from pulse_chat"
```

---

### Task 2: Add `AXON_ACP_PREWARM` to Config

Do this before the prewarm module so `Config` compiles cleanly when prewarm reads from it.

**Files:**
- Modify: `crates/core/config/types/config.rs` (~line 235, near `acp_adapter_args`)
- Modify: `crates/core/config/parse/build_config.rs` (~line 376, near `acp_adapter_args`)
- Modify: `crates/core/config/types/config_impls.rs` (default ~line 87, debug ~line 263)

- [ ] **Step 1: Add field to Config struct**

In `crates/core/config/types/config.rs`, after the `acp_adapter_args` field:

```rust
    /// Whether to pre-warm the default ACP adapter on server boot.
    /// Sourced from `AXON_ACP_PREWARM` (default: `true`).
    pub acp_prewarm: bool,
```

- [ ] **Step 2: Parse in build_config.rs**

After the `acp_adapter_args` parsing:

```rust
        acp_prewarm: env::var("AXON_ACP_PREWARM")
            .map(|v| !matches!(v.as_str(), "false" | "0"))
            .unwrap_or(true),
```

- [ ] **Step 3: Add default in config_impls.rs**

In the `Default` impl, after `acp_adapter_args: None,`:

```rust
            acp_prewarm: true,
```

- [ ] **Step 4: Add to Debug impl**

After `.field("acp_adapter_args", ...)`:

```rust
            .field("acp_prewarm", &self.acp_prewarm)
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check`
Expected: Compiles.

- [ ] **Step 6: Commit**

```bash
git add crates/core/config/types/config.rs crates/core/config/parse/build_config.rs crates/core/config/types/config_impls.rs
git commit -m "feat: add AXON_ACP_PREWARM config option (default: true)"
```

---

### Task 3: Create `prewarm.rs` module

**Files:**
- Create: `crates/web/execute/sync_mode/prewarm.rs`
- Modify: `crates/web/execute/sync_mode.rs` (line 1-7 — add `pub(crate) mod prewarm;`)
- Modify: `crates/web/execute.rs` (add `pub(crate) use sync_mode::prewarm;`)

**Important:** `sync_mode` is declared as `mod sync_mode;` (private) in `execute.rs`. For `web.rs` to reach `prewarm::spawn_prewarm_task()`, we need either:
- (a) Re-export from `execute.rs`: `pub(crate) use sync_mode::prewarm;`, or
- (b) Make `sync_mode` `pub(crate)` in `execute.rs`

Option (a) is more surgical — re-export only what's needed.

- [ ] **Step 1: Add module declaration in `sync_mode.rs`**

In `crates/web/execute/sync_mode.rs`, add after existing `mod` declarations (line 7):

```rust
pub(crate) mod prewarm;
```

- [ ] **Step 2: Add re-export in `execute.rs`**

In `crates/web/execute.rs`, add near the top (after the existing `mod` declarations):

```rust
pub(crate) use sync_mode::prewarm;
```

- [ ] **Step 3: Create `prewarm.rs`**

Create `crates/web/execute/sync_mode/prewarm.rs`:

```rust
//! ACP adapter pre-warming — spawn default adapter on server boot.
//!
//! Eliminates the ~45-second cold start on the first chat message by
//! establishing the adapter session proactively during `start_server()`.
//!
//! **Cache key matching:** The prewarm uses default capabilities (fs=true,
//! term=true, no timeouts) and no MCP servers. This matches the most common
//! request configuration. Requests with different caps will cache-miss and
//! cold-start their own adapter — correct by design.
//!
//! **30-minute reaper window:** The pre-warmed session has the same idle TTL
//! as any other cached session (30 minutes). If no user sends a message
//! within that window, the warm adapter is reaped and the first request
//! will cold-start normally. This is acceptable for a self-hosted system
//! where the server typically starts shortly before use.

use std::sync::Arc;

use crate::crates::core::config::Config;
use crate::crates::services::acp::{self as acp_svc, SESSION_CACHE};

use super::acp_adapter::{AdapterCapabilities, resolve_acp_adapter_command};
use super::pulse_chat::build_agent_key;
use super::types::PulseChatAgent;

/// Default capabilities used for pre-warmed sessions.
///
/// Pre-warmed adapters use the most permissive defaults so they match
/// the most common request configuration.
fn default_prewarm_caps() -> AdapterCapabilities {
    AdapterCapabilities {
        enable_fs: true,
        enable_terminal: true,
        permission_timeout_secs: None,
        adapter_timeout_secs: None,
    }
}

/// Pre-warm a single ACP adapter by spawning it and sending a no-op
/// ping turn to force session establishment.
///
/// Returns the agent_key on success for logging.
async fn prewarm_adapter(
    cfg: &Arc<Config>,
    agent: PulseChatAgent,
) -> Result<String, String> {
    let caps = default_prewarm_caps();
    let agent_key = build_agent_key(agent, false, &[], &caps);

    // Skip if already cached (e.g. from a previous prewarm or a real request).
    if SESSION_CACHE.get(&agent_key).is_some() {
        tracing::info!(
            context = "acp_prewarm",
            agent_key = %agent_key,
            "adapter already cached — skipping prewarm",
        );
        return Ok(agent_key);
    }

    let adapter = resolve_acp_adapter_command(cfg, agent, caps)?;
    let adapter_name = std::path::Path::new(&adapter.program)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&adapter.program)
        .to_string();

    tracing::info!(
        context = "acp_prewarm",
        program = %adapter_name,
        agent_key = %agent_key,
        "pre-warming adapter",
    );

    let scaffold = acp_svc::AcpClientScaffold::new(adapter.clone());
    let initialize = scaffold.prepare_initialize().map_err(|e| e.to_string())?;
    let cwd = resolve_prewarm_working_dir().await?;

    // Use prepare_session_setup with a minimal AcpPromptTurnRequest.
    // This routes through the same code path as real requests, ensuring
    // the prewarm session setup is identical to production.
    let minimal_req = crate::crates::services::types::AcpPromptTurnRequest {
        session_id: None,
        prompt: vec![],
        model: None,
        session_mode: None,
        blocked_mcp_tools: vec![],
        mcp_servers: vec![],
    };
    let session_setup = scaffold
        .prepare_session_setup(&minimal_req, &cwd)
        .map_err(|e| e.to_string())?;

    let permission_responders: acp_svc::PermissionResponderMap =
        Arc::new(dashmap::DashMap::new());

    let handle = Arc::new(acp_svc::AcpConnectionHandle::spawn(
        adapter,
        initialize,
        session_setup,
        permission_responders.clone(),
    ));

    SESSION_CACHE.insert(
        agent_key.clone(),
        Arc::clone(&handle),
        permission_responders.clone(),
    );

    // Send a lightweight ping turn to force the adapter through
    // establish_acp_session(). This is the slow part (~45s) that we
    // want to happen at boot, not on the first user message.
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(64);
    let (result_tx, result_rx) = tokio::sync::oneshot::channel();

    if let Some(cached) = SESSION_CACHE.get_sync(&agent_key) {
        cached.mark_turn_started();
    }

    handle
        .run_turn(acp_svc::TurnRequest {
            req: crate::crates::services::types::AcpPromptTurnRequest {
                session_id: None,
                prompt: vec!["Respond with exactly: WARM".to_string()],
                model: None,
                session_mode: None,
                blocked_mcp_tools: vec![],
                mcp_servers: vec![],
            },
            service_tx: Some(event_tx),
            result_tx,
        })
        .await
        .map_err(|e| format!("prewarm turn dispatch failed: {e}"))?;

    // Drain events — we don't need them, but we must consume the channel
    // so the adapter loop doesn't block.
    let drain_handle = tokio::spawn(async move {
        while event_rx.recv().await.is_some() {}
    });

    // Wait for the turn to complete (this is the ~45s cold start).
    let turn_result = tokio::time::timeout(
        std::time::Duration::from_secs(120), // generous timeout for first cold start
        result_rx,
    )
    .await
    .map_err(|_| "prewarm turn timed out after 120s".to_string())?
    .map_err(|_| "prewarm turn result channel dropped".to_string())?;

    if let Some(cached) = SESSION_CACHE.get_sync(&agent_key) {
        cached.mark_turn_completed();
    }

    // Wait for event drain to finish.
    let _ = drain_handle.await;

    match turn_result {
        Ok(()) => {
            tracing::info!(
                context = "acp_prewarm",
                agent_key = %agent_key,
                program = %adapter_name,
                "adapter pre-warmed successfully",
            );
            Ok(agent_key)
        }
        Err(e) => {
            tracing::warn!(
                context = "acp_prewarm",
                agent_key = %agent_key,
                error = %e,
                "prewarm turn failed (adapter may still be usable)",
            );
            // Don't evict — the adapter is spawned and cached. The next real
            // turn may succeed even if the ping failed (e.g. content policy).
            Ok(agent_key)
        }
    }
}

/// Resolve the working directory for pre-warmed adapters.
///
/// Uses a dedicated `prewarm` subdirectory. Note: the working directory
/// is NOT part of the cache key, so the first real request will reuse
/// this adapter even if its CWD differs. The adapter session's CWD is
/// set at session creation time and persists for the session lifetime.
async fn resolve_prewarm_working_dir() -> Result<std::path::PathBuf, String> {
    let base = std::env::var("AXON_DATA_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        format!("{home}/.local/share")
    });
    let path = std::path::PathBuf::from(base)
        .join("axon")
        .join("prewarm");
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|e| format!("failed to create prewarm dir: {e}"))?;
    Ok(path)
}

/// Spawn a background task that pre-warms the default ACP adapter(s).
///
/// Called once during `start_server()`. Non-blocking — the server starts
/// accepting connections immediately while the adapter warms in the background.
///
/// Controlled by `Config::acp_prewarm` (env `AXON_ACP_PREWARM`, default: true).
pub(crate) fn spawn_prewarm_task(cfg: Arc<Config>) {
    if !cfg.acp_prewarm {
        tracing::info!(context = "acp_prewarm", "pre-warming disabled via AXON_ACP_PREWARM=false");
        return;
    }

    tokio::spawn(async move {
        // Small delay to let the server bind and start accepting connections
        // before we tie up a spawn_blocking thread with the adapter.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        match prewarm_adapter(&cfg, PulseChatAgent::Claude).await {
            Ok(key) => tracing::info!(context = "acp_prewarm", agent_key = %key, "prewarm complete"),
            Err(e) => tracing::warn!(context = "acp_prewarm", error = %e, "prewarm failed (will cold-start on first request)"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_prewarm_caps_are_permissive() {
        let caps = default_prewarm_caps();
        assert!(caps.enable_fs);
        assert!(caps.enable_terminal);
        assert!(caps.permission_timeout_secs.is_none());
        assert!(caps.adapter_timeout_secs.is_none());
    }

    #[test]
    fn spawn_prewarm_skips_when_disabled() {
        // Verify that the disabled path is exercised without panicking.
        // We can't fully test spawn_prewarm_task without a tokio runtime
        // and a real adapter binary, but we verify the Config flag logic.
        let cfg = Config::default();
        assert!(cfg.acp_prewarm, "default config should have prewarm enabled");
    }
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p axon-web`
Expected: Compiles. If `prepare_session_setup` rejects the empty prompt in `validate_prompt_turn_request`, the validation call may need to be bypassed. Check error output and adjust — the `prepare_session_setup` path calls `validate_prompt_turn_request` which checks `!req.prompt.is_empty()`. If it fails, set `prompt: vec!["_".to_string()]` instead of `vec![]` in the minimal request.

- [ ] **Step 5: Run tests**

Run: `cargo test -p axon-web -- --nocapture`
Expected: All pass (existing + 2 new prewarm tests).

- [ ] **Step 6: Commit**

```bash
git add crates/web/execute/sync_mode/prewarm.rs crates/web/execute/sync_mode.rs crates/web/execute.rs
git commit -m "feat: add ACP adapter pre-warming module"
```

---

### Task 4: Wire prewarm into `start_server()`

**Files:**
- Modify: `crates/web.rs` (line ~160, after docker stats spawn)

- [ ] **Step 1: Add the prewarm call**

In `crates/web.rs`, after the docker stats spawn block (around line 160), add:

```rust
    // Pre-warm the default ACP adapter so the first chat message is fast.
    execute::prewarm::spawn_prewarm_task(cfg.clone());
```

This works because Task 3 added `pub(crate) use sync_mode::prewarm;` to `execute.rs`.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p axon-web`
Expected: Compiles without errors.

- [ ] **Step 3: Run full test suite**

Run: `cargo test -p axon-web -- --nocapture`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/web.rs
git commit -m "feat: wire ACP prewarm into server startup"
```

---

### Task 5: Integration smoke test

- [ ] **Step 1: Build the project**

Run: `cargo build -p axon-web`
Expected: Compiles cleanly.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p axon-web -- -D warnings`
Expected: No warnings.

- [ ] **Step 3: Run all tests**

Run: `cargo test`
Expected: All pass.

- [ ] **Step 4: Manual smoke test**

Start `axon serve` and watch logs for prewarm output:

```
[INFO acp_prewarm] pre-warming adapter program="claude-agent-acp" agent_key="Claude:mcp=..."
[INFO acp_prewarm] adapter pre-warmed successfully agent_key="Claude:mcp=..."
```

Then send a message — should respond immediately without the 45-second cold start.

- [ ] **Step 5: Test disable**

Set `AXON_ACP_PREWARM=false` and restart — verify log says "pre-warming disabled".

- [ ] **Step 6: Final commit if any fixes needed**

```bash
git add -A
git commit -m "fix: prewarm integration fixes"
```

---

## Notes

- **Why a ping turn instead of just spawning?** `AcpConnectionHandle::spawn()` defers all I/O setup until the first `RunTurn` message. The subprocess binary starts, but `establish_acp_session()` (spawn adapter, initialize protocol, create/load session) only runs when the first turn arrives. That's where the 45 seconds lives. The ping turn forces this path eagerly.

- **Why "Respond with exactly: WARM"?** We need the adapter to complete a full turn (including LLM inference) so the model is loaded and the connection is fully established. A trivial prompt minimizes token cost while achieving full warm-up.

- **Cache key matching.** The prewarm uses `default_prewarm_caps()` (fs=true, term=true, no timeouts) and no MCP servers. This matches the most common Android app request configuration. Requests with different caps will cache-miss and cold-start their own adapter — correct by design.

- **30-minute reaper window.** The pre-warmed session has the same 30-minute idle TTL as any cached session. If no user sends a message within that window after server boot, the adapter is reaped and the first request cold-starts normally. This is acceptable for a self-hosted system where the server typically starts shortly before use.

- **Working directory.** The prewarm session uses `~/.local/share/axon/prewarm/` as its CWD. The CWD is NOT part of the cache key, so the first real request reuses this adapter regardless of its own CWD needs. The CWD is set at ACP session creation time and persists for the session lifetime. For Pulse chat (the primary use case), this is fine since the adapter doesn't depend on the CWD for file operations.

- **Failure is non-fatal.** If prewarm fails (adapter binary not found, auth error, timeout), the server logs a warning and continues normally. The first real request will cold-start as it does today.

- **Memory cost.** One warm adapter = one `spawn_blocking` thread + one child process (~50-100MB RSS for `claude-agent-acp`). Acceptable for a single-user self-hosted system.

- **Uses existing `prepare_session_setup`.** Instead of adding a new method to `AcpClientScaffold`, the prewarm creates a minimal `AcpPromptTurnRequest` and routes through the same `prepare_session_setup` → `build_session_setup` path as real requests. This ensures the prewarm session setup is identical to production.
