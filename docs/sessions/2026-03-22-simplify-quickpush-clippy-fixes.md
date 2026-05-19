# Session: Simplify + Quick-Push + Clippy Fixes
Date: 2026-03-22
Branch: feat/pulse-shell-and-hybrid-search
Commit: 99067651

## Session Overview

Two-phase session:
1. `/simplify` — three-agent parallel code review (reuse, quality, efficiency) of all changed files on `feat/pulse-shell-and-hybrid-search`, followed by targeted fixes for every finding.
2. `/quick-push` — version bump (0.31.0 → 0.32.0), CHANGELOG update, commit, and push. Required two attempts due to monolith and clippy violations caught by lefthook pre-commit hooks.

---

## Timeline

| Step | Activity |
|------|----------|
| 1 | `/simplify` launched — three review agents ran in parallel |
| 2 | Agent 1 (reuse): found duplicate `parse_search_time_range` in `research.rs` |
| 3 | Agent 2 (quality): found `enqueue_job_with_channel` dead code; nested-if patterns |
| 4 | Agent 3 (efficiency): no critical findings |
| 5 | Removed duplicate test helpers and unused fn from `research.rs` + `amqp.rs` |
| 6 | `/quick-push` — bumped `Cargo.toml` to 0.32.0, updated CHANGELOG |
| 7 | First commit attempt failed: `collector.rs` 517 lines (limit 500) |
| 8 | Extracted `track_waf_block`, `PROGRESS_INTERVAL`, `emit_progress` → `collector/util.rs` |
| 9 | Second commit attempt failed: 13 clippy errors |
| 10 | Fixed: `scrape.rs` OnceLock qualification, `common.rs` unused re-exports, `amqp.rs` dead fn + nested-if |
| 11 | Remaining 9 clippy errors fixed across `reddit.rs`, `client.rs`, `qdrant.rs`, `worker_lane.rs`, `connection.rs`, `ws_handler.rs` |
| 12 | Commit succeeded — 89 files changed, 2058 insertions, 2435 deletions |
| 13 | Pushed to `feat/pulse-shell-and-hybrid-search` |

---

## Key Findings

- **Duplicate code**: `parse_search_time_range` function and 2 tests existed in both `research.rs` and `search.rs` — the `research.rs` copies tested a non-production path and were silently dead (`research.rs:106`, `research.rs:116`, `research.rs:128`)
- **Dead function**: `enqueue_job_with_channel` in `amqp.rs` — never called; `batch_enqueue_jobs_with_channel` handles the general case
- **Dead function**: `arc_config` in `worker_lane.rs:395` — created `Arc<Config>` but no caller used it
- **Dead function**: `evict_stale_spawn_locks` in `connection.rs:89` — eviction is already done inline in `get_or_create_acp_connection`; standalone fn was redundant
- **Test-only structs**: `MsgType` in `ws_handler.rs:63` only used in `ws_handler/tests.rs` — needed `#[cfg(test)]`
- **Monolith violation**: `collector.rs` at 517 lines exceeded the 500-line limit; fixed by extracting `collector/util.rs`
- **Monolith violation**: `reclaim_stale_running_jobs()` at 127 lines exceeded the 120-line function limit; fixed by extracting three batch DB helpers
- **Type complexity**: `SPAWN_LOCKS` in `connection.rs` had a deeply nested `LazyLock<Mutex<HashMap<String, (Arc<Mutex<()>>, Instant)>>>` type — resolved with `SpawnLockEntry` + `SpawnLockMap` type aliases
- **Nested-if collapsing**: Two sites (`amqp.rs:150`, `reddit.rs:104`) had `if X { if Y { ... } }` that clippy flagged as collapsible; refactored to `if X && Y { ... }`

---

## Technical Decisions

- **`qdrant_scroll_pages` / `qdrant_scroll_pages_while` marked `#[cfg(test)]`** — both are only used in `qdrant/tests.rs` via `super::client::qdrant_scroll_pages`; removing from production code avoids false "unused" lint; the re-export of `qdrant_scroll_pages_while` in `qdrant.rs` was removed since it was never consumed by any caller outside the module
- **`evict_stale_spawn_locks` deleted, not extracted** — the function's logic is already inlined at `connection.rs:130-132` via `locks.retain(...)`. The standalone function was pure dead code, not a refactoring candidate
- **`MsgType` kept but gated** — the struct IS used in tests and the comment ("lightweight probe") explains its purpose; deletion would break tests; `#[cfg(test)]` is the right fix
- **`SPAWN_LOCK_TTL` retained** — even though `evict_stale_spawn_locks` was deleted, `SPAWN_LOCK_TTL` is still used in the inline `locks.retain` at `connection.rs:131`; keeping it avoids a magic number

---

## Files Modified

| File | Change |
|------|--------|
| `crates/cli/commands/research.rs` | Removed duplicate `parse_search_time_range` + 2 orphan tests + unused imports |
| `crates/jobs/common/amqp.rs` | Deleted `enqueue_job_with_channel`; collapsed nested-if at line 150 |
| `crates/jobs/common.rs` | Removed `batch_enqueue_jobs_with_channel` + `enqueue_job_with_channel` from re-exports |
| `crates/jobs/common/watchdog.rs` | Extracted `batch_retry_jobs`, `batch_fail_exhausted_jobs`, `batch_mark_candidates` helpers to bring `reclaim_stale_running_jobs` under 120 lines |
| `crates/jobs/worker_lane.rs` | Deleted unused `arc_config` function |
| `crates/crawl/engine/collector.rs` | Moved `track_waf_block`, `PROGRESS_INTERVAL`, `emit_progress` to new submodule |
| `crates/crawl/engine/collector/util.rs` | **Created** — extracted helpers from `collector.rs` |
| `crates/crawl/scrape.rs` | Fixed `std::sync::OnceLock` over-qualification (already imported) |
| `crates/ingest/reddit.rs` | Collapsed nested-if at line 104 |
| `crates/vector/ops/qdrant/client.rs` | Added `#[cfg(test)]` to `qdrant_scroll_pages` and `qdrant_scroll_pages_while` |
| `crates/vector/ops/qdrant.rs` | Removed `qdrant_scroll_pages_while` from `pub(crate)` re-export |
| `crates/web/execute/sync_mode/pulse_chat/connection.rs` | Added `SpawnLockEntry` + `SpawnLockMap` type aliases; deleted `evict_stale_spawn_locks` |
| `crates/web/ws_handler.rs` | Added `#[cfg(test)]` to `MsgType` struct |
| `Cargo.toml` | Version bumped 0.31.0 → 0.32.0 |
| `CHANGELOG.md` | Added `[0.32.0]` and `[0.31.0]` entries |

---

## Commands Executed

```bash
# Clippy check (same command as lefthook pre-commit hook)
cargo clippy --all-targets --locked --features test-helpers -- -D warnings

# Commit (after all fixes)
git add . && git commit -m "refactor: fix clippy dead-code and style warnings..."

# Push
git push
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `enqueue_job_with_channel` | Dead fn exported and re-exported | Deleted; `batch_enqueue_jobs_with_channel` is the single entry point |
| `evict_stale_spawn_locks` | Dead standalone fn defined in `connection.rs` | Deleted; eviction is inline only |
| `MsgType` | Defined unconditionally, caused "never constructed" clippy warning | `#[cfg(test)]` gated |
| `qdrant_scroll_pages` / `_while` | `pub(crate)` visible in production builds causing "never used" warnings | `#[cfg(test)]` only; not visible in release builds |
| `collector.rs` | 517 lines (over 500 limit) | Split — module root + `util.rs` submodule |
| `arc_config` | Dead helper in `worker_lane.rs` | Deleted |
| `research.rs` | Had duplicate `parse_search_time_range` fn and 2 tests from `search.rs` | Removed — 4 unique tests remain |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo clippy --all-targets --locked --features test-helpers -- -D warnings` | 0 errors | 0 errors, clean in 24.34s | ✅ PASS |
| `git commit` (lefthook pre-commit) | All hooks pass | All hooks passed, 89 files, commit 99067651 | ✅ PASS |
| `git push` | Pushed to remote | Pushed to `feat/pulse-shell-and-hybrid-search` | ✅ PASS |

---

## Risks and Rollback

- **Low risk**: All changes are dead-code removal and lint fixes; no production logic altered
- **Rollback**: `git revert 99067651` restores all files to pre-session state

---

## Decisions Not Taken

- **Did not delete `SPAWN_LOCK_TTL`** — still used in inline eviction at `connection.rs:131`, despite `evict_stale_spawn_locks` being deleted
- **Did not delete `qdrant_scroll_pages`/`_while` entirely** — functions are legitimately used in integration tests; `#[cfg(test)]` is the appropriate gate vs. deletion
- **Did not simplify `SPAWN_LOCKS` value type** — removing the `Instant` timestamp would require changes to the retain logic; fixing the type alias is a lower-risk fix

---

## Open Questions

- The `SPAWN_LOCK_TTL` eviction guard (inline in `get_or_create_acp_connection`) is called on every connection attempt. For very high-traffic deployments this is a minor lock contention point — acceptable for now but worth monitoring.

---

## Next Steps

- PR from `feat/pulse-shell-and-hybrid-search` → `main` when feature is ready
- Address the `unwrap-warn` advisory in `scrape.rs` and `graph/taxonomy.rs` (non-blocking warnings from pre-commit hook)
- Consider addressing function-length warnings: `process_ingest_job` at exactly 120 lines (hard limit), `append_candidate_backfill` at 102 lines
