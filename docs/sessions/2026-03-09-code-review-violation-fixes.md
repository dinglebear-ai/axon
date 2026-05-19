# Code Review Violation Fixes
**Date:** 2026-03-09
**Branch:** `refactor/acp-performance-modern-rust`
**Session Type:** Code review remediation

---

## Session Overview

Applied 9 targeted fixes across Rust and TypeScript files in response to a structured code review that flagged security, correctness, and reliability violations. All violations were independently confirmed valid before patching. All Rust changes compiled clean and 11 integration tests pass.

---

## Timeline

1. Loaded violation report (9 findings across 7 files)
2. Read all affected files in parallel to assess each finding
3. Confirmed all 9 violations valid
4. Applied fixes: Rust files in one pass, TypeScript concurrently
5. Verified: `cargo check` clean, `cargo test --test services_acp_lifecycle` → 11/11 pass

---

## Key Findings

| # | File | Finding |
|---|------|---------|
| 1 | `main.rs:82` | `AXON_MAX_BLOCKING_THREADS=0` parsed as `Some(0)`, passed to `max_blocking_threads(0)` → tokio panics at startup |
| 2 | `apps/web/lib/sessions/session-scanner.ts:170` | Nested `Promise.all` over all projects × all files is unbounded; large repos exhaust FDs/subprocesses silently |
| 3 | `tests/services_acp_lifecycle.rs:154` | `prepare_session_setup_rejects_empty_adapter_program` never called a fallible function; test body was `assert_eq!(field, " ")` — guaranteed to pass without catching the regression |
| 4 | `crates/services/acp/mapping.rs:308` | Shell blocklist only checked `path.file_name()` (original path basename); symlink `/tmp/safe_name → /bin/bash` bypassed validation |
| 5 | `crates/services/acp/bridge.rs:117` | `if self.auto_approve` in disconnect + timeout branches is always `false` (early return at line 117 handles `auto_approve=true`); dead code with security-critical labels |
| 6a | `crates/services/acp/config.rs:11` | `HOME=""` → `PathBuf::from("")` → `.join(".codex")` = relative path `.codex`; config resolved against CWD |
| 6b | `crates/services/acp/config.rs:32` | `read_codex_default_model()` returned `Some("")` for `model = ""` in TOML; no `.filter(!empty)` unlike Gemini counterpart |
| 7 | `Justfile:210` | `node apps/web/shell-server.mjs` launched from project root; server uses relative paths `.env.local` and `../../.env` requiring `apps/web` as CWD |
| 8 | `crates/web/execute/sync_mode.rs:826` | Event type extractor found first `"` in `{"type":"assistant_delta",...}` and extracted `"type"` (the key), not the value; streaming-event filter never triggered → all token events logged |
| 9 | `crates/web.rs:33` | Two independent semaphores gate ACP sessions using same env var `AXON_ACP_MAX_CONCURRENT_SESSIONS` but different defaults: `web.rs` = 5, `sync_mode.rs` = 8 |

---

## Technical Decisions

- **Violation 4 (symlink bypass)**: Kept the two-block structure (`#[expect(collapsible_if)]` removed since the block now has two inner `if`s). Added `canonicalize()` call inside the shell check block — separate from the earlier canonicalize that only checked file existence. Slightly redundant canonicalize but the blocks serve different purposes and keeping them separate preserves clarity.
- **Violation 5 (dead code)**: Simply removed the `if self.auto_approve` branches rather than restructuring. If a future mode wants auto-approve + interactive (as the "FIX SEC-1" comments hinted), it must be explicitly designed, not left as dead code.
- **Violation 8 (event type extraction)**: Replaced multi-step index arithmetic with `strip_prefix(r#"{"type":""#).and_then(|rest| rest.find('"').map(|e| &rest[..e]))` — one line, correct, avoids off-by-one risk.
- **Violation 9 (semaphore defaults)**: Aligned `web.rs` default from `5` → `8` to match the "existing" `sync_mode.rs` semaphore. The `web.rs` one is the outer (WS handler level) gatekeeper and was the "new" one with the lower default per the violation description. Not consolidating to one semaphore — the two semaphores serve different abstraction layers and removal would be a larger refactor.
- **session-scanner.ts**: Used an inline `mapWithConcurrency` helper (no new npm dependency) with limits of 8 projects + 16 files/project — conservative enough to prevent FD exhaustion on typical homelab repos (hundreds of projects × dozens of sessions each).

---

## Files Modified

| File | Change |
|------|--------|
| `main.rs` | `.filter(|&v| v > 0)` on `AXON_MAX_BLOCKING_THREADS` parse to reject 0 |
| `crates/services/acp/bridge.rs` | Removed dead `if self.auto_approve` from disconnect + timeout arms |
| `crates/services/acp/config.rs` | `.filter(|v| !v.is_empty())` on HOME + GEMINI_CLI_HOME; `.filter(|s| !s.is_empty())` on codex model value |
| `crates/services/acp/mapping.rs` | Added canonical path basename check after original basename check in shell blocklist |
| `crates/web.rs` | Semaphore default changed from `unwrap_or(5)` → `unwrap_or(8)` |
| `crates/web/execute/sync_mode.rs` | Replaced broken event type extractor with `strip_prefix({"type":"")` approach |
| `tests/services_acp_lifecycle.rs` | `prepare_session_setup_rejects_empty_adapter_program` now calls `prepare_initialize()` and asserts `Err` |
| `apps/web/lib/sessions/session-scanner.ts` | Added `mapWithConcurrency<T,R>()` helper; replaced two `Promise.all` calls with bounded versions |
| `Justfile` | `node apps/web/shell-server.mjs` → `(cd apps/web && node shell-server.mjs)` |

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `AXON_MAX_BLOCKING_THREADS=0` | Tokio runtime panics at startup | Value silently rejected; falls back to default 64 |
| Session scanner (large repos) | Unbounded concurrent git + FD ops; silently skipped on limit | Capped at 8 projects × 16 files concurrently |
| ACP adapter symlink `/tmp/x → /bin/bash` | Passes shell blocklist | Rejected via canonical path check |
| ACP disconnect/timeout in interactive mode | Dead `if auto_approve` branch (never true) retained | Simplified to unconditional `Cancelled` |
| `HOME=""` / `GEMINI_CLI_HOME=""` | Config resolved relative to CWD | Returns `None`; config not read |
| Codex `model = ""` in config.toml | Selected as model name | Filtered out; fallback to default |
| `shell-server.mjs` `.env.local` load | Failed silently (wrong CWD); shell server missing env vars | Resolved correctly from `apps/web/` |
| ACP streaming event log filter | Every token event logged (filter never matched `"type"` key) | Streaming events (`assistant_delta`, `thinking_content`, `user_delta`) suppressed |
| ACP concurrent session default | `web.rs` = 5, `sync_mode.rs` = 8 (inconsistent effective limit) | Both default to 8 |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | 0 errors | `Finished dev profile` | ✅ |
| `cargo test --test services_acp_lifecycle` | 11 passed | 11/11 passed | ✅ |
| `prepare_session_setup_rejects_empty_adapter_program` test | `expect_err` + contains `"cannot be empty"` | PASS | ✅ |

---

## Source IDs + Collections Touched

_Axon embed attempted after file write — see embed section below._

---

## Risks and Rollback

- **Semaphore default change** (`5→8`): increases max concurrent ACP sessions from 5 to 8 when env var unset. In practice both semaphores gate the same sessions at the WS layer first, so the net change is 5→8 concurrent pulse_chat sessions. Rollback: revert `unwrap_or(8)` in `crates/web.rs`.
- **Canonical path check in mapping.rs**: adds a `canonicalize()` syscall on every adapter validation. Non-existent paths return `Err` from `canonicalize` (same as before — the `if let Ok(...)` guard handles it). No behavior change for paths that don't resolve.
- **All other changes**: purely additive guards or dead-code removal. No functional regression path.

---

## Decisions Not Taken

- **Consolidate two ACP semaphores into one**: would require extracting the static from `sync_mode.rs` and referencing `web.rs`'s copy, or creating a shared module. Too broad for a focused violation-fix session.
- **Use `p-limit` npm package for session-scanner**: would require adding a dependency. Inline `mapWithConcurrency` achieves the same with zero new deps.
- **Reject out-of-range values for `AXON_MAX_BLOCKING_THREADS`** (e.g., > 10000): violation only flagged `0`; adding an upper bound was out of scope.

---

## Open Questions

- Should the two ACP semaphores be consolidated? Currently both read `AXON_ACP_MAX_CONCURRENT_SESSIONS`; having two is slightly confusing but harmless now that defaults match.
- The `shell-server.mjs` fix ensures correct CWD but the `../../.env` path assumes exactly two directory levels above `apps/web` is the project root — this breaks if `apps/web` is ever nested differently.

---

## Next Steps

- Run full test suite (`cargo test`) after any future ACP changes to catch regressions in the symlink validation path
- Consider adding a unit test for the symlink bypass fix in `mapping.rs` (requires creating a temporary symlink in a test, which is possible with `std::os::unix::fs::symlink` in a `#[cfg(unix)]` test)
- Consider adding a unit test for the `AXON_MAX_BLOCKING_THREADS=0` guard in `main.rs`
