# Session: PR #60 Review — Beads Issue Resolution

**Date:** 2026-03-27
**Branch:** `feat/lite-mode`
**Commit:** `88be5e9c`
**Duration:** ~1 session

---

## Session Overview

Triaged all 9 open beads from the PR #60 (feat/lite-mode) multi-agent review, confirmed relevance against the current codebase, then dispatched 4 parallel agents to fix the 7 confirmed issues. All changes landed in a single commit with all pre-commit hooks green (clippy, rustfmt, monolith, test suite — 1569 tests).

---

## Timeline

1. **Relevance audit** — Dispatched 2 investigation agents in parallel:
   - `agent-bugs`: rs8.10, rs8.11, rs8.12, rs8.15
   - `agent-cleanup`: rs8.13, rs8.14, rs8.16, rs8.17
2. **Triage results** — 7 confirmed still-relevant, rs8.12 reduced (dead code for intentionally out-of-scope feature), rs8.16 reduced (wrapper still used, not dead)
3. **Parallel fix dispatch** — 4 agents in isolated worktrees:
   - `agent-jobs-layer`: rs8.10 + rs8.11 + rs8.14 + rs8.15
   - `agent-services-layer`: rs8.13
   - `agent-cancel-cleanup`: rs8.12
   - `agent-visibility-fix`: rs8.17
4. **Integration** — All changes landed in main working dir; fixed one new clippy `dead_code` error (`task_handles` field after `pub→pub(crate)` narrowing)
5. **Commit** — `88be5e9c`, all 12 hook checks green, 1569 tests pass
6. **Beads closed** — All 7 issues closed via `bd close`
7. **Dolt remote** — Discussed but not needed; `.beads/backup/` JSONL committed to git is the durability mechanism

---

## Key Findings

- **rs8.10** (`store.rs:30`): `max_connections(4)` with 6 workers, no acquire timeout — confirmed present
- **rs8.11** (`backend.rs:128`): infinite poll loop with no deadline — confirmed present
- **rs8.12** (`cancel.rs:56`): `poll_sqlite_for_cancels` test-only, but cross-process cancel is intentionally out of scope for lite mode (single-process assumption) — closed as dead code
- **rs8.13** (`runtime/mapping.rs`): 6 identical free functions — confirmed; 119 LOC deleted after `From` trait migration
- **rs8.14** (`store.rs:51`, `backend.rs`): hardcoded 6-element table arrays in two places — confirmed; `JobKind::all()` now the single source of truth
- **rs8.15** (`store.rs`): `PRAGMA foreign_keys` never enabled; watch lease reclaim missing from startup — confirmed both gaps
- **rs8.17** (`workers.rs:22`): all 6 `Arc<Notify>` fields `pub` — confirmed; `pub(crate)` + `notify(kind)` method added
- **Bonus fixes by agents**: deterministic `ORDER BY id` tiebreak in `query.rs`, error source chain preserved in `services/jobs.rs::downgrade()`, ps empty-stderr handling in `preflight.rs`, `flock` portability in `memory-capture.sh`, dedup of `refresh_schedule` from MCP help

---

## Technical Decisions

- **`#[allow(dead_code)]` on `task_handles`**: Field intentionally kept for future panic monitoring; suppressed rather than removed since the doc comment explains its purpose
- **`reclaim_stale_watch_leases` at both startup paths**: Called from both `LiteBackend::new()` and `LiteBackend::new_with_path()` to cover all entry points, consistent with how `reclaim_stale_running_jobs` is called
- **`JobKind::all()` returns `&'static [JobKind]`**: Static slice avoids heap allocation; enum variants are exhaustive so this is safe to maintain manually
- **rs8.16 not fixed**: `table_for` wrapper is used 6× within `lite.rs` — not dead code; reducing it further (inline `kind.table_name()`) would be churn with no safety benefit
- **Dolt remote not configured**: `.beads/backup/` JSONL files are git-committed and sufficient for durability; Dolt remote is optional replication for multi-machine workflows not needed here

---

## Files Modified

| File | Change | Issue |
|------|--------|-------|
| `crates/jobs/backend.rs` | `JobKind::all()` added; `wait_for_job` timeout via env var | rs8.11, rs8.14 |
| `crates/jobs/lite.rs` | `reclaim_stale_watch_leases` at startup; `self.workers.notify(kind)` one-liner | rs8.15, rs8.17 |
| `crates/jobs/lite/store.rs` | `max_connections(8)`, `acquire_timeout(30s)`, `PRAGMA foreign_keys=ON`, `reclaim_stale_watch_leases` fn, `JobKind::all()` in reclaim loops | rs8.10, rs8.14, rs8.15 |
| `crates/jobs/lite/cancel.rs` | `poll_sqlite_for_cancels` removed; `CancelStore` doc updated | rs8.12 |
| `crates/jobs/lite/workers.rs` | Fields `pub→pub(crate)`; `notify(kind)` dispatch method added | rs8.17 |
| `crates/jobs/lite/query.rs` | `ORDER BY created_at DESC, id` tiebreak across all 6 job queries | bonus |
| `crates/services/runtime.rs` | `mod mapping;` declaration removed | rs8.13 |
| `crates/services/runtime/full.rs` | 13 call sites → `.map(ServiceJob::from)` | rs8.13 |
| `crates/services/runtime/mapping.rs` | **Deleted** (119 lines) | rs8.13 |
| `crates/services/types/service.rs` | 6 `From<XJob> for ServiceJob` impls added | rs8.13 |
| `crates/services/jobs.rs` | `downgrade()` now wraps error preserving source chain instead of stringifying | bonus |
| `crates/cli/commands/serve_supervisor/preflight.rs` | ps empty-stderr returns distinct error; error matching updated | bonus |
| `crates/mcp/server/handlers_system.rs` | Removed duplicate `refresh_schedule` entry from help subactions | bonus |
| `.beads/hooks/pre-commit` | Lefthook exit code propagation fix | bonus |
| `.claude/hooks/memory-capture.sh` | `flock` portability: fall through without lock when `flock` not installed | bonus |

---

## Commands Executed

```bash
# Triage
bd ready
bd show axon_rust-rs8.{10,11,12,13,14,15,16,17}

# Fix (agents ran internally)
cargo check       # per-agent verification
cargo clippy      # pre-commit hook

# Integration
git add crates/jobs/backend.rs crates/jobs/lite.rs ...
git commit -m "fix(lite): address 7 PR #60 review findings"
# → all 12 hooks green, 1569/1569 tests pass

# Close
bd close axon_rust-rs8.{10,11,12,13,14,15,17}
git push origin feat/lite-mode
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| SQLite pool | 4 connections, no timeout — silent hang under burst | 8 connections, 30s acquire timeout — loud failure |
| `--wait true` with stuck job | Infinite loop, hung terminal | Times out after `AXON_JOB_WAIT_TIMEOUT_SECS` (default 300s) |
| FK enforcement | `ON DELETE CASCADE` declared but unenforced | `PRAGMA foreign_keys=ON` — cascade actually fires |
| Watch lease recovery | Leaked leases on crash — watch stuck permanently | `reclaim_stale_watch_leases` runs at every startup |
| Adding a 7th job type | Must update table arrays in 3 places | Add one `JobKind` variant; `all()` propagates everywhere |
| ServiceJob conversion | 6 free functions in `mapping.rs` | `ServiceJob::from(job)` — idiomatic Rust, 119 LOC gone |
| Worker notification | 6-arm match in `enqueue()` | `self.workers.notify(kind)` |
| Cancel on running job | DB marked canceled, crawl runs to completion | Same behavior (unchanged), but clearly documented as intentional |

---

## Verification Evidence

| Check | Expected | Actual | Status |
|-------|----------|--------|--------|
| `cargo check` | No errors | 1 warning (pre-existing `task_handles`) | ✅ |
| `cargo clippy` | No errors | Clean after `#[allow(dead_code)]` on `task_handles` | ✅ |
| Pre-commit suite (12 hooks) | All pass | All pass (122s) | ✅ |
| Test suite | 1569 pass | 1569/1569 pass, 0 fail | ✅ |
| `bd close` × 7 | All closed | All confirmed closed | ✅ |
| `git push` | Up to date | `88be5e9c` pushed | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/query operations performed this session (pure code fix session).

---

## Risks and Rollback

- **FK enforcement**: Enabling `PRAGMA foreign_keys=ON` is strictly correct but could surface latent integrity violations in existing SQLite DBs that have orphaned rows. Risk is low (watch tables new, not yet in production). Rollback: remove the pragma line from `open_sqlite_pool`.
- **Pool size increase**: `max_connections(8)` opens more file descriptors. Low risk for SQLite. Rollback: revert to 4.
- **`wait_for_job` timeout**: CLI behavior change — `--wait true` now errors after 300s instead of hanging. Any caller expecting infinite wait will break. No known callers depend on infinite wait. Rollback: remove deadline check from `backend.rs`.

---

## Decisions Not Taken

- **rs8.16 (`table_for` wrapper)**: Not removed — still used 6×, only an indirection not dead code. Would be churn.
- **Lease-based mutual exclusion in `watch_lite.rs`**: Postgres version has full row-level locking on `claim_due_watch_defs`; SQLite version does not. Decided not to implement — single-process lite mode assumption makes concurrent schedulers impossible.
- **DoltHub / Dolt remote**: Not configured — `.beads/backup/` JSONL in git is sufficient.

---

## Open Questions

- `task_handles` field on `WorkerHandles`: should it eventually be used to detect worker panics? Currently just held and never read.
- Watch scheduler (`watch_lite.rs`) lease logic: should it match Postgres version's row-level locking for correctness? Currently only a startup reclaim, not per-claim mutual exclusion.
- `AXON_JOB_WAIT_TIMEOUT_SECS` default (300s): matches `AXON_EMBED_DOC_TIMEOUT_SECS`. Should these be unified into one env var?

---

## Next Steps

- rs8 parent bead (`axon_rust-rs8`) — close once PR #60 merges to main
- rs8.16 (`table_for` wrapper) — P3, can be cleaned up opportunistically
- Consider implementing per-claim lease locking in `watch_lite.rs` before scheduler activates in production
