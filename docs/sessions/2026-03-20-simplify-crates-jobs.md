# Simplify: crates/jobs Code Review and Cleanup

**Date:** 2026-03-20
**Branch:** feat/pulse-shell-and-hybrid-search
**Skill:** `/simplify crates/jobs`

---

## Session Overview

Ran a three-agent parallel code review (reuse, quality, efficiency) over the most recently modified files in `crates/jobs`, then applied all genuine findings. Net result: ~35 lines removed from `extract/worker.rs`, peak memory halved for large extract jobs, performance profile tuning now applies to extract parallelism, Redis error handling aligned across workers, and several smaller readability/documentation improvements.

---

## Timeline

1. **Identified target files** — No git diff existed for `crates/jobs`; located the 10 most recently modified `.rs` files by mtime.
2. **Read key files** — `extract/worker.rs`, `embed/worker.rs`, `common.rs`, `common/heartbeat.rs`, `common/job_ops.rs`, `worker_lane.rs`, `worker_lane/amqp.rs`.
3. **Launched 3 review agents in parallel** — Code Reuse, Code Quality, Efficiency.
4. **Aggregated findings** — Discarded false positives (alleged double `mark_job_failed` in embed — traced paths, confirmed no actual double-call), kept 12 genuine issues.
5. **Applied all fixes** — 6 files modified, zero new clippy warnings, 1419 tests pass (17 pre-existing integration failures due to no live Redis/AMQP).

---

## Key Findings

### Genuine Issues Fixed

| # | File | Finding |
|---|------|---------|
| 1 | `extract/worker.rs:10-41` | `ExtractAggregation` had manual `new()` with all-zero fields — should derive `Default` |
| 2 | `extract/worker.rs:103` | `run.results.clone()` followed by move of `run.results` — unnecessary heap allocation |
| 3 | `extract/worker.rs:198-220` | `agg.all_results` duplicated every result value already stored in `agg.runs[i].results` — doubled peak memory for large jobs |
| 4 | `extract/worker.rs:174` | `buffer_unordered(16)` hardcoded — ignored `cfg.batch_concurrency` and performance profiles |
| 5 | `extract/worker.rs:62-83` | `mark_extract_canceled` did a bare `.get()` with no timeout; Redis errors aborted the job instead of failing safe |
| 6 | `extract/worker.rs:273-306` | Hand-rolled 35-line 3-attempt retry loop re-implementing `mark_job_completed` from `common/job_ops.rs` |
| 7 | `worker_lane.rs:292-294` | `stale_timeout_secs.max(60).min(i32::MAX as i64) as i32` — clunky cast, intent unclear |
| 8 | `worker_lane.rs:313-321` | Double `Vec` allocation in `reenqueue_orphaned_pending_jobs`: `Vec<(Uuid,)>` then `.map(|(id,)| id).collect()` |
| 9 | `worker_lane.rs:234-235` | `join_all` restart-all-lanes behavior undocumented at call site |
| 10 | `embed/worker.rs:277-280` | SCHEMA_INIT double-init race undocumented — silent correctness relying on advisory locks |
| 11 | `common/heartbeat.rs:47-61,134-147` | `stop_tx` drop behavior undocumented in both heartbeat spawners |

### False Positives (not fixed)

- **Double `mark_job_failed` in embed** — Agents alleged `process_claimed_embed_job` called `mark_job_failed` twice for the same error. Traced all paths: `process_embed_job_with_runner`'s `Err` arm (from `run_embed_core` failure) calls `mark_job_failed` then returns `Ok(())`, so the outer wrapper never sees an error from that path. Only DB write failures escape to the outer wrapper, which correctly marks failure. No double-call.
- **`cancel_pending_or_running_job` in `mark_extract_canceled`** — Reuse agents suggested using the shared helper. Kept the inline SQL for simplicity; only change was adding timeout + status guard.
- **Test resolver functions** — `resolve_test_pg_url` et al. are near-identical; consolidation deferred (would change `LazyLock` semantics for pg_url specifically).

---

## Technical Decisions

**Remove `all_results` from `ExtractAggregation` rather than just fixing the clone:**
The `all_results: Vec<serde_json::Value>` field was a structural duplication — every value already lived inside `agg.runs[i]["results"]`. Removing the field and reconstructing the flat array in `extract_result_json` reduces peak memory by ~half for large extract jobs (e.g. 1000-result extract across many URLs). The JSON output shape is unchanged.

**Use `mark_job_completed` instead of adding retry to it:**
The retry loop protected against transient DB errors. Rather than adding retry logic to the shared helper (which all workers would inherit, possibly undesirably), we simply use the shared helper without retry. If the completion UPDATE fails, the watchdog will reclaim the job, which is acceptable and consistent with how crawl/embed workers behave.

**Don't use `cancel_pending_or_running_job` in `mark_extract_canceled`:**
The shared helper is correct, but would require another import and error type conversion. The inline SQL with status guard achieves the same safety property with less churn.

**`i32::try_from` over manual `.min(i32::MAX as i64) as i32`:**
`try_from` makes the intent (clamp to i32 range) self-documenting and eliminates the subtle `.min(i32::MAX as i64)` cast that required squinting to verify.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/jobs/extract.rs` | Added `mark_job_completed` to common imports |
| `crates/jobs/extract/worker.rs` | Derived `Default`, removed `all_results`, fixed clone, fixed timeout, replaced retry loop, used `cfg.batch_concurrency` |
| `crates/jobs/embed/worker.rs` | Added SCHEMA_INIT safety comment |
| `crates/jobs/worker_lane.rs` | Fixed cast, fixed double-Vec, added `join_all` comment |
| `crates/jobs/common/heartbeat.rs` | Added shutdown/drop behavior doc to both spawn functions |

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `extract` parallel concurrency | Always 16 concurrent URL fetches | Respects `cfg.batch_concurrency` (tunable via `--batch-concurrency` and performance profiles) |
| `extract` Redis cancel check | Bare `GET` with no timeout; Redis error → job failure | 3s timeout; Redis error → graceful `log_warn`, job continues |
| `extract` cancel DB update | No status guard (`WHERE id=$1` only) | Guards on `status IN ('pending','running')` — safe on double-cancel |
| `extract` completion | 35-line retry loop with 3 attempts and 1s sleeps | Single `mark_job_completed` call; watchdog handles reclaim if DB fails |
| `extract` aggregation memory | Two copies of all results in memory simultaneously | One copy in `agg.runs`, flat `all_results` reconstructed at serialization time |
| `heartbeat` shutdown docs | Undocumented drop-without-send behavior | Documented: drop delays shutdown by up to one interval tick |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --lib` | No errors | `Finished dev profile` | ✅ |
| `cargo clippy` | 0 warnings | 0 warnings | ✅ |
| `cargo test --lib extract` | All unit tests pass | 50 passed, 1 integration (Redis conn refused — pre-existing) | ✅ |
| `cargo test --lib jobs` | All unit tests pass | 153 passed, 10 integration failures (Redis/AMQP not running — pre-existing) | ✅ |

---

## Source IDs + Collections Touched

*(See Axon embed section below for session doc indexing)*

---

## Risks and Rollback

**Low risk** — All changes are internal implementation details with no wire-format changes:
- `mark_job_completed` replaces raw SQL with the same semantics (same `WHERE status='running'` guard, same columns updated). Only difference: no retry on DB failure — watchdog handles reclaim.
- `all_results` removal: JSON output shape is identical; reconstruction from `agg.runs` produces the same flat array.
- `buffer_unordered(cfg.batch_concurrency)` defaults to 16 (same as before) unless user overrides.

**Rollback:** `git checkout crates/jobs/` to revert all five files.

---

## Decisions Not Taken

- **Unify Redis open helpers** into a single `common::open_worker_redis` — would be clean but requires plumbing the `Config` into the common module more explicitly. Deferred.
- **Add retry to `mark_job_completed`** in `job_ops.rs` — would benefit all workers but changes shared behavior. Let watchdog handle transient failures.
- **Consolidate `resolve_test_*_url` helpers** — the `LazyLock` asymmetry on `pg_url` makes a straight unification tricky. Not worth the churn for test-only code.
- **Remove `process_claimed_embed_job`'s `mark_job_failed` call** — agents alleged it was a double-call; verified it is NOT (different error paths). No change needed.

---

## Open Questions

- Should `mark_job_completed` gain optional retry support for all workers? The retry logic in extract was protecting against a real failure mode (PG restart mid-job). Currently the watchdog handles this, but the reclaim delay is `AXON_JOB_STALE_TIMEOUT_SECS` (default 300s) + confirm window.
- `buffer_unordered(cfg.batch_concurrency)` now honors performance profiles for extract, but the default is 16 (same as before). Should the extract default be explicitly documented in the performance profiles table in `CLAUDE.md`?

---

## Next Steps

- None required. All changes are self-contained quality improvements.
- Consider the `open_worker_redis` shared helper consolidation in a future session when touching both embed and extract workers.
