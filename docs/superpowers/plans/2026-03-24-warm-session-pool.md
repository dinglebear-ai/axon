# Warm ACP Session Pool Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a process-global pool of pre-warmed ACP adapter sessions so `axon serve` and `axon mcp` never pay the 5–15s adapter binary cold-start tax on LLM requests.

**Architecture:** A `WarmSessionPool` in `crates/services/acp_llm/pool.rs` holds a minimum of 1 pre-warmed `WarmAcpSession`. The existing `warm_session()` function gains pool awareness — it checks the pool first, falls back to one-shot spawn if the pool is empty or uninitialized. The pool is initialized explicitly at `axon serve` and `axon mcp` startup; CLI commands (`axon ask`, etc.) run as short-lived processes that never initialize the pool and continue using the existing overlap-with-retrieval warm pattern unchanged.

**Tech Stack:** Rust, tokio, `std::sync::{Mutex, OnceLock}`, `std::collections::VecDeque`

---

## Background / Context

Read these before starting:
- `crates/services/acp_llm.rs` — module root, re-exports `warm_session`
- `crates/services/acp_llm/warm.rs` — `WarmAcpSession`, `warm_session()` implementation
- `crates/cli/commands/serve.rs` — where pool init must be added
- `crates/cli/commands/mcp.rs` — where pool init must be added

**What `warm_session()` currently does:**
1. Resolves the adapter command from config
2. Calls `AcpConnectionHandle::spawn_eager()` on a background thread
3. Returns immediately with a `WarmAcpSession` whose adapter is still initializing
4. The adapter process finishes init in the background while the caller does other work

**What the pool adds:** Instead of creating a fresh `WarmAcpSession` on each request, checkout one that was created at server startup (adapter already initialized). On checkout, the pool immediately spawns a replacement so the next request doesn't wait.

**Monolith limits:** Files ≤ 500 lines, functions warn at 80 lines / hard-fail at 120 lines. `pool.rs` should be comfortably under both limits.

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| **Create** | `crates/services/acp_llm/pool.rs` | `WarmSessionPool` struct, global `OnceLock`, `init()`, `try_checkout()` |
| **Modify** | `crates/services/acp_llm/warm.rs` | Split into public `warm_session()` (pool-aware) + private `spawn_warm_session()` (pool-bypassing, used by pool refill) |
| **Modify** | `crates/services/acp_llm.rs` | Add `mod pool; pub use pool::init_warm_pool;` |
| **Modify** | `crates/cli/commands/serve.rs` | Call `acp_llm::init_warm_pool(cfg)` before starting the axum server |
| **Modify** | `crates/cli/commands/mcp.rs` | Call `acp_llm::init_warm_pool(cfg)` before starting the MCP server |

---

## Task 1: Create `pool.rs` with `WarmSessionPool` (TDD)

**Files:**
- Create: `crates/services/acp_llm/pool.rs`

### What to build

A pool that:
1. Holds a `VecDeque<(WarmAcpSession, std::time::Instant)>` protected by `std::sync::Mutex`
2. Tracks the config key `(adapter_cmd, model)` so a stale config (env change) invalidates the pool
3. Has a 10-minute idle TTL: sessions older than `MAX_IDLE_SECS` are dropped on checkout instead of returned
4. Exposes three functions: `init(cfg)`, `try_checkout(cfg) -> Option<WarmAcpSession>`, `pool_size() -> usize`

### Unit tests to write first

The pool's pure logic (config key mismatch, TTL eviction) can be tested without spawning real ACP processes. Test only these properties; do not write integration tests that require a live adapter.

- [ ] **Step 1: Write failing tests in `pool.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_key(cmd: &str, model: &str) -> CfgKey {
        CfgKey {
            adapter_cmd: cmd.to_string(),
            model: model.to_string(),
        }
    }

    #[test]
    fn cfg_key_matches_equal_configs() {
        let k1 = make_key("codex", "gpt-4o");
        let k2 = make_key("codex", "gpt-4o");
        assert_eq!(k1, k2);
    }

    #[test]
    fn cfg_key_differs_on_model_change() {
        let k1 = make_key("codex", "gpt-4o");
        let k2 = make_key("codex", "gpt-4o-mini");
        assert_ne!(k1, k2);
    }

    #[test]
    fn cfg_key_differs_on_adapter_change() {
        let k1 = make_key("codex", "gpt-4o");
        let k2 = make_key("claude", "gpt-4o");
        assert_ne!(k1, k2);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test pool -- --nocapture
```

Expected: compilation error (`CfgKey` does not exist yet).

- [ ] **Step 3: Implement `pool.rs`**

```rust
//! Process-global pool of pre-warmed ACP adapter sessions.
//!
//! Initialized once at server startup via [`init`]. CLI commands that run as
//! short-lived processes never call `init`; for them [`try_checkout`] returns
//! `None` and callers fall back to one-shot [`super::spawn_warm_session`].

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::crates::core::config::Config;
use crate::crates::core::logging::log_warn;

use super::WarmAcpSession;

/// How long a pooled session can sit idle before it is considered stale and
/// dropped on the next checkout. The underlying adapter process may have timed
/// out or been killed by the OS after this long without a prompt.
const MAX_IDLE: Duration = Duration::from_secs(600); // 10 minutes

/// Minimum number of warm sessions the pool tries to maintain.
const MIN_SIZE: usize = 1;

/// Config identity key. If the adapter command or model changes after pool init,
/// `try_checkout` returns `None` and falls back to one-shot cold start.
#[derive(Debug, PartialEq, Eq, Clone)]
pub(super) struct CfgKey {
    pub adapter_cmd: String,
    pub model: String,
}

impl CfgKey {
    fn from_config(cfg: &Config) -> Self {
        Self {
            adapter_cmd: cfg
                .acp_adapter_cmd
                .as_deref()
                .unwrap_or_default()
                .to_string(),
            model: cfg.openai_model.clone(),
        }
    }
}

struct PooledSession {
    session: WarmAcpSession,
    inserted_at: Instant,
}

struct WarmSessionPool {
    sessions: Mutex<VecDeque<PooledSession>>,
    cfg_key: CfgKey,
}

impl WarmSessionPool {
    fn new(cfg: &Config) -> Self {
        Self {
            sessions: Mutex::new(VecDeque::new()),
            cfg_key: CfgKey::from_config(cfg),
        }
    }

    fn push(&self, session: WarmAcpSession) {
        let mut guard = self.sessions.lock().expect("pool mutex poisoned");
        guard.push_back(PooledSession {
            session,
            inserted_at: Instant::now(),
        });
    }

    fn pop_fresh(&self) -> Option<(WarmAcpSession, Duration)> {
        let mut guard = self.sessions.lock().expect("pool mutex poisoned");
        while let Some(entry) = guard.pop_front() {
            let age = entry.inserted_at.elapsed();
            if age <= MAX_IDLE {
                return Some((entry.session, age));
            }
            // Session is stale — drop it and try the next one.
        }
        None
    }

    fn len(&self) -> usize {
        self.sessions.lock().expect("pool mutex poisoned").len()
    }
}

static WARM_POOL: OnceLock<WarmSessionPool> = OnceLock::new();

/// Initialize the warm session pool at server startup.
///
/// Safe to call multiple times — only the first call takes effect (OnceLock).
/// Should be called before the HTTP/MCP server starts accepting requests.
/// No-op when `AXON_ACP_ADAPTER_CMD` is not set in `cfg`.
pub fn init(cfg: &Config) {
    if cfg.acp_adapter_cmd.as_deref().map_or(true, str::is_empty) {
        return; // ACP not configured — pool not useful
    }
    let pool = WARM_POOL.get_or_init(|| WarmSessionPool::new(cfg));
    // Fill to MIN_SIZE on startup. spawn_warm_session is synchronous and
    // returns immediately; the actual adapter init runs on a background thread.
    let current = pool.len();
    for _ in current..MIN_SIZE {
        let t = Instant::now();
        match super::spawn_warm_session(cfg, None) {
            Ok(session) => {
                log_info(&format!(
                    "warm pool: pre-warmed session spawn={}ms",
                    t.elapsed().as_millis()
                ));
                pool.push(session);
            }
            Err(e) => log_warn(&format!("warm pool: failed to pre-warm session: {e}")),
        }
    }
}

/// Try to check out a warm session from the pool.
///
/// Returns `None` when:
/// - The pool was never initialized (CLI process, short-lived invocation)
/// - The pool is empty (all sessions in use, refill in progress)
/// - The config key changed since pool init (adapter cmd or model changed)
/// - The most recent session has been idle > 10 minutes (considered stale)
///
/// After a successful checkout, schedules a background refill.
pub fn try_checkout(cfg: &Config) -> Option<WarmAcpSession> {
    let pool = WARM_POOL.get()?;

    // If config changed since pool init, don't return a mismatched session.
    if pool.cfg_key != CfgKey::from_config(cfg) {
        return None;
    }

    let (session, age) = pool.pop_fresh()?;
    let remaining = pool.len();
    log_info(&format!(
        "warm pool: checkout age={}ms pool_remaining={}",
        age.as_millis(),
        remaining
    ));

    // Spawn a replacement immediately so the pool is refilled for the next request.
    let cfg_clone = cfg.clone();
    tokio::task::spawn_blocking(move || {
        let t = Instant::now();
        match super::spawn_warm_session(&cfg_clone, None) {
            Ok(new_session) => {
                log_info(&format!(
                    "warm pool: refill spawn={}ms",
                    t.elapsed().as_millis()
                ));
                if let Some(p) = WARM_POOL.get() {
                    p.push(new_session);
                }
            }
            Err(e) => log_warn(&format!("warm pool: background refill failed: {e}")),
        }
    });

    Some(session)
}

/// Current number of sessions available in the pool (test / health use only).
pub fn pool_size() -> usize {
    WARM_POOL.get().map_or(0, WarmSessionPool::len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key(cmd: &str, model: &str) -> CfgKey {
        CfgKey {
            adapter_cmd: cmd.to_string(),
            model: model.to_string(),
        }
    }

    #[test]
    fn cfg_key_matches_equal_configs() {
        let k1 = make_key("codex", "gpt-4o");
        let k2 = make_key("codex", "gpt-4o");
        assert_eq!(k1, k2);
    }

    #[test]
    fn cfg_key_differs_on_model_change() {
        let k1 = make_key("codex", "gpt-4o");
        let k2 = make_key("codex", "gpt-4o-mini");
        assert_ne!(k1, k2);
    }

    #[test]
    fn cfg_key_differs_on_adapter_change() {
        let k1 = make_key("codex", "gpt-4o");
        let k2 = make_key("claude", "gpt-4o");
        assert_ne!(k1, k2);
    }

    #[test]
    fn pool_size_zero_before_init() {
        // WARM_POOL may already be set in other tests — just verify the function
        // returns a usize without panicking.
        let _ = pool_size();
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test pool -- --nocapture
```

Expected: 4 tests pass. (pool_size test may vary if pool was init'd elsewhere; that's fine.)

- [ ] **Step 5: Compile check**

```bash
cargo check --lib 2>&1 | tail -5
```

Expected: `Finished` with no errors. There will be a compiler error about `super::spawn_warm_session` not existing — that's fine, it's addressed in Task 2.

---

## Task 2: Split `warm.rs` — add `spawn_warm_session` + pool-aware `warm_session`

**Files:**
- Modify: `crates/services/acp_llm/warm.rs`

The goal: the existing `warm_session()` function gains pool awareness (checks pool first). A new private `spawn_warm_session()` does what `warm_session()` currently does, without any pool involvement. The pool refill calls `spawn_warm_session()` directly to avoid circular calls.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` block in `warm.rs` (or create one):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::Config;

    #[test]
    fn warm_session_fails_without_adapter_cmd() {
        let cfg = Config {
            acp_adapter_cmd: None,
            openai_model: "gpt-4o".to_string(),
            ..Config::default()
        };
        let result = warm_session(&cfg, None);
        assert!(result.is_err(), "warm_session must fail when ACP adapter cmd is not set");
    }
}
```

- [ ] **Step 2: Run test to verify it passes (it should already pass)**

```bash
cargo test warm_session_fails_without_adapter_cmd -- --nocapture
```

This verifies the existing behavior is preserved after the refactor.

- [ ] **Step 3: Refactor `warm.rs` — extract `spawn_warm_session`**

Rename the body of the current `warm_session` function to `spawn_warm_session`, then rewrite `warm_session` to check the pool first:

```rust
/// Start warming an ACP adapter session in the background.
///
/// Checks the process-global warm pool first. If the pool has a ready session,
/// returns it immediately (no subprocess spawn). Otherwise falls back to a new
/// one-shot spawn via [`spawn_warm_session`].
///
/// For callers passing an event channel (`tx.is_some()`), pool sessions are
/// bypassed because they were created without event forwarding.
pub fn warm_session(
    cfg: &Config,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<WarmAcpSession, Box<dyn StdError>> {
    if tx.is_none() {
        if let Some(session) = super::pool::try_checkout(cfg) {
            return Ok(session);
        }
    }
    spawn_warm_session(cfg, tx)
}

/// Spawn a fresh warm ACP session without consulting the pool.
///
/// Used internally by the pool to refill without risking circular calls.
/// External callers should prefer [`warm_session`].
pub(super) fn spawn_warm_session(
    cfg: &Config,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<WarmAcpSession, Box<dyn StdError>> {
    // ... exact same body as the current warm_session function ...
    let adapter = resolve_adapter_command(cfg)?;
    let scaffold = AcpClientScaffold::new(adapter.clone());
    let initialize = scaffold.prepare_initialize()?;
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let model = if cfg.openai_model.trim().is_empty() {
        None
    } else {
        Some(cfg.openai_model.clone())
    };
    let dummy_req = AcpPromptTurnRequest {
        session_id: None,
        prompt: vec!["__warm__".to_string()],
        model: model.clone(),
        session_mode: None,
        blocked_mcp_tools: vec![],
        mcp_servers: vec![],
    };
    let session_setup = scaffold.prepare_session_setup(&dummy_req, &cwd)?;
    let permission_responders: PermissionResponderMap = Arc::new(dashmap::DashMap::new());
    let t = std::time::Instant::now();
    let handle = AcpConnectionHandle::spawn_eager(
        adapter,
        initialize,
        session_setup,
        model,
        tx,
        permission_responders,
    );
    log_info(&format!(
        "acp_llm: spawn_eager returned in {}ms (adapter init continues in background)",
        t.elapsed().as_millis()
    ));
    Ok(WarmAcpSession { handle })
}
```

- [ ] **Step 4: Run all acp_llm tests**

```bash
cargo test acp_llm -- --nocapture
cargo check --lib 2>&1 | tail -5
```

Expected: all existing tests pass, no compile errors.

- [ ] **Step 5: Commit**

```bash
git add crates/services/acp_llm/pool.rs crates/services/acp_llm/warm.rs
git commit -m "feat(acp_llm): add WarmSessionPool + pool-aware warm_session"
```

---

## Task 3: Wire pool into `acp_llm.rs` module root

**Files:**
- Modify: `crates/services/acp_llm.rs`

- [ ] **Step 1: Add `mod pool` and re-export `init_warm_pool`**

Open `crates/services/acp_llm.rs`. Add the pool module declaration and re-export:

```rust
mod pool;
mod runner;
mod types;
mod warm;

pub use pool::init_warm_pool;    // ← add this line
pub use types::{
    AcpCompletionRequest, AcpCompletionResponse, AcpCompletionRunner, AcpCompletionTurnResult,
    AcpUsageSnapshot, extract_completion_result, normalize_stream_flag,
};
pub use warm::{WarmAcpSession, warm_session};
```

Also add `init_warm_pool` as a public alias in `pool.rs`:

```rust
// At module level in pool.rs, add an alias so the re-export name is clear:
pub use self::init as init_warm_pool;
```

Or rename the function to `init_warm_pool` in `pool.rs` directly. Consistent naming is cleaner — rename it.

- [ ] **Step 2: Compile check**

```bash
cargo check --lib 2>&1 | tail -5
```

Expected: `Finished` with no errors.

- [ ] **Step 3: Commit**

```bash
git add crates/services/acp_llm.rs crates/services/acp_llm/pool.rs
git commit -m "feat(acp_llm): re-export init_warm_pool from module root"
```

---

## Task 4: Initialize pool in `axon serve`

**Files:**
- Modify: `crates/cli/commands/serve.rs`

`serve.rs` is currently 5 lines. The pool init goes before the server starts.

- [ ] **Step 1: Write a failing test**

```rust
// In serve.rs, add a test that documents the init is not a no-op when configured:
#[cfg(test)]
mod tests {
    use crate::crates::services::acp_llm;

    #[test]
    fn pool_size_before_init_is_zero() {
        // Verify pool_size() works without panicking before any init.
        // Real pool may have been initialized by another test — just check no panic.
        let _ = acp_llm::pool::pool_size();
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

```bash
cargo test pool_size_before_init_is_zero -- --nocapture
```

- [ ] **Step 3: Add pool init to `run_serve`**

```rust
use crate::crates::core::config::Config;
use crate::crates::services::acp_llm;
use std::error::Error;
use std::sync::Arc;

pub async fn run_serve(cfg: &Config) -> Result<(), Box<dyn Error>> {
    // Pre-warm the ACP adapter session pool so the first LLM request from
    // the web UI doesn't pay the 5–15s adapter binary cold-start tax.
    acp_llm::init_warm_pool(cfg);
    crate::crates::web::start_server(cfg.serve_port, Arc::new(cfg.clone())).await
}
```

- [ ] **Step 4: Compile check**

```bash
cargo check --lib 2>&1 | tail -5
```

- [ ] **Step 5: Commit**

```bash
git add crates/cli/commands/serve.rs
git commit -m "feat(serve): initialize warm ACP session pool on startup"
```

---

## Task 5: Initialize pool in `axon mcp`

**Files:**
- Modify: `crates/cli/commands/mcp.rs`

- [ ] **Step 1: Add pool init to `run_mcp`**

```rust
use crate::crates::core::config::{Config, McpTransport};
use crate::crates::services::acp_llm;
use std::error::Error;

pub async fn run_mcp(cfg: &Config) -> Result<(), Box<dyn Error>> {
    // Pre-warm the ACP adapter session pool. MCP ask/evaluate/suggest calls
    // will check out a warm session instead of paying cold-start per request.
    acp_llm::init_warm_pool(cfg);

    match cfg.mcp_transport {
        McpTransport::Stdio => crate::crates::mcp::run_stdio_server().await,
        McpTransport::Http => {
            crate::crates::mcp::run_http_server(&cfg.mcp_http_host, cfg.mcp_http_port).await
        }
        McpTransport::Both => {
            let host = cfg.mcp_http_host.clone();
            let port = cfg.mcp_http_port;
            tokio::try_join!(
                crate::crates::mcp::run_stdio_server(),
                crate::crates::mcp::run_http_server(&host, port),
            )?;
            Ok(())
        }
    }
}
```

Note: For MCP stdio transport, stdout is the JSON-RPC protocol pipe. `init_warm_pool` does not write to stdout (it uses `log_warn` which goes to stderr), so this is safe.

- [ ] **Step 2: Compile check + existing tests**

```bash
cargo check --lib 2>&1 | tail -5
cargo test mcp -- --nocapture
```

Expected: all existing MCP tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/cli/commands/mcp.rs
git commit -m "feat(mcp): initialize warm ACP session pool on startup"
```

---

## Task 6: End-to-end verification

No new files. Verify the integration is correct and the pool path is exercised.

- [ ] **Step 1: Run full test suite**

```bash
cargo test --lib 2>&1 | tail -20
```

Expected: all tests pass. Count should be ≥ 1560 (baseline from prior session).

- [ ] **Step 2: Run clippy**

```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -20
```

Expected: no warnings.

- [ ] **Step 3: Verify `pool_size()` is exported correctly**

```bash
cargo test pool -- --nocapture
```

Expected: 4 pool tests pass.

- [ ] **Step 4: Verify the ask path compiles with warm session call tracing**

```bash
cargo check --bin axon 2>&1 | tail -5
```

Expected: `Finished` with no errors.

- [ ] **Step 5: Manual smoke test (if ACP adapter is configured)**

```bash
# Start serve with a configured adapter
./scripts/axon serve &
sleep 2

# First ask — should use pool session (adapter already initialized)
time ./scripts/axon ask "what is axon?" 2>&1

# Expected: tokens stream to stdout progressively (streaming fix from this session)
# Expected: first-token latency noticeably faster than cold start
```

- [ ] **Step 6: Final commit**

```bash
cargo fmt
git add -A
git commit -m "feat(acp_llm): warm session pool for serve and mcp — eliminates cold-start per request"
```

---

## Notes for the Implementer

### `spawn_blocking` vs `tokio::spawn` for pool refill

`spawn_warm_session()` is synchronous (returns immediately, adapter runs on background thread via `spawn_blocking` inside `AcpConnectionHandle::spawn_eager`). The pool refill can use `tokio::task::spawn_blocking` or `tokio::spawn` — either works since the work is non-blocking. `tokio::spawn_blocking` is safer if `spawn_warm_session` ever becomes blocking internally.

### CLI commands are unaffected

`axon ask`, `axon evaluate`, `axon suggest`, `axon debug`, `axon research` are short-lived processes. They never call `init_warm_pool`, so `WARM_POOL` stays `None` for their lifetime. `warm_session()` calls `pool::try_checkout()` which returns `None` immediately (no pool), then falls back to `spawn_warm_session()`. Behavior is identical to before — the overlap-with-retrieval pattern still applies.

### OnceLock means pool init is idempotent

`OnceLock::get_or_init` ensures only the first `init_warm_pool` call takes effect. Safe to call multiple times (e.g., in tests or if serve and mcp are co-located).

### Session TTL is 10 minutes (`MAX_IDLE`)

A session created at server startup will be valid for 10 minutes while idle. After that, `pop_fresh()` drops it and returns `None`. The caller falls back to one-shot cold start, and the next request triggers a new pool refill. This prevents returning a session whose adapter process has died due to OS timeout or network failure.

### Config key validation

If `AXON_ACP_ADAPTER_CMD` or `OPENAI_MODEL` changes between pool init and a request (e.g., env var reload), `try_checkout` detects the key mismatch and returns `None`. The caller pays a cold start for that request. The pool is not re-initialized (it keeps its original sessions); the operator should restart the server to pick up config changes.
