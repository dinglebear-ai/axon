# Session: Simplify — FullServiceRuntime + extract.rs Cleanup
**Date:** 2026-03-28
**Branch:** `feat/lite-mode`

---

## Session Overview

Short maintenance session: built the release binary, then ran `/simplify` over the last 5 commits of `feat/lite-mode`. Three review agents (reuse, quality, efficiency) identified and we fixed three code quality issues in the service layer.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | `cargo build --release --bin axon` — 5m 26s, exit 0 |
| After build | `/simplify` — read recent commits (HEAD~5..HEAD), launched 3 review agents in parallel |
| Phase 3 | Applied fixes to `runtime/full.rs` and `extract.rs` |
| Verify | `cargo check` — 14.62s, exit 0, no warnings |

---

## Key Findings

1. **`FullServiceRuntime::has_active_jobs`** (`runtime/full.rs:39-46`) — manual `match kind { ... }` hardcoded 6 table name strings; `JobKind::table_name()` already existed in `backend.rs` with identical mapping. `LiteServiceRuntime` already used `kind.table_name()`. Divergence risk on new `JobKind` variant.

2. **`cancel_job`, `cleanup_jobs`, `clear_jobs`, `recover_jobs`** (`runtime/full.rs`) — 19 occurrences of `.map_err(|e| e.to_string().into())` when the in-scope `lift_ss` helper (imported via `use super::lift_ss`) does exactly the same thing. Inconsistent with every other `.map_err` call in the same file.

3. **`extract_status_raw` / `extract_list_raw`** (`extract.rs:78-91`) — dead functions with zero callers. Took `cfg: &Config` directly instead of `&ServiceContext`, bypassing the services-first contract. Legacy from before the `ServiceContext` refactor.

4. **Decisions not taken (agents flagged, but skipped):**
   - `schedule_subaction` string literals → enum: requires schema changes, larger refactor
   - `base_service_context()` repeated 31× → helper method: bigger MCP refactor
   - Triple manifest reads in `crawl_sync.rs`: complex to thread state through safely
   - Sequential `enqueue` loop in `handle_refresh_start`: low-to-medium, not hot path

---

## Technical Decisions

- Used `kind.table_name()` from `JobBackend` (already the canonical source in `backend.rs:23`) — eliminates a duplicate maintenance surface and matches `LiteServiceRuntime`'s existing pattern.
- Used `lift_ss` instead of inline closure — `lift_ss` is literally `|e| e.to_string().into()` (defined `runtime.rs:109`), so this is pure consistency, no behavior change.
- Deleted `extract_status_raw`/`extract_list_raw` outright rather than keeping with deprecation shim — zero callers confirmed by full codebase grep; consistent with CLAUDE.md "no backwards-compat shims" rule.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/services/runtime/full.rs` | Replace 6-arm table-name match with `kind.table_name()`; replace 19× `.map_err(|e| e.to_string().into())` with `lift_ss` |
| `crates/services/extract.rs` | Delete `extract_status_raw`, `extract_list_raw`; remove `get_extract_job`, `list_extract_jobs` imports; remove `pub use ExtractJob` re-export (unused) |

---

## Commands Executed

```bash
# Release build
cargo build --release --bin axon
# → Finished release profile in 5m 26s (exit 0)

# Type check after fixes
cargo check
# → Finished dev profile in 14.62s (exit 0)

# Confirm dead code had zero callers
grep -rn "extract_status_raw\|extract_list_raw" crates/
# → Only definitions in services/extract.rs (no callers)

grep -rn "services::extract::ExtractJob" crates/
# → No results (re-export unused)
```

---

## Behavior Changes (Before/After)

| Surface | Before | After |
|---------|--------|-------|
| `has_active_jobs` table name | Hardcoded 6-arm match | Delegates to `JobKind::table_name()` |
| Error mapping in full runtime | `.map_err(\|e\| e.to_string().into())` in 19 arms | `.map_err(lift_ss)` — same behavior, consistent style |
| `extract_status_raw` | Existed (zero callers) | Deleted |
| `extract_list_raw` | Existed (zero callers) | Deleted |

No user-visible behavior changes. All changes are internal consistency fixes.

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | 0 warnings, exit 0 | 0 warnings, exit 0 | ✅ PASS |
| `grep extract_status_raw crates/` | 0 callers remain | 0 results | ✅ PASS |
| `grep services::extract::ExtractJob crates/` | 0 callers | 0 results | ✅ PASS |

---

## Source IDs + Collections Touched

*(Axon embed attempted below — see embedding section)*

---

## Risks and Rollback

**Risk:** None — all changes are behavioral no-ops (same runtime behavior, different code path).

**Rollback:** `git revert HEAD` if needed (single commit).

---

## Decisions Not Taken

| Alternative | Reason Skipped |
|-------------|----------------|
| `schedule_subaction` → enum in schema | Requires schema.rs + handler changes; out of scope for simplify pass |
| `base_service_context()` helper | 31-site MCP refactor; worth a dedicated PR |
| Reduce triple manifest reads in `crawl_sync` | Requires threading `previous_urls` + backfill URLs through 3 functions; risk of subtle correctness bugs without test coverage |
| Parallelize `handle_refresh_start` enqueue loop | Not a hot path (called per-request but URL count is typically 1); partial failure semantics easier to reason about sequentially |

---

## Open Questions

- `run_worker` in `FullServiceRuntime` has no guard against double-invocation (spawns a permanent background OS thread per call). Not a current bug since callers invoke it once at startup, but a future "restart worker" MCP action would silently spawn competing workers.

---

## Next Steps

- Open a tracking issue for the `schedule_subaction` enum refactor (medium priority)
- Consider a follow-up issue for `base_service_context()` helper to eliminate the 31-site repetition in MCP handlers
- Merge `feat/lite-mode` → `main` (was marked ready 2026-03-28)
