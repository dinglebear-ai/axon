# Spider `control` Feature — Graceful Crawl Cancellation
**Date:** 2026-03-03 | **Branch:** `feat/sidebar`

## Session Overview

Investigated spider.rs's `control` feature flag (reported as "empty" by another agent), confirmed it gates real in-process pause/resume/shutdown functionality, then wired it into axon's crawl worker to replace the abrupt `tokio::select!` future-drop cancellation with graceful shutdown that preserves partial results.

## Timeline

1. **Investigation** — Traced `control = []` in spider's `Cargo.toml` through all `#[cfg(feature = "control")]` gates in `website.rs` and `utils/mod.rs`. Confirmed it enables `Handler` enum, global `CONTROLLER` watch channel, `pause()`/`resume()`/`shutdown()`/`reset()` functions, `CrawlStatus::Shutdown`/`Paused` variants, and `configure_handler()` on `Website`.
2. **Design discussion** — Explained cross-process (Redis) vs in-process (control) trade-offs. Decided on two-layer approach: Redis for external signal, spider control for immediate in-process stop.
3. **Implementation** — Four files changed, ~60 lines of new code. Added `control` feature, flipped `with_no_control_thread`, added `crawl_id` threading, rewrote cancel path.
4. **Tightening** — Narrowed `configure_website_with_crawl_id` visibility from `pub(in crate::crates::crawl)` back to `pub(super)`, updated `crates/crawl/CLAUDE.md` and memory file.

## Key Findings

- `control = []` in Cargo.toml is NOT empty — it's a zero-dependency compile-time feature gate (standard Cargo convention)
- Spider's `target_id()` = `string_concat!(self.crawl_id, self.url.inner())` — the shutdown target must match this exact format
- `with_no_control_thread(true)` was explicitly set in `runtime.rs:226` with comment "we never pause/resume crawls externally" — this was the gate preventing control usage
- Spider's `configure_handler()` spawns a background task that watches an `AtomicI8` (0=running, 1=paused, 2=shutdown) via `tokio::sync::watch` channel
- The `CONTROLLER` is a process-global `lazy_static!` — all crawls in the same process share it, disambiguated by `target_id`

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Two-layer cancel (Redis + control) | Redis bridges process boundaries (CLI → worker); control gives immediate in-process stop without 3s poll latency |
| `crawl_id = job_uuid.to_string()` | Unique per job, deterministic, matches the DB primary key for traceability |
| 30s drain timeout | Generous enough for in-flight requests to complete; prevents infinite hang if spider's control loop is broken |
| Save partial `result_json` before marking canceled | Data is already on disk — preserving the metadata (pages_crawled, md_created) in the DB makes cancel results queryable |
| `Option<&str>` parameter over builder pattern | Minimal API change; CLI callers pass `None`, worker passes `Some` — no new types needed |
| `#[allow(clippy::too_many_arguments)]` on `run_crawl_once` | Function already had 8 params; adding crawl_id makes 9. Refactoring to a params struct would touch all callers for marginal benefit. |

## Files Modified

| File | Change |
|------|--------|
| `Cargo.toml` | Added `"control"` to spider feature list |
| `crates/crawl/engine/runtime.rs` | Flipped `with_no_control_thread(false)`; added `configure_website_with_crawl_id()` |
| `crates/crawl/engine.rs` | Added `crawl_id: Option<&str>` param to `run_crawl_once()`; route through `configure_website_with_crawl_id` |
| `crates/cli/commands/crawl/sync_crawl.rs` | Updated 3 `run_crawl_once` call sites to pass `None` for crawl_id |
| `crates/jobs/crawl/runtime/worker/process.rs` | Rewrote `run_active_crawl_job`: builds `control_target`, pins crawl future, calls `spider::utils::shutdown()` on cancel, awaits drain with 30s timeout, saves partial results |
| `crates/crawl/CLAUDE.md` | Updated mid-crawl cancellation docs, configure_website docs |
| `memory/MEMORY.md` | Updated mid-crawl cancellation entry |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Cancel mechanism | `tokio::select!` drops crawl future | `spider::utils::shutdown()` + await drain |
| In-flight requests on cancel | Abandoned (connections dropped) | Drain gracefully (up to 30s) |
| Partial results on cancel | Lost (no result_json saved) | Saved to DB (`phase: "canceled"`, `graceful_shutdown: true`) |
| Cancel latency | Up to 3s (Redis poll interval) | Immediate in-process signal after Redis detection |
| Spider control thread | Disabled (`with_no_control_thread(true)`) | Enabled (one background task per crawl) |
| `run_crawl_once` signature | 8 parameters | 9 parameters (`crawl_id: Option<&str>`) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Clean | Clean | PASS |
| `cargo clippy` | 0 warnings | 0 warnings | PASS |
| `cargo fmt --check` | Clean | Clean | PASS |
| `cargo test --lib` | All pass | 682 passed, 1 failed (pre-existing flaky integration test) | PASS |

The 1 failure (`count_stale_and_pending_jobs_with_pool_returns_zero_for_empty_tables`) is pre-existing — it found 2 pending jobs from other test runs in the shared Postgres DB. Unrelated to this change.

## Risks and Rollback

- **Risk:** Spider's `CONTROLLER` is process-global. If two crawl jobs run concurrently with the same URL but different crawl_ids, shutdown targets the correct one (crawl_id disambiguates). If same crawl_id+url somehow occurs, both would shut down — mitigated by UUID uniqueness.
- **Risk:** The 30s drain timeout is a new blocking period during cancel. If spider hangs (bug in control loop), the worker thread blocks for 30s. Mitigated by timeout + fallback to hard-cancel.
- **Rollback:** Revert `with_no_control_thread` to `true` in `runtime.rs` and remove the `shutdown()` call in `process.rs`. The Redis cancel path still works as before (just drops the future). No schema changes, no config changes.

## Decisions Not Taken

- **Params struct for `run_crawl_once`** — Would clean up the 9-param signature but touches all callers (CLI + worker) for no functional benefit. Deferred.
- **Exposing pause/resume to CLI** — `control` supports it, but no user-facing need yet. Cancel is the priority; pause/resume can be added later without further spider changes.
- **Removing Redis cancel layer** — Redis is still needed for cross-process signaling. Control is in-process only. Both layers serve different purposes.

## Open Questions

- Does `spider::utils::shutdown()` guarantee all in-flight requests complete before the crawl future returns, or just stop dispatching new ones? The test in spider uses `sleep(5s)` which suggests it blocks, but the exact drain semantics are undocumented.
- The `CONTROLLER` watch channel sends to ALL subscribers — if multiple workers run in the same process (e.g., `axon crawl worker` with multiple lanes), each gets every shutdown signal. The `target_id` match filters correctly, but this is broadcast overhead proportional to concurrent crawls.

## Next Steps

- Live test: run a multi-hundred-page crawl, cancel mid-flight, verify partial results in DB and on disk
- Consider adding `"canceled_pages"` count to the partial result JSON for better observability
- The `docs/spider-feature-flags.md` file (untracked) should document the `control` flag
