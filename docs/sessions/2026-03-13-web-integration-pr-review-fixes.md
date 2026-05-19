# Web Integration PR Review Fixes — feat/web-integration-review-fixes

**Date:** 2026-03-13
**Branch:** `feat/web-integration-review-fixes`
**PR:** jmagar/axon — web integration security, protocol, and performance fixes

---

## Session Overview

Continued from a previous context-compressed session. The goal was to address all open CodeRabbit/reviewer threads on the PR for web integration fixes. This session resolved 14 remaining unresolved threads across 2 new commits after parallel agent work (Rust + TypeScript agents) had already handled the bulk of the issues.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Resumed from context summary; `cargo check` passing with 2 warnings |
| 16:30 | Removed unused `uuid::Uuid` import from `execute.rs` |
| 16:35 | Committed security + compilation fixes (RBg0, RBgp): session ownership gate, download.rs auth consistency |
| 16:40 | Analyzed 14 remaining unresolved CodeRabbit threads; categorized as real fixes vs false positives |
| 16:45 | Fixed Q92M (shell WS max_message_size), RBgc (ACP_MODES constant), RBg8/RBhD (markdown) |
| 16:50 | Fixed clippy `collapsible_if` in `run_forward_task` |
| 16:55 | Committed second batch; resolved all 14 threads via `mark_resolved.py` |
| 16:57 | Verified 0 unresolved threads |

---

## Key Findings

- **RBg0 (Critical security):** `ws_handler.rs::handle_acp_resume` used `session_ownership.entry().or_insert_with()` unconditionally, allowing any authenticated client to claim ownership of arbitrary session IDs. Fixed by checking `SESSION_CACHE.get_by_session_id()` first and early-returning if session doesn't exist.
- **RBgp (Auth consistency):** `download.rs` had its own inline auth that used case-sensitive `trim_start_matches("Bearer ")` with inverted header precedence (`x-api-key` before `authorization`), bypassing the RFC 6750-compliant `check_auth()` fix. Simplified to delegate entirely to `check_auth()`.
- **Q92M (Security/Allocation):** The shell WebSocket input-size check at `shell.rs:106` ran AFTER axum had already buffered the full message. Fixed at the upgrade level with `ws.max_message_size(65_536)` in `web.rs`.
- **RBgc (Maintenance):** `acquire_acp_permit()` in `execute.rs` and `is_acp_mode()` in `sync_mode.rs` independently hardcoded `pulse_chat / pulse_chat_probe`. Added `ACP_MODES` constant to `constants.rs` as shared source of truth.
- **Threads 1, 2, 3, 5, 6, 7, 8, 9, 10 (False positives):** CodeRabbit was commenting on the diff vs main rather than the current branch state. All referenced code had already been fixed by the parallel agents.

---

## Technical Decisions

- **Kept `is_acp_mode()` as enum match** rather than rewriting it to use `ACP_MODES.contains(&mode.as_str())` — `ServiceMode` has no `as_str()` method and adding one would be scope creep. Cross-reference comment keeps both sites in sync without forcing a breaking change to the enum.
- **`max_message_size` at upgrade vs runtime check** — axum's `WebSocketUpgrade::max_message_size()` rejects oversized frames before allocation; the existing per-message check in `handle_shell_ws` now only guards PTY stdin writes (still valuable defense-in-depth).
- **Resolved all 14 threads** rather than deferring architectural ones (P2 capability flags, shell input size) — the code changes are either already in place (capability flags were wired correctly) or simple enough to fix now (shell size).
- **Markdown list numbering** reset to sequential per subsection (1-7) rather than continuing globally (5-11) — MD029 requires `1/2/3` style.

---

## Files Modified

| File | Purpose |
|------|---------|
| `crates/web/execute.rs` | Added `pub(crate) use context::ExecCommandContext`; changed `handle_command` to `pub(crate)`; removed unused `uuid::Uuid`; used `ACP_MODES` constant |
| `crates/web/execute/context.rs` | Changed `ExecCommandContext` and all fields from `pub(super)` to `pub(crate)` for cross-module access |
| `crates/web/execute/constants.rs` | Added `ACP_MODES: &[&str]` constant |
| `crates/web/execute/sync_mode.rs` | Added cross-reference comment to `is_acp_mode()` linking to `ACP_MODES` |
| `crates/web/ws_handler.rs` | Fixed RBg0 session ownership gate; added `_client_ip` param; rewrote `handle_command` call site; fixed clippy `collapsible_if` |
| `crates/web/download.rs` | Removed redundant inline auth; delegated to `check_auth()` (RBgp) |
| `crates/web.rs` | Added `max_message_size(65_536)` on shell WebSocketUpgrade (Q92M) |
| `WEB-INTEGRATION-REVIEW.md` | Fixed MD029 list numbering; removed duplicate L-13 entry |

---

## Commits

| Hash | Message |
|------|---------|
| `57c33133` | fix(web): session ownership gate, auth consistency, and compilation fixes (RBg0, RBgp) |
| `b387bf95` | fix(web): shell WS msg size gate, ACP mode constant, markdown formatting (Q92M, RBgc, RBg8/D) |

Prior session (parallel agents):
| `6c6b3837` | fix(web): address P1/P2 Rust PR review issues (threads 4,5,6,7,11,13,14,16,17,19,20,21,22) |
| `ae3382d4` | fix(web): address TypeScript PR review issues (threads 2,9,12,15) |

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| ACP session ownership | Any authenticated client could claim ownership of arbitrary session IDs via `acp_resume` | Only sessions that exist in `SESSION_CACHE` can be claimed; bogus IDs return "not found" |
| Download auth | `download.rs` used case-sensitive Bearer parsing + inverted precedence | Delegates to `check_auth()` — RFC 6750 compliant, consistent with all other routes |
| Shell WS message size | Size check ran after axum allocated the full message buffer | axum rejects messages >64 KiB before allocation; PTY guard is defense-in-depth only |
| ACP mode constant | `pulse_chat / pulse_chat_probe` hardcoded in two independent places | Single `ACP_MODES` constant; both sites reference it with cross-reference comment |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | 0 errors, 0 warnings | 0 errors, 0 warnings | ✅ PASS |
| `cargo clippy` | 0 errors | 0 errors | ✅ PASS |
| `cargo fmt --check` | clean | clean | ✅ PASS |
| pre-commit hooks (lefthook) | all pass | all pass (2 commits) | ✅ PASS |
| `verify_resolution.py` | 0 unresolved threads | "All threads resolved!" | ✅ PASS |
| `mark_resolved.py` (14 threads) | 14/14 resolved | 14/14 resolved | ✅ PASS |

---

## Source IDs + Collections Touched

*(No Axon embed/retrieve operations performed in this session.)*

---

## Risks and Rollback

- **Session ownership gate (RBg0):** Low risk — more restrictive than before. Any connection attempting to resume a nonexistent session now gets a "not found" response instead of silently succeeding. Rollback: revert `ws_handler.rs` session lookup order.
- **max_message_size on shell WS (Q92M):** Very low risk — limit is 64 KiB, generous for any legitimate terminal input. Rollback: remove `.max_message_size(MAX_SHELL_WS_MSG)` from `web.rs`.
- **ACP_MODES constant (RBgc):** Pure refactor — no behavior change. Rollback: revert constants.rs and execute.rs to inline literals.

---

## Decisions Not Taken

- **Adding `as_str()` to `ServiceMode`** — would allow `is_acp_mode` to use `ACP_MODES.contains()` directly, but `ServiceMode` is a domain enum and adding a string method for a single use-case in the concurrency gate is over-engineering.
- **Fixing all MD022 heading violations in WEB-INTEGRATION-REVIEW.md** — CodeRabbit only flagged specific lines; fixing all 30+ headings in the file would be a noisy diff with no functional value.
- **Wiring a permission approval UI for `onPermissionRequest`** (thread RBgG suggestion) — the TS agent already added `useCallback` auto-approve fallback; a full UI modal is a separate feature ticket.

---

## Open Questions

- The two "deferred architectural" threads (P2 capability flags, shell input size) were resolved as fixed — but the capability flags issue was confirmed as already-wired by code review (threads 5/6/13 were false positives). If CodeRabbit re-triggers on the push, re-verify `dispatch.rs:320-329` shows `AdapterCapabilities { enable_fs, enable_terminal, ... }` being passed.
- `WEB-INTEGRATION-REVIEW.md` still has 30+ headings missing blank lines after them (MD022). Not flagged in recent threads but could resurface.

---

## Next Steps

1. Push branch and await PR CI / new CodeRabbit pass
2. Merge `feat/web-integration-review-fixes` → `main` once PR is approved
3. Version bump to v0.24.0 on merge (per `quick-push` convention)
4. Address remaining WEB-INTEGRATION-REVIEW.md action plan items (M-tier and L-tier) in follow-up PRs
