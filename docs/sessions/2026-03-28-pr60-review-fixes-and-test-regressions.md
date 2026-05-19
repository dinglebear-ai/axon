# Session: PR #60 Review Fixes, Comprehensive Code Review, and Test Regression Resolution
**Date:** 2026-03-28
**Branch:** `feat/lite-mode`
**Commits:** `fd990106`, `06ce812d`

---

## 1. Session Overview

Three sequential phases:

1. **Address open PR #60 review threads** — fetched all 70 review threads, addressed the 4 still-open ones, marked all threads resolved, pushed.
2. **Comprehensive parallel PR review** — dispatched 3 specialized agents (code-reviewer, silent-failure-hunter, pr-test-analyzer) across the large `feat/lite-mode` branch to surface remaining issues.
3. **Fix all review findings** — dispatched 4 parallel fix agents across non-overlapping file domains, integrated their changes, resolved 2 test regressions, committed and pushed.

Final state: 1576 lib tests pass, clippy clean with `-D warnings`, all pre-commit hooks green, branch pushed.

---

## 2. Timeline

| Phase | Activity |
|-------|---------|
| 09:00 | Fetched PR #60 review threads (70 total, 4 open) |
| 09:15 | Fixed 4 open threads: `Wrapper::source`, adapter cmd defaults, partial enqueue failure, help subaction schema |
| 09:30 | Marked all 70 threads resolved, pushed `fd990106` |
| 09:45 | Launched 3 parallel review agents (code, errors, tests) |
| 10:30 | Review complete: 4 critical, 7 important, 6 suggestions identified |
| 10:35 | Dispatched 4 parallel fix agents across non-overlapping domains |
| 11:30 | Integrated all agent changes; discovered 2 test regressions |
| 11:40 | Fixed regression 1: early subaction validation ordering in `handle_refresh_schedule` |
| 11:45 | Fixed regression 2: `OnceCell<PgPool>` lazy init in `FullServiceRuntime` |
| 12:00 | Fixed clippy warning (`redundant_closure` in `handlers_refresh_status.rs:193`) |
| 12:10 | All 1576 tests pass; committed and pushed `06ce812d` |

---

## 3. Key Findings

### PR #60 Open Threads (4 fixed)

- **Thread 67** (`crates/services/jobs.rs`): `Wrapper::source()` returned `self.0.source()` (the inner error's source, skipping one level) instead of `Some(self.0.as_ref())` (exposing the inner error itself as the source). Fixed to preserve full error chain.
- **Thread 68** (`crates/core/config/parse/build_config.rs`): Default ACP adapter cmd names were wrong (`"claude"` / `"codex"` instead of `"claude-agent-acp"` / `"codex-acp"`). Also missing Gemini default args `"--experimental-acp"`.
- **Thread 69** (`crates/mcp/server/handlers_refresh_status.rs`): Refresh start returned only last `job_id` instead of all `job_ids`; partial enqueue failures weren't logged with already-created IDs.
- **Thread 70** (`crates/mcp/server/handlers_system.rs`): Help response included `refresh_schedule` as a top-level action (wrong — it's a subaction under `refresh`).

### Review Agent Findings (critical/important)

- **Silent failures in `lite/ops.rs`**: `mark_completed`/`mark_failed` didn't check `rows_affected()` — no-ops on terminal-state jobs were silently swallowed.
- **UUID corruption in `lite/query.rs`**: `Uuid::from_str().unwrap_or_default()` on parse failure returned `Uuid::nil()` silently; now warns via `tracing::warn!`.
- **No supervisor for spawned workers** (`lite/workers.rs`): Each `tokio::spawn` handle was dropped; panics/unexpected exits were invisible.
- **Missing output isolation** (`lite/workers/runners.rs`): Crawl and refresh runners used the bare output dir, not the per-job isolated `predict_crawl_output_dir` path.
- **Unbounded wait** (`services/crawl.rs`): `wait_for_pending_embed_jobs` had no deadline; now 300s with `tracing::warn!` on timeout.
- **Graph capability check** (`mcp/server/handlers_graph.rs`): Used `self.cfg.lite_mode` directly instead of `ctx.capabilities.graph.supported` — bypassed the capability abstraction.
- **Eager Postgres pool** (`services/runtime.rs`): Agent added `pool: sqlx::PgPool` with `make_pool()` called eagerly in `resolve_runtime()`, breaking all tests using `Config::default()` (empty `pg_url`).

---

## 4. Technical Decisions

### `OnceCell<PgPool>` for lazy Postgres init
`FullServiceRuntime` needs a `PgPool` for `has_active_jobs()` but tests use `Config::default()` which has an empty `pg_url`. Two alternatives rejected:
- `connect_lazy()`: Still fails at struct construction because `"".parse()` gives "relative URL without a base" before any connection is attempted.
- Eager `make_pool()` in `resolve_runtime()`: Breaks tests immediately.
Chosen: `tokio::sync::OnceCell<sqlx::PgPool>` — pool created only on first `has_active_jobs()` call via `get_or_try_init`.

### Early subaction validation in `handle_refresh_schedule`
The test `refresh_schedule_unknown_subaction_returns_invalid_params` got `INTERNAL_ERROR` because `base_service_context()` was called first and constructed a full `ServiceContext` (connecting to SQLite). By moving the subaction `match` before the context call, unknown subaction always returns `INVALID_PARAMS(-32602)` regardless of DB availability.

### `config_json: "{}"` in lite mode (deferred)
Every lite-mode job enqueue passes `config_json: "{}"` — losing per-request config overrides (e.g. `max_pages`, `render_mode`). A proper fix requires serializing `Config` overrides into the job payload at enqueue time. Deferred as architectural change too large for current sprint.

---

## 5. Files Modified

| File | Change |
|------|--------|
| `crates/services/jobs.rs` | Fix `Wrapper::source()` to return `Some(self.0.as_ref())` |
| `crates/core/config/parse/build_config.rs` | Fix adapter defaults; add 4 serial tests |
| `crates/mcp/server/handlers_refresh_status.rs` | Collect all job_ids; early subaction validation; clippy fix (redundant_closure) |
| `crates/mcp/server/handlers_system.rs` | Remove `refresh_schedule` from help top-level actions |
| `crates/mcp/server/handlers_graph.rs` | Use `ctx.capabilities.graph.supported` not `self.cfg.lite_mode` |
| `crates/mcp/server/services_migration_tests.rs` | Add `refresh_start_response_includes_all_job_ids` test |
| `crates/jobs/lite/ops.rs` | `rows_affected()==0` warns; add 2 new tests |
| `crates/jobs/lite/query.rs` | UUID/JSON deser errors warn+nil; applied to `list_jobs`, `job_status_row`, `service_job_from_tuple` |
| `crates/jobs/lite/workers.rs` | Supervisor task wraps each `tokio::spawn` handle |
| `crates/jobs/lite/workers/runners.rs` | Per-job output dir via `predict_crawl_output_dir`; `tracing::warn!` on all `Ok(None)` returns |
| `crates/services/crawl.rs` | 300s deadline for `wait_for_pending_embed_jobs` with timeout warn |
| `crates/services/runtime.rs` | `FullServiceRuntime.pool: OnceCell<PgPool>`; added module-level doc comment |
| `crates/services/runtime/full.rs` | `has_active_jobs`: lazy pool via `get_or_try_init`; `kind.table_name()` replaces inline match |
| `crates/cli/commands/serve_supervisor/runtime.rs` | `spawn_log_task` logs stderr read errors |
| CLAUDE.md files (6) | Updated to reflect current architecture |

---

## 6. Commands Executed

```bash
# Fetch PR review threads
python3 $HOME/.claude/skills/gh-address-comments/scripts/fetch_comments.py

# Mark all threads resolved
python3 $HOME/.claude/skills/gh-address-comments/scripts/mark_resolved.py <thread_ids...>

# Verify all resolved
python3 $HOME/.claude/skills/gh-address-comments/scripts/fetch_comments.py | \
  python3 $HOME/.claude/skills/gh-address-comments/scripts/verify_resolution.py

# Test runs
cargo test --lib           # after each fix; 1576 passing at final state
cargo clippy -- -D warnings  # clean before commit

# Final commit
git add -u && git commit -m "fix(lite): comprehensive PR review fixes and test regressions"
git push
```

---

## 7. Behavior Changes (Before/After)

| Item | Before | After |
|------|--------|-------|
| `Wrapper::source()` | Skipped one error level in chain | Exposes inner error as source |
| ACP adapter defaults | `"claude"` / `"codex"` (wrong binaries) | `"claude-agent-acp"` / `"codex-acp"` |
| refresh.start | Returns single `job_id` | Returns both `job_ids[]` and `job_id` (last) |
| refresh.schedule unknown subaction | INTERNAL_ERROR (-32603) | INVALID_PARAMS (-32602) |
| graph handler capability check | Reads `cfg.lite_mode` directly | Uses `ctx.capabilities.graph.supported` |
| `FullServiceRuntime` Postgres pool | Eagerly connected at construction | Lazily created on first `has_active_jobs()` call |
| Lite mode `mark_completed` no-op | Silent | `tracing::warn!` with job ID |
| Lite mode UUID parse failure | Silent `Uuid::nil()` | `tracing::warn!` with raw value |
| Worker panic | Invisible to supervisor | Logged via supervisor task |
| Crawl/refresh runner output | Flat output dir | Per-job isolated `<base>/domains/<domain>/<job_id>/` |
| `wait_for_pending_embed_jobs` | No timeout | 300s deadline with warn on timeout |

---

## 8. Verification Evidence

| Command | Expected | Actual | Status |
|---------|---------|--------|--------|
| `cargo test --lib` | 0 failures | 1576 passed, 0 failed | ✅ |
| `cargo clippy -- -D warnings` | No errors/warnings | Clean | ✅ |
| Pre-commit hooks (lefthook) | All green | All 12 hooks green | ✅ |
| `git push` | Pushed to remote | `fd990106..06ce812d feat/lite-mode` | ✅ |

---

## 9. Source IDs + Collections Touched

None (this session made no embed/retrieve calls; work was all code changes).

---

## 10. Risks and Rollback

- **`OnceCell` pool**: Low risk. If `has_active_jobs()` is never called (most tests), no Postgres connection is made. On first call, `make_pool()` errors propagate as `BackendResult::Err` like before.
- **Rollback**: `git revert 06ce812d` or `git reset --hard fd990106` restores prior state. PR #60 review thread resolutions are on GitHub and unaffected by git rollback.

---

## 11. Decisions Not Taken

| Alternative | Reason Rejected |
|-------------|----------------|
| `PgPoolOptions::connect_lazy()` for deferred pool | `"".parse::<PgConnectOptions>()` still fails at struct construction with "relative URL without a base" |
| Fix `config_json: "{}"` in lite mode | Architectural: requires serializing `Config` overrides into `JobPayload`; too large for sprint |
| Single sequential fix agent | Parallel agents saved ~1 hour on 4 independent domains |

---

## 12. Open Questions

- `config_json: "{}"` lite mode issue: per-request config overrides are silently dropped when jobs are enqueued in lite mode. Needs a design decision on how to embed config deltas into `JobPayload`.
- The 4 GitHub Dependabot vulnerabilities (1 high, 3 moderate) on the `main` branch are pre-existing and unrelated to this PR.

---

## 13. Next Steps

1. **Merge PR #60** — all review threads resolved, tests passing, hooks green.
2. **Fix `config_json` in lite mode** — serialize relevant `Config` fields into job payload at enqueue time.
3. **Address Dependabot alerts** — separate PR targeting `main`.
