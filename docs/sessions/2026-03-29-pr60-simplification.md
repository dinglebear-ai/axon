# Session: PR #60 Simplification Pass
Date: 2026-03-29
Branch: feat/lite-mode
Commit: fffc1b55

---

## Session Overview

Code review and simplification pass on PR #60 (feat/lite-mode). Three parallel review agents (reuse, quality, efficiency) analyzed the full PR diff across 292 changed files (~28k lines). Six issues were fixed directly; documentation improvements were applied to dead code and constraint explanations.

---

## Timeline

1. **Orient** — Confirmed no ready beads (`bd stats`: 23 closed, 0 open). Pivoted to `/simplify` on the full PR.
2. **Diff analysis** — PR diff is 28,224 lines across 292 files; narrowed to 127 source `.rs` files for review.
3. **Parallel review** — Launched 3 agents concurrently: reuse, quality, efficiency agents reviewed core new code (`backend.rs`, `full.rs`, `lite/`, `services/runtime.rs`, `services/types/service.rs`).
4. **Fix pass** — Applied 6 targeted fixes: lift_err dedup, status warn, deadline order, WorkerHandles drop, from_summary comment, list_ingest_jobs doc.
5. **Verification** — `cargo check` clean; 75 unit tests passing (status/backend/lite/services suite).
6. **Push** — Version bumped 0.33.8 → 0.33.9; all 9 pre-commit hooks passed (monolith, clippy, test, rustfmt, etc.); pushed to origin.

---

## Key Findings

### Reuse Agent
1. `lift_err` (`full.rs:22`) and `lift_ss` (`runtime.rs:109`) — identical function defined in two crates with different names
2. `list_service_query`/`status_service_query` in `lite/query.rs:171-238` — 12 near-identical SQL projection strings across 6 job kinds × 2 query functions (not fixed — SQL DML, low risk)
3. `from_status_row`/`from_summary` (`service.rs:253-288`) — dead constructors; prefer `From` impls
4. `from_summary` sets `updated_at: summary.created_at` — `JobSummary` has no `updated_at` field (documented, not a bug)
5. `run_*_job_lite` in `runners.rs` — "fetch field, warn if missing, return Ok(None)" pattern repeated 6× (not collapsed — different fetch fields per job type)

### Quality Agent
1. `list_ingest_jobs` default impl applies `source_filter` post-`LIMIT` — silently wrong if matching rows < limit
2. `run_extract_job_lite` manually constructs `ExtractWebConfig` from 15 `cfg.*` fields — duplicates private `build_extract_web_config` in `services/extract.rs`
3. `JobStatus::from_str` unknown arm: silent `Failed` with no warning
4. `#[allow(dead_code)]` on `task_handles` without explanation of intentional detach behavior
5. `from_summary` dead code — `updated_at = created_at` undocumented

### Efficiency Agent
1. `FullBackend::list_jobs` hardcodes `(500, 0)` — confirmed dead code, bypassed by `FullServiceRuntime`
2. `wait_for_job`: deadline check after sleep → up-to-500ms overshoot
3. `WorkerHandles`: 12 background tasks spawned for one-shot commands; no drop/abort on cleanup
4. `run_extract_job_lite`: sequential URL extraction instead of concurrent fan-out
5. Discarded `HashSet<String>` from `run_crawl_once` silently skips sitemap backfill in lite mode

---

## Technical Decisions

### lift_err consolidation strategy
`lift_ss` in `runtime.rs` was aliased (`use crate::crates::jobs::backend::lift_err as lift_ss`) instead of renaming 30+ call sites in `runtime/full.rs`. Minimal churn, same deduplication result.

### wait_for_job fix: swap order, not rewrite
Used the simpler fix (move deadline check before sleep) rather than rewriting with `tokio::time::timeout`. The `timeout()` refactor would require restructuring the `job_status` call into a nested async block and changing error message format. Not worth it for 500ms max overshoot on a 300s poll.

### WorkerHandles::drop — abort vs detach
Added `Drop` impl that calls `handle.abort()` on all supervisor handles. Rust's default for dropped `JoinHandle` is silent detach (workers keep running). Abort is the correct behavior when `LiteBackend` is dropped at end of a one-shot command.

### Serial URL extraction not fixed
`run_extract_job_lite` serial loop left as-is. Concurrent fan-out requires `futures::stream::buffer_unordered` + non-trivial error propagation changes. Scope is too large for a simplification pass; filed implicitly via session note.

### Sitemap backfill gap not fixed
`run_crawl_once` returns `(CrawlSummary, HashSet<String>)` and the second value is discarded in both lite runner functions. Wiring `append_sitemap_backfill` in lite mode requires understanding when/how it's called in full mode. Deferred to a dedicated issue.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/jobs/backend.rs` | Added `pub(crate) fn lift_err`; moved deadline check before sleep in `wait_for_job` |
| `crates/jobs/full.rs` | Removed local `lift_err` def; imported from `backend` |
| `crates/jobs/lite/workers.rs` | Added `Drop for WorkerHandles` (aborts task handles); removed `#[allow(dead_code)]`; improved comment |
| `crates/jobs/status.rs` | Added `use tracing;`; added `tracing::warn!` in `from_str` unknown arm |
| `crates/services/runtime.rs` | Replaced `fn lift_ss` with `use lift_err as lift_ss`; added doc warning to `list_ingest_jobs` default |
| `crates/services/types/service.rs` | Added comment on `from_summary` explaining `updated_at = created_at` approximation |
| `Cargo.toml` | Version 0.33.8 → 0.33.9 |
| `CHANGELOG.md` | Added [0.33.9] entry |

---

## Commands Executed

```bash
# Review scope
git diff main...HEAD --stat  # 292 files, 16443 ins / 5378 del
git diff main...HEAD --name-only | grep '\.rs$' | wc -l  # 133 total, 127 source

# Verification
cargo check --bin axon  # clean
cargo test --lib -- status backend lite  # 75 passed, 0 failed

# Pre-commit hooks (all passed)
# monolith, rustfmt, clippy, test (1686 tests), check, claude-symlinks

# Push
git push  # fffc1b55 → origin/feat/lite-mode
```

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| Corrupt DB status value | Silently mapped to `Failed`, invisible in logs | `tracing::warn!(raw = s, ...)` emitted |
| `wait_for_job` timeout | Checked after 500ms sleep — up to 500ms overshoot | Checked before sleep — fires at deadline |
| `LiteBackend` drop | 12 worker tasks detached, keep running | All task handles aborted on drop |
| `lift_err` / `lift_ss` | Two identical private functions in different modules | One `pub(crate)` function in `backend.rs` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | No errors | No errors | ✅ |
| `cargo test --lib -- status backend lite` | 75 pass | 75 passed, 0 failed | ✅ |
| Pre-commit hooks (9 total) | All pass | All pass (62s test, 93s clippy) | ✅ |
| `git push` | Accepted | fffc1b55 pushed to feat/lite-mode | ✅ |

---

## Source IDs + Collections Touched

None — no Qdrant operations performed this session (code review + fix pass only).

---

## Risks and Rollback

- **Low risk** — all changes are mechanical refactors with no behavior changes except the three listed above (status warn, deadline order, worker abort).
- **WorkerHandles drop**: If any caller holds a `WorkerHandles` reference and expects workers to keep running after the handle is dropped, they will now be aborted. Current code: `LiteBackend` owns `WorkerHandles`; the only caller is lite-mode startup. No cross-process consumers.
- **Rollback**: `git revert fffc1b55`

---

## Decisions Not Taken

| Option | Rejected Because |
|--------|-----------------|
| Rename `lift_ss` → `lift_err` throughout `runtime/full.rs` | 30+ mechanical changes with no behavioral benefit; alias achieves same dedup |
| Rewrite `wait_for_job` with `tokio::time::timeout` | More invasive refactor for 500ms max improvement on a 300s poll |
| Fix serial extraction in `run_extract_job_lite` | `buffer_unordered` + error propagation changes too large for simplification pass |
| Wire sitemap backfill in lite mode | Requires understanding full-mode call sites and may affect job result JSON shape |
| Collapse `run_*_job_lite` fetch pattern | Different fetch fields per job type; a helper adds complexity without reducing lines meaningfully |

---

## Open Questions

1. Is `build_extract_web_config` in `services/extract.rs` intended to be private forever, or should it be `pub(crate)` for `runners.rs` reuse?
2. Should sitemap backfill be wired in lite mode? The full-mode `CrawlJob` result JSON includes a `backfill_count` field; lite-mode result JSON has no equivalent.
3. The `from_status_row` and `from_summary` constructors on `ServiceJob` have no call sites — should they be removed or converted to `From` impls?

---

## Next Steps

- Open a bead for sitemap backfill gap in lite mode (`run_crawl_once` discarded `HashSet<String>`)
- Consider opening a bead for `build_extract_web_config` pub(crate) to eliminate the `run_extract_job_lite` duplication
- PR #60 is ready for re-review with these simplifications applied
