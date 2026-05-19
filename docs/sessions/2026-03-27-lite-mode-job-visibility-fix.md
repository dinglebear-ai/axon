# Session: Lite Mode Job Visibility Fix

**Date:** 2026-03-27
**Branch:** `feat/lite-mode`
**Outcome:** Fixed — all 4 MCP `start` subactions and 2 service functions corrected

---

## Session Overview

Diagnosed and fixed a bug where crawl jobs started via the MCP tool were invisible to subsequent `status` and `list` queries when the server ran in lite mode (`AXON_LITE=1`). Root cause: `crawl.start` (and `embed.start`, `extract.start`, `ingest.start`) were calling Postgres-direct service functions, while `status`/`list` read from SQLite via `LiteServiceRuntime`. Extended the fix to enforce the correct `cfg.wait` contract: all `*_start_with_context` functions in lite mode now only block when `cfg.wait == true`.

---

## Timeline

1. User ran `/axon crawl gofastmcp.com` — job started, returned job ID `aa764ef6`
2. `/axon crawl status aa764ef6` returned `null`; `crawl list` returned empty
3. Ran `/systematic-debugging` — traced data flow from `crawl_start` → Postgres vs `crawl_list` → SQLite
4. `doctor` call confirmed `lite_mode: true` on running server
5. User identified the fix should go through `ServiceContext` and only block on `--wait true`
6. Fixed `crawl_start_with_context` + `embed_start_with_context` service functions (added `cfg.wait` guard)
7. Fixed all 4 MCP `start` handlers to call `*_with_context` variants
8. `cargo check` clean; migration-conflict test failures confirmed pre-existing

---

## Key Findings

- **Root cause** (`crates/services/crawl.rs:106`): `crawl_start` calls `crawl::start_crawl_jobs_batch` which always writes to Postgres, ignoring `cfg.lite_mode`
- **MCP `crawl_status`/`crawl_list`** (`crates/mcp/server/handlers_crawl_extract.rs:150,64`): both called `service_context_for` → `LiteServiceRuntime` → SQLite — correct backend, wrong write path
- **`lite_mode: true`** confirmed via `doctor` action on running MCP server
- **`crawl_start_with_context`** (`crates/services/crawl.rs:144`): always blocked in lite mode via `wait_for_job` — violates `--wait false` default
- **`embed_start_with_context`** (`crates/services/embed.rs:121`): same always-block bug in lite mode
- **`extract_start_with_context`** and **`ingest_start_with_context`**: already async-safe (never blocked) — no changes needed
- Pre-existing test failure: "migration 3 was previously applied but has been modified" — unrelated to this session, confirmed by `git stash` test

---

## Technical Decisions

- **Service layer owns the wait/block decision** — not handlers. The `cfg.wait` check belongs in `*_start_with_context`, not in MCP or CLI handlers.
- **All 4 MCP start handlers** use `*_with_context` variants so they route through the same `ServiceJobRuntime` as `status`/`list`/`cancel`. Consistent backend for all subactions.
- **Non-blocking lite mode returns `StartDisposition::Enqueued, ExecutionMode::InProcess`** — same as `extract` and `ingest` already did.
- **Did not add a new function** (`crawl_enqueue` considered, rejected) — the `cfg.wait` guard inside `crawl_start_with_context` is sufficient and keeps the call path unified.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/services/crawl.rs` | Added `cfg.wait` guard in lite branch of `crawl_start_with_context`; added new non-blocking test; updated 2 existing tests to set `cfg.wait = true` |
| `crates/services/embed.rs` | Added `cfg.wait` guard in lite branch of `embed_start_with_context` |
| `crates/mcp/server/handlers_crawl_extract.rs` | `handle_crawl_start`: replaced `crawl_svc::crawl_start` with `crawl_svc::crawl_start_with_context` + `service_context_for`; `handle_extract_start`: replaced `extract_svc::extract_start` with `extract_svc::extract_start_with_context` + `base_service_context` |
| `crates/mcp/server/handlers_embed_ingest.rs` | `handle_embed_start`: replaced `embed_start_with_input` with `embed_start_with_context` + `base_service_context`; `handle_ingest_start`: replaced `ingest_start` with `ingest_start_with_context` + `base_service_context`; updated imports |

---

## Behavior Changes (Before/After)

| Operation | Before | After |
|-----------|--------|-------|
| `crawl.start` in lite mode | Wrote to Postgres | Writes to SQLite via `service_context.jobs.enqueue` |
| `embed.start` in lite mode | Wrote to Postgres | Writes to SQLite via `service_context.jobs.enqueue` |
| `extract.start` in lite mode | Wrote to Postgres | Writes to SQLite via `service_context.jobs.enqueue` |
| `ingest.start` in lite mode | Wrote to Postgres | Writes to SQLite via `service_context.jobs.enqueue` |
| `crawl_start_with_context` lite blocking | Always blocked | Only blocks when `cfg.wait == true` |
| `embed_start_with_context` lite blocking | Always blocked | Only blocks when `cfg.wait == true` |
| `crawl.status` / `crawl.list` after start | Returned `null` / empty | Will find jobs (same SQLite pool) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | 0 errors | 0 errors, 15.08s | PASS |
| `crawl_start_with_context_completes_in_lite_mode` (test) | PASS | FAIL — pre-existing migration conflict | PRE-EXISTING |
| `crawl_start_with_context_rejects_empty_urls_in_lite_mode` (test) | PASS | FAIL — pre-existing migration conflict | PRE-EXISTING |
| `crawl_start_with_context_surfaces_canceled_jobs_in_lite_mode` (test) | PASS | FAIL — pre-existing migration conflict | PRE-EXISTING |
| `git stash && test rejects_empty && git stash pop` | Same failure without our changes | Confirmed pre-existing | CONFIRMED |
| `doctor` action on live MCP server | `lite_mode: true` | `lite_mode: true` | CONFIRMED |

---

## Risks and Rollback

- **Risk**: CLI commands that relied on `crawl_start_with_context` always blocking in lite mode (e.g., inline completion workflows) will now return immediately unless `--wait true` is passed. This is the correct behavior per the `--wait` contract.
- **Rollback**: Revert the 4 files. No schema changes; no data migrations.
- **No Postgres writes in lite mode anymore** for crawl/embed/extract/ingest start — if any external code depended on Postgres jobs being present in lite mode, it would break. There is no such code in this repo.

---

## Decisions Not Taken

- **`crawl_enqueue` new function** — considered to split enqueue-only from wait logic; rejected because adding `cfg.wait` guard inside `crawl_start_with_context` achieves the same result with less surface area
- **Fix only `crawl.start`** — rejected; systematic audit found embed/extract/ingest start had the same backend-bypass bug
- **Make `crawl_start` check `lite_mode` itself** — rejected; it has no `ServiceContext`, so we'd need to add one. Upgrading MCP handlers to use `*_with_context` is cleaner

---

## Open Questions

- The pre-existing "migration 3 was previously applied but has been modified" test failure — needs investigation. Related to uncommitted changes in `crates/jobs/lite/store.rs` and `crates/jobs/lite/workers.rs`.
- `embed_start_with_context` is called from `enqueue_embed_job` in the CLI embed command (`crates/cli/commands/embed.rs:229`) only when `!cfg.wait`. After this fix, when `!cfg.wait` + lite mode, `embed_start_with_context` enqueues to SQLite and returns immediately — correct. When `cfg.wait` + lite mode, the CLI embed command takes a different path (`embed_now`) and never calls `embed_start_with_context` — no regression.

---

## Next Steps

- Investigate and resolve migration 3 conflict in `crates/jobs/lite/store.rs`
- Deploy updated binary — running MCP server still uses old code without `--wait`-aware service layer
- Consider adding an integration smoke test: `crawl.start` → `crawl.status` in lite mode, verifying job is visible
