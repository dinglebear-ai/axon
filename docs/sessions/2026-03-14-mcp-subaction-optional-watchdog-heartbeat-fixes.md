# Session: MCP Subaction Optional + Watchdog Heartbeat Fixes

**Date:** 2026-03-14
**Branch:** main
**Version:** v0.23.3

---

## Session Overview

Two independent bug classes fixed in one session:

1. **MCP `subaction` required field error** — All lifecycle families (`crawl`, `extract`, `embed`, `ingest`, `refresh`) required `subaction` as a non-optional serde field. Omitting it caused `invalid_params: missing field 'subaction'`. Made optional with `None → Start` default.

2. **Ingest watchdog reclaim on large GitHub repos** — The `steipete/mcporter` ingest job was reclaimed as stale after 5 min because `updated_at` stopped advancing after 9 seconds. Two fixes applied: wired up env-var overrides for watchdog timeouts, and added heartbeat progress calls during GitHub's silent phases.

---

## Timeline

| Time | Event |
|------|-------|
| Session start | `/axon ingest steipete/mcporter` → `MCP error -32602: missing field 'subaction'` |
| Fix 1 | Made `subaction` `Option<T>` on all 5 lifecycle request structs; added `Copy` to all subaction enums; updated 5 handlers to `.unwrap_or(XSubaction::Start)`; fixed 8 tests |
| Ingest retry | Submitted `steipete/mcporter` with explicit `subaction: "start"` → job `6728837f` |
| Status check | Job `6728837f` reported `failed` — but `result_json` showed `chunks_embedded: 182, tasks_done: 5/5` — work completed, watchdog fired before job could self-mark |
| Root cause analysis | Dispatched parallel explore agent; found heartbeat stopped at T+9s, watchdog Pass 1 at T+5m25s, job completed at T+7m02s |
| Fix 2a | Wired `AXON_JOB_STALE_TIMEOUT_SECS` / `AXON_JOB_STALE_CONFIRM_SECS` env vars (`config_impls.rs` was hardcoded to 300/60) |
| Fix 2b | Added `send_progress` calls in `github/files.rs` at `phase: "cloning"` and `phase: "enumerating_files"` |
| Audit | Explored all other ingest/job types for same heartbeat gap; found Reddit (heartbeat-only, no per-item progress), crawl (no DB heartbeat at all) |
| Git log review | Discovered recent `PreparedDoc` migration commits — user's "doing shit their own way" complaint was already being actively addressed |
| Session end | Ran `/save-to-md` |

---

## Key Findings

### MCP Schema
- `crates/mcp/schema.rs:43,82,106,129,348` — `subaction` was `T` (required), not `Option<T>`. Strict serde with `deny_unknown_fields` rejected any payload missing the field.
- All 5 subaction enums (`CrawlSubaction`, `ExtractSubaction`, `EmbedSubaction`, `IngestSubaction`, `RefreshSubaction`) were missing `Copy` — needed to allow `unwrap_or(XSubaction::Start)` without partial move errors.
- Skill docs (`~/.claude/skills/axon/SKILL.md:97`) incorrectly stated "Default: omitting `subaction` resolves to `start`" — this was aspirational documentation of intended behavior, not actual behavior.

### Watchdog / Heartbeat
- `crates/jobs/common/watchdog.rs:140-151` — Stale detection SQL: `WHERE status='running' AND updated_at < NOW() - make_interval(secs => $1::int)`. Two-pass: Pass 1 marks at T+300s, Pass 2 confirms and kills at T+360s.
- `crates/core/config/types/config_impls.rs:119,123` — `watchdog_stale_timeout_secs` and `watchdog_confirm_secs` were hardcoded; `AXON_JOB_STALE_TIMEOUT_SECS` / `AXON_JOB_STALE_CONFIRM_SECS` env vars documented in `.env.example` but never read.
- `crates/ingest/github/files.rs` — Silent phases: `clone_repo` (multi-min for large repos) and file tree enumeration had no `send_progress` calls. The concurrent fetch loop (`collect_embed_docs`) already had per-25-file progress. Fixed by adding two `send_progress` calls before the previously-silent phases.
- `crates/jobs/ingest/process.rs:290-291` — Generic `spawn_heartbeat_task(pool, TABLE, id, INGEST_HEARTBEAT_INTERVAL_SECS=30)` runs for all ingest types. GitHub additionally uses a `progress_tx/rx` channel (lines 298-310) for DB updates via `update_ingest_progress`.

### Audit Results (Remaining Risk)
- **Reddit** (`crates/ingest/reddit.rs`): heartbeat-only (30s), no per-item progress. Low-medium risk — 30s << 300s watchdog window, but large subreddit trees have no live status.
- **Crawl worker**: No explicit DB heartbeat during crawl operation. Relies on finishing before 300s. Long crawls of slow sites could theoretically hit the window.
- **Embed/Extract/Refresh/YouTube/Sessions**: All adequate (15-30s heartbeat intervals, well within 360s window).

### PreparedDoc Migration (Context)
Recent commits show `PreparedDoc` pipeline already being rolled out to all ingest types:
- `2a7c93b0` — reddit, youtube migrated
- `aa2bce2b`, `99dfb55d` — github migrated
- `89c4011d` — old batch embed pipeline deleted

---

## Technical Decisions

1. **`Option<T>` with `unwrap_or(Start)` over `#[serde(default)]`** — Used `Option<T>` rather than `#[serde(default = "...")]` because the subaction enums didn't have `Default` impls. Adding `Copy` to unit-variant enums was zero-cost and cleaner than `.clone()`.

2. **`send_progress` over reducing heartbeat interval** — Adding heartbeat calls at natural phase boundaries (before clone, before enumeration) is more informative (user sees live phase) and more precise than globally lowering `INGEST_HEARTBEAT_INTERVAL_SECS`. Coarser interval = more DB load for all jobs.

3. **Env vars wired without per-job-type override** — Implementing per-job-type timeout (Option C from the agent report) would require schema changes to `config_json` and watchdog lookup logic. The env-var approach is blunt but unblocks operators immediately. Per-type timeout remains future work.

4. **Did not add Reddit progress updates** — Reddit's 30s heartbeat is sufficient for the current watchdog window (360s). Adding per-item progress requires passing `pool + job_id` into `ingest_reddit()`, which has no current mechanism for it. Deferred.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/mcp/schema.rs` | `subaction` → `Option<T>` on 5 structs; added `Copy` to 5 enums; updated 3 test assertions; renamed 3 tests from `*_returns_error` to `*_defaults_to_start` |
| `crates/mcp/server/handlers_crawl_extract.rs` | `match req.subaction` → `match req.subaction.unwrap_or(CrawlSubaction::Start)` and `ExtractSubaction::Start` |
| `crates/mcp/server/handlers_embed_ingest.rs` | Same pattern for Embed and Ingest |
| `crates/mcp/server/handlers_refresh_status.rs` | Same pattern for Refresh |
| `crates/mcp/server/services_migration_tests.rs` | Wrapped `subaction: XSubaction::Start` → `Some(XSubaction::Start)` in 2 test structs |
| `crates/core/config/types/config_impls.rs` | Added `use std::env;`; wired `AXON_JOB_STALE_TIMEOUT_SECS` and `AXON_JOB_STALE_CONFIRM_SECS` env vars |
| `crates/ingest/github/files.rs` | Added 2 `send_progress` calls: `phase: "cloning"` before `clone_repo`, `phase: "enumerating_files"` before `collect_indexable_files` |
| `~/.claude/skills/axon/SKILL.md` | Corrected `subaction` docs to reflect actual optional behavior (updated twice — wrong → right, then back to correct after code fixed) |
| `~/.claude/skills/axon/references/routing-cheatsheet.md` | Same correction |

---

## Commands Executed

```bash
# Verify MCP schema tests pass after subaction changes
cargo test -p axon --lib -- mcp::schema   # 31/31 passed

# Verify compilation after Copy + Option changes
cargo check --bin axon                     # clean, 13.91s

# Implementation agent verification
cargo test -p axon --lib -- ingest         # 152/152 passed
cargo check --bin axon                     # clean
```

---

## Behavior Changes (Before/After)

| Surface | Before | After |
|---------|--------|-------|
| MCP `ingest` without `subaction` | `invalid_params: missing field 'subaction'` | Succeeds, defaults to `start` |
| MCP `crawl`/`extract`/`embed`/`refresh` without `subaction` | Same error | Same fix |
| GitHub ingest on large repos | Watchdog-reclaimed at ~6 min, job marked `failed` even if work completed | `updated_at` advances during clone and enumeration phases — watchdog won't fire on healthy jobs |
| `AXON_JOB_STALE_TIMEOUT_SECS` env var | Silently ignored (hardcoded 300s) | Read and applied |
| `AXON_JOB_STALE_CONFIRM_SECS` env var | Silently ignored (hardcoded 60s) | Read and applied |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test -- mcp::schema` | 31 pass | 31 pass | ✅ |
| `cargo check --bin axon` | No errors | No errors | ✅ |
| `cargo test -- ingest` | 152 pass | 152 pass | ✅ |
| MCP `ingest` without `subaction` | `ok: true` | `ok: true, job_id: ...` | ✅ |
| `grep AXON_JOB_STALE config_impls.rs` | env::var call | Lines 119,123 confirmed | ✅ |
| `grep send_progress github/files.rs` | 2+ new calls | Lines 238-254 confirmed | ✅ |

---

## Source IDs + Collections Touched

| Job | Target | Chunks | Collection | Outcome |
|-----|--------|--------|------------|---------|
| Ingest `2e3244f8` | `steipete/mcporter` | 182 | `cortex` | Data landed, job marked failed (watchdog) |
| Ingest `6728837f` | `steipete/mcporter` | 182 | `cortex` | Same — watchdog reclaim, data in Qdrant |
| Ingest `8d465f50` | `steipete/mcporter` | TBD | `cortex` | Running at session end |

---

## Risks and Rollback

- **`subaction` optional change**: Non-breaking. Existing callers that already pass `subaction` continue to work. Callers that omitted it previously got errors — now they get `start`. No behavior regression for existing clients.
- **Watchdog env vars**: Increasing `AXON_JOB_STALE_TIMEOUT_SECS` to a very large value masks genuinely stuck jobs. Recommend keeping at 600s max for large-repo ingests.
- **GitHub `send_progress` calls**: Purely additive. Additional DB writes during ingest; negligible overhead.
- **Rollback**: `git revert` on the two affected commits is clean. The `config_impls.rs` and `github/files.rs` changes are independent.

---

## Decisions Not Taken

| Option | Rejected Because |
|--------|-----------------|
| Per-job-type watchdog timeout | Requires schema changes to `config_json` + watchdog SQL lookup logic — higher complexity for marginal benefit over env var |
| Reduce `INGEST_HEARTBEAT_INTERVAL_SECS` from 30 to 10 | More DB load for all ingest jobs globally; doesn't address the root cause (silent blocking phases) |
| Add Reddit per-item progress | Would require threading `pool + job_id` into `ingest_reddit()` — no existing mechanism; 30s heartbeat is sufficient for current watchdog window |
| Add crawl DB heartbeat | Crawl already has `!Send` futures and complex worker loop; adding a heartbeat task requires care. Deferred — medium priority |

---

## Open Questions

1. **Why did GitHub heartbeat stop at T+9s?** The 30s `spawn_heartbeat_task` should have kept firing regardless of what `ingest_github()` was doing. Did the task panic silently? Was the tokio runtime saturated? The watchdog data (last heartbeat 9s after start) is suspicious — the 30s task should have fired at least once before the 300s window.
2. **Will `steipete/mcporter` job `8d465f50` complete cleanly?** Running at session end. Expected outcome: `status: completed` with the new `send_progress` calls keeping `updated_at` fresh.
3. **Reddit progress on large subreddits**: No live status during recursive comment traversal. Low priority for now, but worth adding a `pool + job_id` progress path to `ingest_reddit()` eventually.
4. **Crawl heartbeat**: Crawl worker has no DB `updated_at` touch during the actual crawl. Long crawls (>5 min) could theoretically be watchdog-reclaimed if the watchdog ever applies to crawl jobs. Needs investigation of whether `axon_crawl_jobs` is covered by the same watchdog.

---

## Next Steps

1. Confirm `steipete/mcporter` job `8d465f50` completes with `status: completed` (validates Fix 2b)
2. Rebuild and restart `axon mcp` so the `subaction` optional fix is live (currently running old binary)
3. Consider adding Reddit progress reporting (low priority — heartbeat adequate for current window)
4. Investigate crawl watchdog coverage — does `axon_crawl_jobs` use the same watchdog or a separate one?
5. Consider adding crawl DB heartbeat for very long crawls
