# Session: mcporter Test Audit + Refresh Lite-Mode Fixes

**Date**: 2026-03-28
**Branch**: `feat/lite-mode`
**Commit**: `79c3995b fix(mcp): route all refresh handlers through ServiceContext for lite mode`

---

## Session Overview

User asked whether `scripts/test-mcp-tools-mcporter.sh` was "fully up-to-date." Initial analysis (parallel agent dispatch + static comparison) concluded routes matched. User then asked if the script had actually been *run* — it hadn't. Running it exposed real bugs: the help handler was missing a key needed for route normalization, and the entire MCP refresh handler family bypassed the `ServiceContext`/`JobBackend` abstraction, hard-coding Postgres calls that break in `AXON_LITE=1` mode.

Three bugs fixed; suite moved from 3 failures → 1 transient failure (confirmed passing on re-run).

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Read `scripts/test-mcp-tools-mcporter.sh`; dispatched two parallel Explore agents to compare expected routes vs. actual MCP implementation |
| +5 min | Static analysis: all 64 routes matched — concluded "up-to-date" |
| +6 min | User challenged: "Have you run it?" — ran the script for the first time |
| +7 min | First run: 3 FAILs: `full_help_routes_match_expected`, `full_research` (transient), `lite_refresh_start` |
| +10 min | Diagnosed help failure: `"refresh_schedule"` key missing from `handlers_system.rs` action map |
| +12 min | Diagnosed lite refresh failure: `handle_refresh_start` calls `refresh_service::refresh_start()` → `make_pool()` → Postgres — bypasses `ServiceContext` |
| +20 min | Made both fixes; ran `cargo check` (no errors) — forgot to `cargo build`; test still failed (old binary) |
| +22 min | Ran `cargo build --bin axon`; test now shows PASS for both original issues + reveals 6 more refresh failures |
| +30 min | Diagnosed remaining 6: all refresh handlers (status/cancel/list/cleanup/clear/recover) have the same Postgres bypass pattern |
| +40 min | Ported all 6 handlers to `job_service::*` + `ServiceContext`; hit compile error (`ServiceJob` has no `.payload` field) |
| +42 min | Fixed: used `serde_json::to_value(j)` instead of `j.payload` |
| +45 min | Final build + test run: PASS=151, FAIL=1 (transient `lite_ask` — confirmed passing on isolated re-run) |
| +46 min | Committed; pre-commit hooks passed (monolith check, clippy, cargo check, 1569 unit tests) |

---

## Key Findings

1. **Missing `refresh_schedule` key** (`handlers_system.rs:280`): The help action map had `"refresh": [..., "schedule"]` but no `"refresh_schedule": [...]` sibling key. The test's `normalize_discovered_routes` function (line 218–232 of the script) special-cases `refresh_schedule` to generate `refresh:schedule:create/delete/disable/enable/list` route entries. Without this key, 5 expected routes were absent from the normalized output.

2. **All MCP refresh handlers bypassed `ServiceContext`**: `handlers_refresh_status.rs` called `refresh_service::refresh_*()` functions directly. These call `make_pool(cfg)` → Postgres connection. In `AXON_LITE=1` mode there is no Postgres, so all operations fail with `-32603: refresh.* failed`. This affected 7 handlers: start, status, cancel, list, cleanup, clear, recover.

3. **Pattern contrast**: crawl/embed/ingest MCP handlers all use `self.base_service_context()` → `ServiceContext` → `JobBackend` trait (routes to `FullBackend` or `LiteBackend` transparently). Refresh had been missed in the shared-runtime cutover.

4. **`ServiceJob` has no `.payload` field**: Embed handler's `embed_status()` returns `Option<EmbedJobResult>` (which has `.payload`), not `Option<ServiceJob>`. When I used `job_service::job_status()` directly, it returns `Option<ServiceJob>` — had to use `serde_json::to_value(j)` instead.

5. **`full_research` transient failure**: ACP/Tavily synthesis failed once, passed on immediate re-run. Not a code issue.

---

## Technical Decisions

**Route refresh handlers through `job_service::*` directly** rather than adding `refresh_start_with_context` etc. in `services/refresh.rs`.
*Why*: `crates/services/jobs.rs` already has all required functions (`job_status`, `cancel_job`, `list_jobs`, `cleanup_jobs`, `clear_jobs`, `recover_jobs`) that route through `ServiceContext` — adding wrapper functions would duplicate the pattern that embed's dedicated functions already do internally. Going direct was simpler.

**One `JobPayload::Refresh` per URL for `refresh_start`**.
*Why*: `JobPayload::Refresh { url: String, config_json: String }` is a single-URL payload. The full backend's existing `enqueue` impl for Refresh already does `start_refresh_job(cfg, &[url])` — i.e., one job per URL. The only semantic difference from the old Postgres path (which stored all URLs in `urls_json` in one row) is that multiple URLs now create multiple jobs. The test only passes one URL so this is not observable in practice.

**`config_json: "{}"`** for lite mode Refresh payload.
*Why*: The lite worker's `run_refresh_job_lite` (`crates/jobs/lite/workers/runners.rs:234`) only reads `SELECT url FROM axon_refresh_jobs WHERE id=?` — never reads `config_json`. Empty JSON is safe.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/mcp/server/handlers_system.rs` | Added `"refresh_schedule": ["create", "delete", "disable", "enable", "list"]` to help action map (line 281) |
| `crates/mcp/server/handlers_refresh_status.rs` | Added `JobKind`, `job_service` imports; rewrote all 7 refresh handlers to use `ServiceContext` via `base_service_context()` |

---

## Commands Executed

```bash
# First run (revealed failures)
bash scripts/test-mcp-tools-mcporter.sh

# Build after fix (needed before re-test — cargo check is NOT sufficient)
cargo build --bin axon

# Verified research was transient
mcporter --config ".cache/mcporter-test/mcporter-full.json" call axon.axon action:research query:'rust async best practices' limit:3 offset:0 --output json

# Final run
bash scripts/test-mcp-tools-mcporter.sh
# Results: PASS=151, FAIL=1 (lite_ask transient)

# Verified lite_ask transient
mcporter --config ".cache/mcporter-test/mcporter-lite.json" call axon.axon action:ask query:'What is this repository?' --output json
# → ok: true (passed)

# Commit
git add crates/mcp/server/handlers_refresh_status.rs crates/mcp/server/handlers_system.rs
git commit -m "fix(mcp): route all refresh handlers through ServiceContext for lite mode"
```

---

## Behavior Changes (Before/After)

| Operation | Before | After |
|-----------|--------|-------|
| `refresh:start` in lite mode | `-32603: refresh.start failed` | Returns `job_id` (queued in SQLite) |
| `refresh:status` in lite mode | `-32603: refresh.status failed` | Returns job object or null |
| `refresh:cancel` in lite mode | `-32603: refresh.cancel failed` | Returns `{job_id, canceled: bool}` |
| `refresh:list` in lite mode | `-32603: refresh.list failed` | Returns `{jobs: [], limit, offset}` |
| `refresh:cleanup` in lite mode | `-32603: refresh.cleanup failed` | Returns `{deleted: n}` |
| `refresh:clear` in lite mode | `-32603: refresh.clear failed` | Returns `{deleted: n}` |
| `refresh:recover` in lite mode | `-32603: refresh.recover failed` | Returns `{recovered: n}` |
| `action:help` `refresh_schedule` routes | Missing from normalized route list | `refresh:schedule:create/delete/disable/enable/list` present |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `bash scripts/test-mcp-tools-mcporter.sh` (initial) | All pass | 3 FAIL | ❌ |
| `bash scripts/test-mcp-tools-mcporter.sh` (after `cargo check` only) | All pass | 3 FAIL (old binary) | ❌ |
| `cargo build --bin axon` | Compiled | `Finished dev profile` | ✅ |
| `bash scripts/test-mcp-tools-mcporter.sh` (rebuilt binary) | All pass | PASS=151, FAIL=1 (transient lite_ask) | ✅ |
| Re-run `lite_ask` isolated | ok:true | ok:true | ✅ |
| Pre-commit hook (1569 tests) | All pass | All pass | ✅ |

---

## Source IDs + Collections Touched

None — no embed/retrieve operations were performed during this session.

---

## Risks and Rollback

**Risk (low)**: The `refresh_start` behavior change for multiple URLs (N jobs instead of 1 aggregate job) is a semantic difference from the Postgres path. If any consumer parses `urls_json` from the single aggregate refresh job row, it would break. However, inspection shows the lite worker never reads `urls_json` for refresh jobs, and the test only exercises single-URL refresh.

**Rollback**: `git revert 79c3995b` restores both files. No schema changes were made.

---

## Decisions Not Taken

- **Add `refresh_start_with_context` etc. to `services/refresh.rs`**: Would have mirrored the `embed_status`/`embed_cancel` pattern but added wrapper boilerplate for functions that `services/jobs.rs` already provides generically.
- **Fix the CLI `refresh.rs` (line 65)**: The CLI also calls `refresh_service::refresh_start` directly. Left out-of-scope since the CLI wasn't the failing surface here. Filed mentally as follow-up.

---

## Open Questions

- **CLI refresh in lite mode**: `crates/cli/commands/refresh.rs:65` also calls `refresh_service::refresh_start(cfg, &urls)` directly — same Postgres bypass. The mcporter test only exercises the MCP path. CLI `axon refresh <url>` in `AXON_LITE=1` mode is untested and likely broken.
- **Multiple-URL refresh semantics**: The Postgres path stores all URLs in one `axon_refresh_jobs` row (`urls_json`). The new path creates N rows (one per URL). If this table is ever queried expecting the aggregate row shape, it could misbehave.

---

## Next Steps

- Create a beads issue for CLI `refresh` lite-mode fix (mirrors the MCP fix done here)
- Consider adding a multi-URL refresh test case to the mcporter suite
- The `feat/lite-mode` branch is ready for PR/merge per memory notes; these fixes strengthen the lite-mode story further
