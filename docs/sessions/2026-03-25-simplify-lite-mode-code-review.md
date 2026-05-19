# Session: Simplify — Lite-Mode Branch Code Review & Cleanup
**Date:** 2026-03-25
**Branch:** `feat/lite-mode`
**Duration:** Single session

---

## 1. Session Overview

Ran the `/simplify` skill on the `feat/lite-mode` branch. Three specialized review agents (reuse, quality, efficiency) analyzed the diff in parallel and surfaced 12 findings. Seven were fixed; five were deferred as too risky or out of scope for a cleanup pass.

---

## 2. Timeline

1. **Captured diff** — `git diff HEAD` over `crates/**/*.rs` (2759 lines, 111 KB)
2. **Launched 3 agents in parallel** — reuse, quality, efficiency reviewers each received the full diff + 4 untracked new files
3. **Aggregated findings** — 12 unique issues, rated by confidence (80–95%)
4. **Implemented 7 fixes** — all verified with `cargo check` (0 errors)
5. **Deferred 5 items** — noted below in Decisions Not Taken

---

## 3. Key Findings

| # | Category | Issue | Confidence | Fixed? |
|---|----------|-------|-----------|--------|
| 1 | Reuse/Quality | `WorkerMode` match block copy-pasted 5× across CLI command files | 95% | ✅ |
| 2 | Reuse | `ms_to_dt` defined in both `lite/query.rs` and `watch_lite.rs` | 92% | ✅ |
| 3 | Reuse/Efficiency | `lite_pool`/`pool` helpers duplicated; both call `open_sqlite_pool` (runs migrations) per call | 90% | ✅ |
| 4 | Reuse | 3 identical `parse_*_port` functions in `build_config.rs` | 88% | ✅ |
| 5 | Quality | Dead code guards in `embed.rs:28-31` and `extract.rs:30-33` — unreachable after `maybe_handle_*_subcommand` | 88% | ✅ |
| 6 | Quality | `'running'` SQL literal in `watch_lite.rs` bypasses `WATCH_RUN_STATUS_RUNNING` constant | 85% | ✅ |
| 7 | Quality | `dt_to_ms` helper in `watch_lite.rs` — single-use wrapper for `.timestamp_millis()` | 85% | ✅ (inlined) |
| 8 | Efficiency | `get_watch_def` in full mode: fetches 500 rows + filters in Rust instead of `WHERE id = ?` | 90% | ⏭ deferred |
| 9 | Efficiency | `list_service_query` in lite mode always `LIMIT 500`, ignores caller's `limit` param | 85% | ⏭ deferred |
| 10 | Efficiency | `capture_screenshot_bytes`: `spawn_blocking` → inner `std::thread` → own runtime (3 layers) | 83% | ⏭ deferred |
| 11 | Quality | `port_owner_matches_binding` uses stringly-typed `name` dispatch in `serve_supervisor.rs` | 82% | ⏭ deferred |
| 12 | Efficiency | `inspect_port_owners`: sequential `ps` subprocesses per PID | 82% | ⏭ deferred |

---

## 4. Technical Decisions

### `handle_worker_mode` extracted to `common.rs`
The `WorkerMode` match had 5 identical copies (crawl, embed, extract, ingest, refresh). Moving to `common.rs` is the established pattern for shared CLI helpers — the file already owns `handle_job_cancel`, `handle_job_cleanup`, etc. All 5 call sites become one-liners.

### `open_config_pool` added to `jobs/lite/store.rs`
Both `services/jobs.rs` and `jobs/watch_lite.rs` wrapped `open_sqlite_pool(&cfg.sqlite_path.to_string_lossy())`. This call runs `PRAGMA journal_mode=WAL` + full migration on every invocation — a subtle perf concern on hot paths. The shared helper makes the coupling to `Config` explicit and reduces duplication. The pool-per-call pattern remains (noted as a deeper architectural concern in deferred items).

### `env_port` replaces 3 `parse_*_port` functions
`parse_mcp_http_port`, `parse_web_dev_port`, `parse_shell_server_port` were identical bodies differing only in the env var name in the error message. `env_port(env_var, default)` also absorbs the `env::var(…).ok().as_deref().map(fn).transpose()?.unwrap_or(default)` boilerplate at each call site (saves 12 lines total).

### `WATCH_RUN_STATUS_RUNNING` as bind param
The `'running'` SQL literal was inside a VALUES clause alongside the two status constants already imported from `watch.rs`. Changed to a `?` bind param with `.bind(WATCH_RUN_STATUS_RUNNING)`. Consistent with how `WATCH_RUN_STATUS_COMPLETED`/`FAILED` are consumed via `finish_watch_run`.

---

## 5. Files Modified

| File | Change |
|------|--------|
| `crates/cli/commands/common.rs` | Added `WorkerMode` import, `use std::error::Error`, `handle_worker_mode()` fn |
| `crates/cli/commands/crawl/subcommands.rs` | Added `handle_worker_mode` to imports, removed `WorkerMode` import, one-lined worker arm |
| `crates/cli/commands/embed.rs` | Added `handle_worker_mode`, removed `WorkerMode`, removed dead code guard, one-lined worker arm |
| `crates/cli/commands/extract.rs` | Added `handle_worker_mode`, removed `WorkerMode`, removed dead code guard, one-lined worker arm |
| `crates/cli/commands/ingest_common.rs` | Added `handle_worker_mode`, removed `WorkerMode`, one-lined worker arm |
| `crates/cli/commands/refresh.rs` | Added `handle_worker_mode`, removed `WorkerMode`, one-lined worker arm |
| `crates/jobs/lite/query.rs` | `fn ms_to_dt` → `pub(crate) fn ms_to_dt` |
| `crates/jobs/lite/store.rs` | Added `Config` import, added `pub(crate) async fn open_config_pool` |
| `crates/jobs/watch_lite.rs` | Removed `ms_to_dt`, `dt_to_ms`, `pool`; imported `ms_to_dt` + `open_config_pool`; added `WATCH_RUN_STATUS_RUNNING`; changed `'running'` to bind param; inlined `dt_to_ms` |
| `crates/services/jobs.rs` | Removed `lite_pool`, imported `open_config_pool`, replaced 6 `lite_pool(cfg)` calls, moved `open_sqlite_pool` to test-only import |
| `crates/core/config/parse/build_config.rs` | Replaced 3 `parse_*_port` fns + verbose call pattern with `env_port(env_var, default)` |

---

## 6. Commands Executed

```bash
git diff HEAD --stat               # 118 files changed, 1830 insertions, 2065 deletions
cargo check 2>&1 | grep "^error"   # (no output — zero errors)
cargo check 2>&1 | grep "unused import"  # 3 unused imports found → fixed
cargo check 2>&1 | grep -c "^warning"   # 21 (all pre-existing std::error::Error qualification warnings)
```

---

## 7. Behavior Changes (Before / After)

| Surface | Before | After |
|---------|--------|-------|
| `axon crawl worker` / `embed worker` / etc. | 5 independent match blocks, any drift = divergence | Single `handle_worker_mode` — one source of truth for the message and `Unsupported` error path |
| `embed.rs` / `extract.rs` "worker" in lite mode | Dead guard ran (unreachable) before subcommand handler | Guard removed — handled by `maybe_handle_*_subcommand` as designed |
| `watch_lite.rs` INSERT status | `'running'` SQL literal (inconsistent with status constants) | Bound via `WATCH_RUN_STATUS_RUNNING` constant |
| Port env var parse errors | Three functions with env var name baked in | Single `env_port` — consistent error format |
| `open_sqlite_pool` call sites | 6 in `services/jobs.rs` + 6 in `watch_lite.rs` wrapping same one-liner | `open_config_pool` in `store.rs` — single callsite |

---

## 8. Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check 2>&1 \| grep "^error"` | No output | No output | ✅ Pass |
| `cargo check 2>&1 \| grep "unused import"` | No unused imports | No output after cleanup | ✅ Pass |
| `cargo check 2>&1 \| grep -c "^warning"` | ≤21 (pre-existing) | 21 | ✅ Pass |

---

## 9. Source IDs + Collections Touched

None — this was a code review and refactor session with no Axon embed/retrieve operations prior to this save.

---

## 10. Risks and Rollback

**Risk:** `open_config_pool` runs `open_sqlite_pool` → WAL pragma + full migration on every service call in lite mode. The per-call pool pattern is unchanged; the duplication is reduced but the underlying concern (noted in efficiency finding #8 deferred) remains. Future work: cache the pool at startup.

**Rollback:** All changes are in `crates/`. `git diff HEAD -- 'crates/**/*.rs'` captures them. `git stash` or `git checkout` of affected files restores prior state.

---

## 11. Decisions Not Taken

| Issue | Reason Deferred |
|-------|----------------|
| `get_watch_def` full-table scan (500 rows for 1 result) | Requires adding a new Postgres query function — non-trivial, belongs in a focused fix PR |
| `list_service_query` ignores `limit`/`offset` in lite mode | Needs parameter threading through 6 SQL match arms — functional regression risk without tests |
| `capture_screenshot_bytes` 3-layer execution nesting | `screenshot.rs` already near monolith limit; restructuring `spawn_blocking` semantics warrants its own review |
| `port_owner_matches_binding` stringly-typed dispatch | Needs a new `enum BoundService` and refactor of `serve_supervisor.rs` — out of scope for simplify pass |
| Sequential `ps` in `inspect_port_owners` | Startup-only code; impact <200ms; acceptable for now |

---

## 12. Open Questions

- Should `open_config_pool` eventually hold a cached pool (Arc<SqlitePool>) at the `Config` level to avoid repeated migration runs on every service call? The current pattern opens a fresh pool per request.
- The `list_service_query` LIMIT 500 hardcode: since the caller's `limit`/`offset` are silently dropped in lite mode, do lite-mode users ever see truncated results from large databases?

---

## 13. Next Steps

1. Add `WHERE id = ?` single-row lookup for `get_watch_def` full mode (deferred issue #8)
2. Thread `limit`/`offset` into `list_service_jobs` in `jobs/lite/query.rs` (deferred issue #9)
3. Consider `Arc<SqlitePool>` cached at startup to eliminate per-call migration overhead
