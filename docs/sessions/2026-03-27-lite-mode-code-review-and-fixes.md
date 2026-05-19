# Session: Lite Mode Code Review and Fixes
**Date:** 2026-03-27
**Branch:** `feat/lite-mode`
**Scope:** `crates/jobs/lite/store.rs`, `crates/jobs/lite/workers.rs`

---

## Session Overview

Performed a comprehensive Rust code review of the two modified files on `feat/lite-mode` using the `beagle-rust:review-rust` skill. Identified 4 issues (1 Critical, 2 Major, 1 Minor) and dispatched an agent to fix all of them. All fixes verified clean via `just verify`.

---

## Timeline

1. **Review triggered** — `/beagle-rust:review-rust` invoked on `feat/lite-mode`
2. **Changed files identified** — 100 Rust files changed across the branch; git status showed 2 unstaged/staged files: `store.rs` (staged) and `workers.rs` (unstaged)
3. **Clippy run** — `cargo clippy -D warnings` immediately surfaced a hard compile error in `store.rs:8`
4. **Skills loaded** — `rust-code-review`, `tokio-async-code-review`, `sqlx-code-review`, `serde-code-review`, `rust-testing-code-review`, `review-verification-protocol`
5. **Callers verified** — `reclaim_stale_running_jobs_for_table` traced to `services/runtime.rs:530`; confirmed `kind.table_name()` always returns `&'static str` from enum match, so SQL interpolation was latent-but-not-active injection surface
6. **Fix agent dispatched** — Single agent applied all 4 fixes
7. **Verification** — `just verify` passed, both inline tests pass

---

## Key Findings

- **`store.rs:8-14`** — Nested `if let Err` inside `if … && let Some(parent)` violates `clippy::collapsible_if` (Rust 2024 edition, `-D warnings`); this was a hard compile error blocking the entire build
- **`store.rs:75-91`** — `reclaim_stale_running_jobs_for_table(table: &str)` is `pub` and interpolates `table` directly into SQL via `format!`. Current callers only pass `kind.table_name()` (enum-derived `&'static str`), making it latent, not active
- **`workers.rs:39-69`** — All 6 `tokio::spawn` JoinHandles immediately dropped; worker task panics would be silently absorbed by the runtime with no signal to the supervisor
- **`workers.rs:504`** — Test `worker_picks_up_job_via_notify` used `sleep(100ms)` as a synchronization barrier; timing-dependent and flaky under load
- **Bonus fix** — Agent also caught `run_ingest_job_lite` preview truncation using raw byte slice (`&config_json[..120]`), which panics at non-ASCII char boundaries; rewritten to `.chars().take(120).collect::<String>()`

---

## Technical Decisions

- **Allowlist over type change** — `reclaim_stale_running_jobs_for_table` was left accepting `&str` (changing it to `JobKind` would ripple into `services/runtime.rs`); instead a `VALID_TABLES: &[&str]` const allowlist + early return was added. Keeps the public API stable while closing the injection surface.
- **`task_handles: Vec<JoinHandle<()>>` on `WorkerHandles`** — Storing handles in the returned struct rather than a `JoinSet`-based supervisor loop keeps the change minimal and non-breaking; callers decide how to monitor.
- **`oneshot` channel over `Notify` for test sync** — The test spawns a task that may or may not find a job; `oneshot` + `timeout(5s)` makes the wait deterministic and gives a clear failure message if the task drops the sender.

---

## Files Modified

| File | Purpose |
|------|---------|
| `crates/jobs/lite/store.rs` | Fix clippy::collapsible_if; add VALID_TABLES allowlist guard in reclaim fn |
| `crates/jobs/lite/workers.rs` | Store JoinHandles in WorkerHandles; fix flaky test; fix char-boundary bug in preview |

---

## Commands Executed

```bash
# Identified changed files
git diff --name-only $(git merge-base HEAD main)..HEAD | grep -E '\.rs$'

# Confirmed hard compile error
cargo clippy --all-targets --all-features -- -D warnings
# → error: this `if` statement can be collapsed (crates/jobs/lite/store.rs:8)

# Verified caller of reclaim_stale_running_jobs_for_table
grep -rn "reclaim_stale_running_jobs_for_table" .
# → crates/services/runtime.rs:530 using kind.table_name() (&'static str from enum)

# Full gate after fixes
just verify
# → all pass

# Inline test confirmation
cargo test --lib workers
# → test crates::jobs::lite::workers::tests::worker_picks_up_job_via_notify ... ok
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Build | **Broken** — `cargo clippy -D warnings` fails at `store.rs:8` | Clean — all targets compile |
| `open_sqlite_pool` dir creation | Silently swallowed errors (`.ok()`) | Warns via `tracing::warn!` on failure |
| `reclaim_stale_running_jobs_for_table` | Accepts any `&str` table name, interpolates into SQL | Returns `sqlx::Error::Configuration` for unrecognized table names |
| Worker supervision | 6 JoinHandles dropped; panics silent | Handles stored in `WorkerHandles.task_handles`; caller can monitor |
| `worker_picks_up_job_via_notify` test | Races on 100ms sleep; flaky under load | Deterministic `oneshot` + `timeout(5s)` |
| Ingest job preview truncation | `&config_json[..120]` — panics at multi-byte char boundary | `.chars().take(120).collect()` — always safe |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo clippy --all-targets --all-features -- -D warnings` | No warnings/errors | `Finished dev profile` | ✅ |
| `cargo check --all-targets` | Clean | Clean | ✅ |
| `just verify` (fmt + clippy + check + test) | All pass | All pass | ✅ |
| `cargo test --lib workers` | `worker_picks_up_job_via_notify ... ok` | ok (0.01s) | ✅ |

---

## Source IDs + Collections Touched

None — this session did not perform any Axon embed/retrieve operations on external sources.

---

## Risks and Rollback

- **All changes are additive or narrowly scoped** — no behavioral regressions expected in currently exercised paths
- `VALID_TABLES` allowlist will return an error if `JobKind::table_name()` is ever extended with a new table and the allowlist isn't updated — this is intentional (forces explicit acknowledgment of new tables). Keep the two lists in sync.
- **Rollback**: `git checkout crates/jobs/lite/store.rs crates/jobs/lite/workers.rs` reverts all changes; the branch was already ahead of origin by 2 commits before this session.

---

## Decisions Not Taken

- **Change `reclaim_stale_running_jobs_for_table` to accept `JobKind`** — Would eliminate the interpolation entirely but requires touching `services/runtime.rs` and the function's public API. Deferred; allowlist is sufficient for now.
- **Use `tokio::task::JoinSet` for worker supervision loop** — More robust (auto-restarts on panic) but more complex. Deferred; storing handles in `WorkerHandles` gives callers visibility without mandating a supervision strategy.
- **Suppress clippy with `#[allow]`** — Rejected; the collapsible lint is a real improvement, not just style.

---

## Open Questions

- Who (if anyone) in the caller tree currently inspects `WorkerHandles.task_handles`? If nobody joins them, the panic-visibility improvement is still latent.
- Should `reclaim_stale_running_jobs` (bulk version, line 43) also add the allowlist? It iterates a hardcoded static slice so there's no injection risk there — but consistency might be worth it.

---

## Next Steps

- Commit the two modified files (`store.rs`, `workers.rs`) — `CLAUDE.md` and `.lavra/memory/knowledge.jsonl` are session artifacts and should be evaluated separately
- Consider adding a supervision loop in the `serve` command that joins `task_handles` and logs/alerts if any worker exits unexpectedly
- Keep `VALID_TABLES` in `store.rs` in sync with `JobKind::table_name()` in `backend.rs` whenever a new job type is added
