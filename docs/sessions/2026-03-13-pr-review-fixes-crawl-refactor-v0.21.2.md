# Session: PR Review Fixes + Crawl Engine Refactoring (v0.21.2)
Date: 2026-03-13
Branch: fix/pr-review-fixes-crawl-refactor
Commit: 6afcfdc2

---

## Session Overview

Ran a comprehensive parallel PR review on the most recent commit (`ca7831c0`) in PR #42, then dispatched three parallel agents to address 8 of the identified findings. Also integrated pre-existing crawl engine refactors and pushed a new branch at v0.21.2.

---

## Timeline

1. **PR Review** — Invoked `/pr-review-toolkit:review-pr`, identified 12 changed files in HEAD~1..HEAD. Launched 3 parallel review agents (rust-reviewer, silent-failure-hunter, pr-test-analyzer).
2. **Findings Synthesized** — Aggregated results: 2 monolith violations (skipped per user), 1 `.expect()`, 5 important issues, 4 suggestions.
3. **Parallel Fix Dispatch** — Dispatched 3 parallel agents in worktree isolation:
   - Agent 1 (OAuth): `handlers_broker.rs` + `tests.rs`
   - Agent 2 (worker_lane): 4 issues in `worker_lane.rs`
   - Agent 3 (scaffolding): `evaluate.rs`, `streaming.rs`, `suggest.rs`, `display.rs`
4. **Integration** — OAuth changes landed in main workspace; agent 2/3 worktrees copied into main; `cargo check --lib` clean; new tests passing.
5. **Issue 8 Reverted** — `#[tokio::test]` → `#[test]` conversion failed: `GoogleOAuthState::from_env` calls `tokio::sync` primitives, requires Tokio runtime. Reverted both OAuth cookie tests back to `async fn`.
6. **Push** — Created `fix/pr-review-fixes-crawl-refactor` branch from main, bumped to v0.21.2, updated CHANGELOG, committed and pushed.

---

## Key Findings

- **`GoogleOAuthState::from_env` requires Tokio runtime** — `state.rs:65` panics "there is no reactor running" without `#[tokio::test]`. Even tests with no `.await` calls must use `#[tokio::test]` if they call this function. Issue 8 (remove async) was correctly identified as unnecessary overhead but is not safe to remove.
- **`worker_lane.rs` at 540 lines** — Exceeds 500-line monolith limit. Not fixed this session (requires file split, excluded from batch).
- **`streaming.rs` at 519 lines** — Same. Excluded.
- **SQL interval pattern** — `($2 || ' seconds')::INTERVAL` binds threshold as TEXT, bypassing sqlx type safety. `make_interval(secs => $2)` with an `i32` bind is the correct pattern.
- **`SideBySideBuffer::push` wildcard** — Silent `_ => {}` discards data when an unknown stream name is passed. Any rename of the stream constants would lose output silently.
- **`#[expect(dead_code)]` cross-module limitation** — Items in `evaluate.rs` and `streaming.rs` referenced from other modules cannot use `#[expect]` (would trigger `unfulfilled_lint_expectations`); kept `#[allow]` with reason comments.

---

## Technical Decisions

- **Reverted `#[tokio::test]` → `#[test]`**: `from_env` initializes `tokio::sync::RwLock` or similar; the async runtime is a hard dependency, not just an optimization.
- **`make_interval(secs => $2)` over string concat**: Type-safe binding — sqlx maps `i32` to Postgres `INTEGER`; the old pattern bound as TEXT and relied on Postgres string concatenation.
- **`#[expect(dead_code)]` selectively applied**: Three locations (display.rs, evaluate/streaming.rs, suggest.rs) genuinely have dead items → `#[expect]`. Two locations (evaluate.rs, streaming.rs) are cross-module live → kept `#[allow]` + reason comment.
- **Non-fatal orphaned job re-enqueue**: AMQP failure at worker startup should not block the main loop. The warn log (not error) is correct policy; the improvement is better log context (job_kind, threshold) not policy change.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/jobs/worker_lane.rs` | i32 clamp, make_interval SQL, TOCTOU comment, 2 unit tests | Issues 3, 5, 6, 7 |
| `crates/mcp/server/oauth_google/handlers_broker.rs` | `.expect()` → `unreachable!()` | Issue 2 |
| `crates/mcp/server/oauth_google/tests.rs` | Reverted `#[test]` back to `#[tokio::test]` | Issue 8 (reverted) |
| `crates/vector/ops/commands/evaluate.rs` | `log_warn` in wildcard arm, `#[allow]` + reason comments | Issues 4, 9 |
| `crates/vector/ops/commands/evaluate/display.rs` | `#![allow]` → `#![expect(dead_code, reason = "...")]` | Issue 9 |
| `crates/vector/ops/commands/evaluate/streaming.rs` | `#![allow]` → `#![expect(dead_code, reason = "...")]` | Issue 9 |
| `crates/vector/ops/commands/streaming.rs` | `#[allow]` + inline reason comments | Issue 9 |
| `crates/vector/ops/commands/suggest.rs` | `#[allow]` → `#[expect(dead_code, reason = "...")]` | Issue 9 |
| `crates/crawl/engine.rs` | `prepare_crawl_output_dir` helper extracted | Pre-existing refactor |
| `crates/crawl/engine/sitemap.rs` | `enqueue_robots_sitemaps` added | Pre-existing refactor |
| `crates/jobs/crawl/runtime/worker/process.rs` | `save_partial_cancel_result` added | Pre-existing refactor |
| `Cargo.toml` | `0.21.1` → `0.21.2` | Patch version bump |
| `CHANGELOG.md` | New highlight entry for v0.21.2 | Session documentation |

---

## Commands Executed

```bash
# Review scope
git diff --name-only HEAD~1 HEAD
gh pr view

# Verify all changes compile
cargo check --lib       # → Finished dev profile [unoptimized+debuginfo]

# Run new unit tests
cargo test orphaned_pending -- --nocapture
# → test orphaned_pending_threshold_enforces_60s_floor ... ok
# → test orphaned_pending_select_query_contains_table_and_placeholders ... ok

# Debug issue 8 failure
cargo test session_cookie_name_is_plain_on_http -- --nocapture
# → panicked at state.rs:65: there is no reactor running, must be called from context of Tokio 1.x runtime

# Push
git push -u origin fix/pr-review-fixes-crawl-refactor
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `handlers_broker.rs` redirect_to_login | `.expect()` could theoretically panic in library code | `unreachable!()` — semantically identical but policy-compliant |
| `orphaned_pending_threshold_secs` | `i64 as i32` silent overflow on pathological config | `.min(i32::MAX as i64) as i32` clamped |
| `reenqueue_orphaned_pending_jobs` SQL | `($2 || ' seconds')::INTERVAL` string-concat pattern | `make_interval(secs => $2)` type-safe i32 bind |
| `SideBySideBuffer::push` unknown stream | Silent data discard `_ => {}` | `log_warn` with stream name + byte count |
| Scaffolding dead code suppression | `#[allow(dead_code)]` — silent forever | `#[expect(dead_code, reason = "...")]` — compiler warns when wired up |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --lib` | Clean compile | `Finished dev profile` | ✅ |
| `cargo test orphaned_pending` | 2 tests pass | `2 passed; 0 failed` | ✅ |
| `cargo test session_cookie_name` | 2 tests pass | `2 passed; 0 failed` | ✅ |
| Pre-commit hook (lefthook) | All gates pass | `Monolith policy check passed` + 1262 tests ok | ✅ |
| `git push -u origin fix/pr-review-fixes-crawl-refactor` | New branch created | `[new branch]` | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations this session.

---

## Risks and Rollback

- **Monolith violations remain** — `worker_lane.rs` (540L) and `streaming.rs` (519L) still exceed the 500-line limit. Pre-commit hook shows warning but passes; CI gate is the authoritative check. Rollback: split test modules into sibling files.
- **`make_interval` SQL** — Postgres 8.4+ supports `make_interval`. Not a risk for this stack.
- **Rollback**: `git revert 6afcfdc2` cleanly undoes all changes.

---

## Decisions Not Taken

- **Issue 8 (`#[tokio::test]` → `#[test]`)**: Reverted after discovering `GoogleOAuthState::from_env` requires a Tokio runtime. The "unnecessary async" observation was correct but not safe to remove without refactoring `from_env` to be sync.
- **Monolith file splits** (issues 1 & 2 from review): Excluded from this batch — requires extracting `#[cfg(test)] mod tests` to sibling files, straightforward but a separate commit.
- **`batch_enqueue_jobs` partial failure** (critical from silent-failure review): Deferred — would require a larger refactor to track per-ID publish status.

---

## Open Questions

- `build_artifact_path_uses_action_subdirectory` test fails intermittently in parallel runs — pre-existing isolation issue, not caused by this session.
- Is `GoogleOAuthState::from_env` refactorable to be synchronous? If yes, issue 8 could be properly resolved.

---

## Next Steps

1. Split `worker_lane.rs` test module → `worker_lane/tests.rs` (monolith fix)
2. Split `streaming.rs` test module → `streaming/tests.rs` (monolith fix)
3. Investigate `build_artifact_path_uses_action_subdirectory` flaky test
4. Open PR for `fix/pr-review-fixes-crawl-refactor` targeting main
