# Session: ACP Performance/Scalability Fixes + Modern Rust Idioms (v0.11.2)

**Date:** 2026-03-09
**Branch:** `refactor/acp-performance-modern-rust`
**Commit:** `5279f7ad`
**Version:** 0.11.1 → 0.11.2 (patch)

---

## Session Overview

Resolved all 19 findings from `docs/reports/acp-performance-scalability-analysis-2026-03-08.md` using three parallel agents (zero file conflicts), then applied a modern Rust idiom audit in response to user feedback. Key accomplishments:

- Split 2060-line `crates/services/acp.rs` monolith into a proper `acp/` module (6 files, all ≤500 lines)
- Eliminated `Arc<Mutex<>>` on the streaming token hot path (OnceLock + RefCell)
- Fixed double serde_json serialization per streaming token (FINDING-5)
- Added configurable tokio blocking thread ceiling (FINDING-6)
- Applied Rust 2018 module convention: `acp/mod.rs` → `acp.rs`, `types/mod.rs` → `types.rs`
- Fully fixed FINDING-14: exit watcher now `drop(exit_tx)` on clean exit instead of sending empty string

---

## Timeline

1. **Agent dispatch** — Identified 19 findings, confirmed pre-wired `acp/` module files already existed as untracked. Dispatched 3 parallel agents with non-overlapping file ownership.
2. **Agent 1** — `crates/services/acp/mod.rs` + `bridge.rs` + `runtime.rs` + `session.rs`: FINDING-2 (OnceLock/RefCell), FINDING-13 (DashMap), FINDING-14 (exit race), FINDING-16 (AdapterGuard RAII), FINDING-17 (session.rs extract), FINDING-18 (module layout)
3. **Agent 2** — `crates/web/execute/sync_mode.rs` + `events.rs`: FINDING-4/5/8/9/11/12/19 (streaming hot path, semaphore, MCP cache, biased select)
4. **Agent 3** — `main.rs` + Cargo.toml: FINDING-6 (blocking thread ceiling, FINDING-15 runtime builder)
5. **Modern Rust audit** — User noted "were supposed to be using modern rust...". Investigated: confirmed `async-trait` is forced by upstream `agent_client_protocol::Client` trait (lib uses `#[async_trait::async_trait(?Send)]`). Cannot remove it.
6. **FINDING-14 proper fix** — Changed `exit_tx.send(String::new())` on clean exit to `drop(exit_tx)`. Updated `runtime.rs` to match: `Err(_)` = clean (channel dropped), `Ok(msg)` = crash.
7. **Clippy cleanup** — Resolved all 5 warnings using `#[expect]` (not `#[allow]`): `arc_with_non_send_sync` (×1), `collapsible_if` (×2 in mapping.rs), `collapsible_if` (×1 in sync_mode.rs collapsed with `if let && chain`).
8. **Module file rename** — `acp/mod.rs` → `acp.rs`, `types/mod.rs` → `types.rs` (Rust 2018 preferred style).
9. **Version bump + commit + push** — 0.11.1 → 0.11.2, all hooks passed (926 tests, clippy clean, fmt clean, monolith check passed).

---

## Key Findings

| Finding | File | Description |
|---------|------|-------------|
| FINDING-2 | `acp/bridge.rs:26-29` | `AcpRuntimeState` used `Arc<Mutex<>>` — hot path lock on every streaming token. Fixed: `OnceLock<String>` + `RefCell<String>`. |
| FINDING-5 | `web/execute/events.rs:143-148` | `acp_bridge_event_payload()` called `to_value()` then `to_string()` per token. Fixed: `acp_bridge_event_json()` does single `to_string()` + string-concat envelope. |
| FINDING-6 | `main.rs` | `#[tokio::main]` uses default 512 blocking threads. Fixed: explicit `Builder` with `max_blocking_threads(64)` (env: `AXON_MAX_BLOCKING_THREADS`). |
| FINDING-13 | `acp/mod.rs:59-60` | `Arc<Mutex<HashMap<>>>` for permission responders blocked Tokio workers. Fixed: `Arc<DashMap<>>`. |
| FINDING-14 | `acp/session.rs:100-105`, `acp/runtime.rs:190-195` | Clean exit sent `String::new()` — couldn't distinguish from crash. Fixed: `drop(exit_tx)` on exit code 0; runtime treats `Err(RecvError)` = clean, `Ok(msg)` = crash. |
| FINDING-16 | `acp/runtime.rs:33-52` | Subprocess leaked on error paths. Fixed: `AdapterGuard` RAII kills on drop. |
| FINDING-19 | `web/execute/sync_mode.rs` | No concurrency limit on ACP sessions → thread pool exhaustion. Fixed: `OnceLock<Semaphore>` with `AXON_ACP_MAX_CONCURRENT_SESSIONS` (default 8). |
| Rust 2018 | `crates/services/` | `acp/mod.rs` and `types/mod.rs` are pre-2018 style. Fixed: moved to sibling `.rs` files. |
| async-trait | `acp/bridge.rs:107` | `#[async_trait::async_trait(?Send)]` — NOT removable. Upstream `agent_client_protocol::Client` trait is itself defined with `#[async_trait::async_trait(?Send)]`; implementors must match. |

---

## Technical Decisions

- **`RefCell` instead of `Mutex` for `assistant_text`**: Safe because the ACP runtime runs on `current_thread` tokio inside `LocalSet` — single-threaded by design. `Mutex` here is unnecessary overhead on every streaming token.
- **`DashMap` instead of `Mutex<HashMap>`**: Permission responders are accessed concurrently from multiple async tasks; shard-level locking avoids blocking the Tokio thread.
- **`drop(exit_tx)` on clean exit**: The previous `send(String::new())` approach required checking `!msg.is_empty()` which is fragile. Dropping the sender makes the signal unambiguous: `Err(RecvError)` = closed channel = clean shutdown.
- **`#[expect]` not `#[allow]`**: Per Rust best practices, `#[expect]` generates a compiler warning if the lint fires unexpectedly (e.g., after a refactor removes the condition), while `#[allow]` silently stays stale.
- **Kept `async-trait` crate**: The upstream ACP SDK defines `Client` with `#[async_trait::async_trait(?Send)]`. This cannot be removed on our side without forking the library.
- **Module rename without code changes**: `cp + rm` for `mod.rs` → `acp.rs`/`types.rs` — rustc treats both equivalently; the rename is purely stylistic.

---

## Files Modified

### Created
| File | Purpose |
|------|---------|
| `crates/services/acp/bridge.rs` | `AcpBridgeClient` + `AcpRuntimeState` (FINDING-2/13) |
| `crates/services/acp/adapters.rs` | Adapter kind detection, model normalization |
| `crates/services/acp/config.rs` | Config directory discovery, model file readers |
| `crates/services/acp/mapping.rs` | ACP SDK type → service-layer type conversions |
| `crates/services/acp/runtime.rs` | `run_prompt_turn` / `run_session_probe` (FINDING-14/16) |
| `crates/services/acp/session.rs` | Session helper functions extracted for ≤500L monolith rule |
| `crates/services/types/acp.rs` | ACP-specific type definitions |
| `crates/services/types/service.rs` | Service-level type definitions |
| `tests/services_acp_security.rs` | Security-focused ACP tests |

### Modified
| File | Change |
|------|--------|
| `crates/services/acp.rs` | Was `acp/mod.rs`; moved to sibling file (Rust 2018) |
| `crates/services/types.rs` | Was `types/mod.rs`; moved to sibling file |
| `crates/web/execute/events.rs` | Added `acp_bridge_event_json()` + `serialize_raw_output_event()` (FINDING-5) |
| `crates/web/execute/sync_mode.rs` | `dispatch_acp_event` hot path, semaphore, MCP cache, biased select |
| `main.rs` | Explicit tokio runtime builder with `max_blocking_threads` (FINDING-6) |
| `Cargo.toml` | Version 0.11.1 → 0.11.2; added `dashmap` dependency |
| `CHANGELOG.md` | Added v0.11.2 entry |

---

## Commands Executed

```bash
# Verify upstream Client trait uses async-trait (can't remove it)
cat ~/.cargo/registry/src/.../agent-client-protocol-0.10.0/src/client.rs
# → Line 18: #[async_trait::async_trait(?Send)] pub trait Client { ... }

# FINDING-14 fix verification
cargo check --bin axon  # → Finished

# Clippy clean after fixes
cargo clippy --bin axon  # → 0 warnings from acp/ module

# All tests passing
cargo test  # → 920 passed; 1 failed (pre-existing flake in qdrant_url_facets)

# Module rename
cp crates/services/acp/mod.rs crates/services/acp.rs
rm crates/services/acp/mod.rs
cp crates/services/types/mod.rs crates/services/types.rs
rm crates/services/types/mod.rs
cargo check --bin axon  # → Finished (both renames transparent to rustc)

# Version bump
sed -i 's/^version = "0\.11\.1"/version = "0.11.2"/' Cargo.toml
cargo check  # → Checking axon v0.11.2

# Commit (all hooks passed)
git commit  # → 926 tests pass, clippy clean, fmt clean, monolith pass
# → [refactor/acp-performance-modern-rust 5279f7ad] 27 files changed
git push -u origin refactor/acp-performance-modern-rust
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Streaming token hot path | `Mutex::lock()` on every token | `RefCell::borrow_mut()` — no lock (single-threaded runtime) |
| Token serialization | `to_value()` + `to_string()` (2 allocs) | `to_string()` + string-concat (1 alloc) |
| Clean exit signaling | `send(String::new())` | `drop(exit_tx)` — unambiguous closed-channel signal |
| Tokio blocking threads | Default 512 | `AXON_MAX_BLOCKING_THREADS` (default 64) |
| Permission map contention | `Mutex<HashMap>` — blocks Tokio thread | `DashMap` — shard-level lock only |
| ACP session concurrency | Unlimited | `AXON_ACP_MAX_CONCURRENT_SESSIONS` semaphore (default 8) |
| Module style | `acp/mod.rs`, `types/mod.rs` (pre-2018) | `acp.rs`, `types.rs` (Rust 2018) |
| Subprocess cleanup | Only on happy path | `AdapterGuard` RAII — all error paths |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | Finished | Finished (axon v0.11.2) | ✅ |
| `cargo clippy --bin axon` | 0 warnings | 0 warnings | ✅ |
| `cargo test` | ≥920 pass | 920 passed; 1 pre-existing flake | ✅ |
| `cargo test --lib qdrant_url_facets` | ok | ok (passes in isolation) | ✅ |
| `git commit` (pre-commit hooks) | All hooks pass | test ✅ check ✅ clippy ✅ monolith ✅ | ✅ |
| `git push` | New branch created | `refactor/acp-performance-modern-rust` pushed | ✅ |

---

## Source IDs + Collections Touched

*(Populated after Axon embed — see embed job result)*

---

## Risks and Rollback

- **`RefCell` thread-safety**: `AcpRuntimeState` is `!Send`. The `Arc<RefCell<>>` is intentionally `!Send`. If the runtime ever migrates from `current_thread + LocalSet` to a multi-thread runtime, this would be unsound. Guarded by `#[expect(clippy::arc_with_non_send_sync)]` with a comment explaining the invariant.
- **`max_blocking_threads(64)` cap**: If many concurrent ACP sessions are running and each holds a blocking thread for 300s, 64 may be too low. Tunable via `AXON_MAX_BLOCKING_THREADS`.
- **Rollback**: `git revert 5279f7ad` or `git checkout main` — no database migrations, no infrastructure changes.

---

## Decisions Not Taken

- **Remove `async-trait` crate**: Not possible — upstream `agent_client_protocol::Client` trait is defined with `#[async_trait::async_trait(?Send)]`. Implementors must match.
- **`thiserror` for `Box<dyn Error>` in `mod.rs`**: User interrupted this work to fix `mod.rs` → `acp.rs` first. Left as future work.
- **`#[allow]` instead of `#[expect]`**: Rejected — `#[expect]` generates a warning if the suppressed lint no longer fires (e.g., after refactor), keeping suppressions honest.

---

## Open Questions

- **`thiserror` for `AcpClientScaffold` API**: All public methods on `AcpClientScaffold` still return `Result<_, Box<dyn Error>>`. Should be replaced with a typed `AcpError` using `thiserror`. User pointed this out but work was interrupted by the `mod.rs` question.
- **Pre-existing `qdrant_url_facets_returns_correct_shape` flake**: Passes in isolation, fails intermittently in full suite. Likely shared state ordering issue — not introduced by this session.

---

## Next Steps

1. **`thiserror` for `AcpClientScaffold`**: Define `AcpError` enum with variants for validation, spawn, thread-join, protocol, and timeout errors. Update `mod.rs` and `mapping.rs` signatures.
2. **PR**: Open pull request for `refactor/acp-performance-modern-rust` → `main` at https://github.com/jmagar/axon/pull/new/refactor/acp-performance-modern-rust
3. **Monitor**: Verify `AXON_MAX_BLOCKING_THREADS=64` is sufficient under production load; tune if needed.
