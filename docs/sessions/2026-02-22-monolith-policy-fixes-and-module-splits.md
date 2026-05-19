# Session: Monolith Policy Fixes and Module Splits

**Date:** 2026-02-22
**Branch:** `perf/command-performance-fixes`
**Duration:** Single focused session

---

## Session Overview

Fixed a gap in the monolith enforcement policy where inline `#[cfg(test)] mod tests { ... }` blocks were counted toward file line limits but the actual test *files* (`tests/**`) were excluded. This caused 4 legitimate Rust source files to be incorrectly listed in `.monolith-allowlist`. After fixing the policy script, two genuinely oversized files (`batch_jobs.rs`, `ask.rs`) were refactored via module splits to clear their exemptions. Final result: `.monolith-allowlist` went from 7 entries to 1.

---

## Timeline

1. **Reviewed `.monolith-allowlist`** — 7 entries, 6 Rust files + 1 Python script
2. **Identified policy gap** — `enforce_monoliths.py` `file_line_count()` counted all lines raw; `parse_rust_functions()` already skipped `#[cfg(test)] mod tests {}` for function checks but file-size checks had no equivalent exclusion
3. **Fixed `enforce_monoliths.py`** — rewrote `file_line_count()` for `.rs` files to skip `#[cfg(test)] mod tests { ... }` blocks using brace-depth tracking
4. **Audited all allowlist files** — ran updated counter against all 6 Rust files to find which entries were falsely required
5. **Dropped 4 entries** — `http.rs` (202 real lines), `ranking.rs` (498), `worker_lane.rs` (370), `engine.rs` (469) all pass without exemption
6. **Split `batch_jobs.rs`** — 557 non-test lines; extracted queue injection logic to `batch_jobs/queue_injection.rs`; root file reduced to 209 lines
7. **Split `ask.rs`** — 569 lines; extracted context pipeline to `ask/context.rs`; root file reduced to 164 lines
8. **Cleared final 2 allowlist entries** — both files now pass the 500-line limit
9. **Ran full test suite** — 336 passing, 0 failures

---

## Key Findings

- `enforce_monoliths.py:103-105` — `file_line_count()` was a raw `len(splitlines())` with no test exclusion, while `parse_rust_functions():130-210` already had full `#[cfg(test)] mod tests` brace-depth skipping — inconsistent by design or oversight
- `batch_jobs.rs` had 557 *production* lines even after tests were excluded — queue injection rule engine (~300 lines) was the obvious extraction target
- `ask.rs` had 569 lines — context building pipeline (lines 14–416) was a clean cohesive unit separate from the command runner + output layer (lines 418–569)
- `batch_jobs/tests.rs` is NOT matched by any `EXCLUDED_GLOBS` pattern (`**/tests/**` requires `tests` to be a directory, not a filename) — currently 140 lines so benign, but will falsely trigger if it grows
- `use super::*` in Rust child modules picks up parent's private imports (child modules can access parent scope per Rust privacy rules) — key reason the `worker.rs` pattern works without explicit re-exports for all items

---

## Technical Decisions

- **`file_line_count()` skips `#[cfg(test)] mod tests` only** — individual `#[test]` functions outside a test module still count. Matches the existing function-level behavior exactly and is the least-surprise change.
- **`batch_jobs/queue_injection.rs` as submodule** — follows the existing `batch_jobs/worker.rs` + `batch_jobs/maintenance.rs` pattern already in the codebase; natural Rust module split
- **`ask/context.rs` as submodule** — `ask.rs` + `ask/` directory pattern; same as `batch_jobs.rs` + `batch_jobs/`
- **`pub use queue_injection::*` + explicit `pub(crate) use`** — preserves exact original visibility: public types are re-exported as pub, `apply_queue_injection_with_pool` kept as `pub(crate)`
- **Did not fix `**/tests.rs` glob** — deferred; current `tests.rs` files are small and the fix wasn't requested

---

## Files Modified

| File | Change | Lines Before → After |
|------|--------|----------------------|
| `scripts/enforce_monoliths.py` | Rewrote `file_line_count()` to skip `#[cfg(test)] mod tests` blocks for `.rs` files | 313 → 343 |
| `.monolith-allowlist` | Removed 6 entries; only `scripts/qdrant-quality.py` remains | 37 → 9 |
| `crates/jobs/batch_jobs.rs` | Removed queue injection code; added `mod queue_injection` + re-exports | 559 → 209 |
| `crates/jobs/batch_jobs/queue_injection.rs` | **Created** — queue injection structs + functions extracted from `batch_jobs.rs` | 0 → 362 |
| `crates/vector/ops/commands/ask.rs` | Removed context pipeline; added `mod context` + re-exports | 569 → 164 |
| `crates/vector/ops/commands/ask/context.rs` | **Created** — `AskContext`, `build_ask_context`, and all pipeline helpers | 0 → 407 |

---

## Commands Executed

```bash
# Audit non-test line counts for all allowlisted files
python3 -c "..." # custom script using updated file_line_count logic
# Result: http.rs=202, ranking.rs=498, worker_lane.rs=370, engine.rs=469 (all ok)
#         batch_jobs.rs=557 (OVER), ask.rs=569 (OVER)

# Verify splits and run test suite
cargo test --lib
# Result: 336 passed; 0 failed
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| `file_line_count()` for `.rs` | Counted all lines including inline test modules | Skips `#[cfg(test)] mod tests { ... }` blocks |
| `.monolith-allowlist` entries | 7 (6 Rust + 1 Python) | 1 (Python only) |
| `batch_jobs.rs` size | 559 lines (557 non-test) | 209 lines |
| `ask.rs` size | 569 lines | 164 lines |
| Queue injection types/fns | Defined in `batch_jobs.rs` | Defined in `batch_jobs/queue_injection.rs`, re-exported from `batch_jobs` |
| Context pipeline | Defined in `ask.rs` | Defined in `ask/context.rs`, re-exported from `ask` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test --lib` | All tests pass | 336 passed, 0 failed | ✅ PASS |
| `file_line_count(http.rs)` | 202 non-test lines | 202 | ✅ PASS |
| `file_line_count(ranking.rs)` | ≤500 | 498 | ✅ PASS |
| `file_line_count(worker_lane.rs)` | ≤500 | 370 | ✅ PASS |
| `file_line_count(engine.rs)` | ≤500 | 469 | ✅ PASS |
| `file_line_count(batch_jobs.rs)` | ≤500 | 209 | ✅ PASS |
| `file_line_count(ask.rs)` | ≤500 | 164 | ✅ PASS |
| `wc -l ask/context.rs` | ≤500 | 407 | ✅ PASS |
| `wc -l batch_jobs/queue_injection.rs` | ≤500 | 362 | ✅ PASS |

---

## Risks and Rollback

- **Risk:** `batch_jobs/queue_injection.rs` is 362 lines — if the rule engine grows, it will need its own split
- **Risk:** `ask/context.rs` is 407 lines — `build_context_from_candidates` at ~91 lines triggers the 80-line function warning (not a hard fail)
- **Rollback:** `git revert` the two commits or restore original files from git history; no schema changes, no infra changes, no behaviour changes to running workers

---

## Decisions Not Taken

- **Fixing `**/tests.rs` glob in EXCLUDED_GLOBS** — `batch_jobs/tests.rs` is 140 lines and not a problem today; deferred to avoid unplanned scope
- **Splitting `queue_injection.rs` further** — 362 lines is under the 500-line limit; no need now
- **Moving `build_context_from_candidates` out of `context.rs`** — 91 lines triggers the warn threshold but not the hard-fail; the function is cohesive and splitting it would add noise

---

## Open Questions

- Should `**/tests.rs` filenames be added to `EXCLUDED_GLOBS` in `enforce_monoliths.py` to properly exclude files like `batch_jobs/tests.rs`? Currently unprotected but benign.
- `ask/context.rs:build_context_from_candidates` is 91 lines — will produce a monolith warning on next commit touching that function. Acceptable or should it be split?

---

## Next Steps

- Monitor `queue_injection.rs` and `context.rs` sizes as the codebase grows
- Consider adding `**/**/tests.rs` to `EXCLUDED_GLOBS` when convenient
- Commit this work to the branch
