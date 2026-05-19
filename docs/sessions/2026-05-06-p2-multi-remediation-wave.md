---
date: 2026-05-06 01:33:26 EST
repo: git@github.com:jmagar/axon.git
branch: bd-work/p2-multi-remediation
head: 270b3fb2
plan: none
agent: Claude (claude-sonnet-4-6)
session id: ff9217fb-e8bc-4784-9b31-117ec1cbbf44
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon-rust/ff9217fb-e8bc-4784-9b31-117ec1cbbf44.jsonl
working directory: /home/jmagar/workspace/axon_rust
---

## User Request

Run the multi-bead work flow (`/lavra:lavra-work-multi`) on all ready beads, then perform a full pre-push review and push.

## Session Overview

Executed a 5-bead parallel implementation wave using the lavra-work-multi skill. Implemented taplo TOML formatting, queue caps for three job types, a global ACP session LRU cap, typed MapResult migration, and env/config doc centralization. A Phase M8 review found and fixed 2 P1 bugs before committing. A second `/lavra:lavra-review` was run as a pre-push gate. Branch pushed, 5 beads closed, 12 follow-on beads filed.

## Sequence of Events

1. Checked `bd ready` — found 5 actionable tasks plus several epics
2. Created branch `bd-work/p2-multi-remediation` from `main`
3. Ran conflict detection — no file overlaps; all 5 beads assigned to Wave 1
4. Sub-wave A (3 agents in parallel): taplo config, queue caps, ACP session cap
5. Sub-wave B (2 agents in parallel): typed MapResult migration, env/config docs
6. Phase M8 review (5 agents in parallel) found 2 P1 bugs, 5 P2s, 5 P3s
7. Fixed P1-1: MCP handle_map double-pagination — added `total: u64` to `MapResult`, removed `paginate_vec` from handler
8. Fixed P1-2: unsafe `std::env::set_var` in queue cap tests — refactored to direct parameter injection via `check_pending_cap_for`
9. Ran `cargo test` — 1674 passed, 0 failed
10. Committed 5 per-bead commits + version bump (v1.3.4 → v1.4.0)
11. Closed 5 beads; filed 12 follow-on beads (2 P1-fixes already resolved, 10 P2/P3 follow-ups)
12. Ran second `/lavra:lavra-review` as pre-push gate — found 1 new P2 (`mapped_urls` naming), 3 new P3s; no blockers
13. Pushed to `origin/bd-work/p2-multi-remediation`

## Key Findings

- **P1 — MCP double-pagination**: `services::map::discover()` applied offset/limit pagination before returning `MapResult.urls`, but `handle_map()` in `crates/mcp/server/handlers_query.rs:115` called `paginate_vec(&urls, offset, limit)` a second time. `total_urls` reported the paginated count, not the pre-pagination total. Fix: added `total: u64` to `MapResult` set before the `into_iter()` consumption; handler now uses `result.total`.
- **P1 — unsafe env tests**: `crates/jobs/lite/ops/tests.rs` used `ENV_LOCK: Mutex<()>` + `unsafe { std::env::set_var(...) }`. A file-local mutex does not guard against concurrent `getenv` from tokio/tracing in the same test binary. Fix: refactored `check_pending_cap_for` to accept `limit: u64` explicitly; tests seed rows via raw SQL and call the function directly.
- **P2 — `mapped_urls` naming inversion** (filed, not blocking): after the typed migration, `mapped_urls` equals `urls.len()` (post-pagination) while `total` holds the pre-pagination count. The name implies total discovered. CLI always passes limit=0 so `total == mapped_urls` there; MCP uses `result.total` explicitly — no user-visible misinformation currently. Filed as `axon_rust-pkl.34.3`.
- **Thin job wrapper anti-pattern avoided**: queue cap wrappers (`check_crawl_pending_cap`, etc.) correctly resolve env vars and call the testable `check_pending_cap_for` core — they do not call `open_sqlite_pool()` and do not bypass `LiteBackend::enqueue()`.

## Technical Decisions

- **Dependency injection over env mutation in tests**: refactored `check_pending_cap_for` to take `limit: u64` explicitly so tests never need `unsafe { set_var }`. Wrappers own the env read; the testable primitive is pure.
- **`total: u64` field added to `MapResult`**: chosen over Option B (returning full URL list to callers) because it preserves the existing service-side pagination contract and avoids doubling memory for large map results.
- **Queue caps read env per-call, session cap uses `LazyLock`**: intentional — queue caps are conceptually runtime-configurable per-operation; session cap is a process-lifetime bound. Inconsistency documented via filed P2 bead `axon_rust-pkl.10.4`.
- **`paginate_vec` removed from `crates/mcp/server/common.rs`**: had no remaining callers after the pagination fix; removed rather than left as dead code.
- **Single-pass LRU eviction (not loop)**: `evict_if_over_cap` removes one victim per call. Concurrent inserts can transiently overshoot the cap — acceptable given the `AXON_ACP_MAX_CONCURRENT_SESSIONS` semaphore provides a separate hard bound on in-flight sessions.

## Files Modified

| File | Purpose |
|------|---------|
| `.taplo.toml` | Created: taplo formatter config (2-space indent, 100-col, no key reorder) |
| `Justfile` | Added `taplo-check` and `taplo-fmt` recipes |
| `.cargo/audit.toml`, `deny.toml`, `Cargo.toml` | Reformatted by taplo to comply |
| `crates/jobs/lite/ops/enqueue.rs` | Generalized `check_pending_cap` → `check_pending_cap_for(pool, table, queue_name, limit)` + 4 env-reading wrappers |
| `crates/jobs/lite/ops.rs` | Removed inline test module (moved to tests.rs), added `mod tests` declaration |
| `crates/jobs/lite/ops/tests.rs` | New: extracted tests + 5 new queue-cap tests using direct param injection (no unsafe) |
| `crates/services/acp/session_cache.rs` | Added `MAX_SESSIONS: LazyLock<usize>` + 5 new eviction tests |
| `crates/services/acp/session_cache/cache.rs` | Added `evict_if_over_cap()`, wired into `insert()` |
| `crates/services/types/service.rs` | Replaced `MapResult { payload: Value }` with typed struct (10 fields, `total: u64` included) |
| `crates/services/map.rs` | `discover()` constructs typed `MapResult`; `parse_map_result()` helper + 7 tests |
| `crates/cli/commands/map.rs` | Updated to use typed `MapResult` fields directly |
| `crates/mcp/server/handlers_query.rs` | Use `result.total` for `total_urls`; removed `paginate_vec` call |
| `crates/mcp/server/common.rs` | Removed now-dead `paginate_vec` function |
| `.env.example` | Added `AXON_MAX_PENDING_EMBED/EXTRACT/INGEST_JOBS` and `AXON_ACP_MAX_SESSIONS` |
| `docs/CONFIG.md` | Designated as single authoritative env var reference; added ~25 missing vars, removed ~8 stale ones |
| `README.md` | Replaced 200-line env table with essentials table + link to CONFIG.md |
| `docs/mcp/ENV.md` | Added 3 missing MCP-specific vars |
| `tests/cli_full_rewire_smoke.rs`, `tests/services_discovery_services.rs` | Updated for typed `MapResult` (added `total` field to fixtures) |
| `.monolith-allowlist` | Added `crates/services/types/service.rs` (547 lines, expires 2026-06-09) |
| `Cargo.toml`, `CHANGELOG.md`, `.claude-plugin/plugin.json` | Version bump 1.3.4 → 1.4.0 |

## Commands Executed

```bash
# Branch creation
git checkout -b bd-work/p2-multi-remediation

# Tests after all changes
cargo test  # → 1674 passed, 0 failed, 9 ignored

# Taplo verification
taplo fmt --check  # → exit 0
cargo metadata --locked --format-version 1 > /tmp/axon-metadata.json  # → exit 0

# Push
git push -u origin bd-work/p2-multi-remediation
# → new branch pushed; PR URL: https://github.com/jmagar/axon/pull/new/bd-work/p2-multi-remediation
```

## Errors Encountered

- **unsafe `set_var` in queue cap tests** (P1 bug found during review): original implementation used `unsafe { std::env::set_var("AXON_MAX_PENDING_EMBED_JOBS", "2") }` with a file-local mutex — unsound under parallel test execution. Root cause: cap check function read env internally instead of accepting a parameter. Resolution: refactored to `check_pending_cap_for(pool, table, queue_name, limit)` accepting explicit limit; tests use direct SQL seed + parameter injection.
- **MCP double-pagination** (P1 bug found during review): service already paginated `result.urls` before the MCP handler called `paginate_vec()` again. Root cause: `MapResult` previously carried the full URL list; after pagination moved into the service, the handler was not updated. Resolution: added `total: u64` to `MapResult` capturing pre-pagination count; handler uses `result.total` and drops `paginate_vec` call.

## Behavior Changes (Before/After)

| Feature | Before | After |
|---------|--------|-------|
| Embed job queue | No cap — unlimited pending jobs accepted | Capped at 50 (`AXON_MAX_PENDING_EMBED_JOBS`, 0=unlimited) |
| Extract job queue | No cap | Capped at 50 (`AXON_MAX_PENDING_EXTRACT_JOBS`) |
| Ingest job queue | No cap | Capped at 50 (`AXON_MAX_PENDING_INGEST_JOBS`) |
| ACP session cache | Unbounded session accumulation | Capped at 100 sessions (`AXON_ACP_MAX_SESSIONS`), LRU eviction |
| MCP `map` with pagination | `total_urls` returned paginated count; double-pagination produced wrong results | `total_urls` returns pre-pagination count; single pagination applied by service |
| `MapResult` type | `{ payload: serde_json::Value }` — opaque pass-through | Typed struct with 10 named fields + `Serialize/Deserialize` |
| `docs/CONFIG.md` | Partial, drifted, ~25 missing vars | Authoritative: all vars documented, stale entries removed, sync script added |
| README env table | 200-line duplicate of CONFIG.md (with stale entries) | 15-var essentials table + link to CONFIG.md |
| TOML formatting | Inconsistent (4-space, 2-space, multi-line arrays) | Uniform via taplo: 2-space, 100-col, single-line short arrays |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test` | 1674 passed, 0 failed | 1674 passed, 0 failed | ✓ |
| `taplo fmt --check` | exit 0 | exit 0 | ✓ |
| `cargo metadata --locked` | exit 0 | exit 0 | ✓ |
| All pre-commit hooks | Pass | Pass (all 6 hooks) | ✓ |
| `git push` | New branch on remote | Branch pushed, PR URL generated | ✓ |

## Risks and Rollback

- **Queue caps reject previously-accepted jobs**: embed/extract/ingest caps default to 50. High-volume deployments that relied on unlimited queuing will see rejection errors. Mitigate by setting `AXON_MAX_PENDING_*_JOBS=0` in `.env`. Rollback: revert `crates/jobs/lite/ops/enqueue.rs` changes.
- **`MapResult` wire format addition**: `total` field added to JSON output. Consumers that use strict deserialization on persisted map results from before this branch will fail. Map results are not persisted to disk (no DB row for `MapResult`); MCP artifact files write a hand-constructed JSON object, not a serialized `MapResult`. Risk is low.
- **ACP session eviction**: sessions can now be silently evicted. If `AXON_ACP_MAX_SESSIONS` is too low for the workload, active sessions may be evicted mid-stream. The existing `AXON_ACP_MAX_CONCURRENT_SESSIONS` semaphore limits in-flight sessions independently. Set `AXON_ACP_MAX_SESSIONS=0` to disable eviction.

## Decisions Not Taken

- **Inline 4 queue-cap wrappers into `enqueue_job` match arms**: the code-simplicity reviewer flagged this as a P2. Not done in this session to avoid scope creep; filed as `axon_rust-pkl.10.2`.
- **LazyLock for queue cap limits**: performance reviewer suggested caching env reads via `LazyLock` to match `MAX_SESSIONS` pattern. Not done; filed as `axon_rust-pkl.10.4`.
- **Remove `parse_map_result` missing-field tests**: simplicity reviewer noted 5 of 7 tests exercise serde's `#[derive(Deserialize)]`, not project logic. Not removed; filed as follow-up.

## Next Steps

**Unfinished (in-scope for this branch, deferred as beads):**
- `axon_rust-pkl.10.2`: Inline 4 wrapper functions into `enqueue_job` match arms
- `axon_rust-pkl.10.3`: Use proper domain error type instead of `sqlx::Error::Configuration` for cap rejection
- `axon_rust-pkl.10.4`: Cache queue cap env vars with `LazyLock` for consistency
- `axon_rust-pkl.10.5`: Fix `count as u64` cast (use `u64::try_from`)
- `axon_rust-pkl.11.1`: Document/address O(n) LRU scan bound in `evict_if_over_cap`
- `axon_rust-pkl.11.3`: Add single-victim-per-call doc to `evict_if_over_cap`
- `axon_rust-pkl.11.4`: Remove redundant `cap==0` check inside `evict_if_over_cap`
- `axon_rust-pkl.34.2`: Remove redundant clone in MCP `handle_map`
- `axon_rust-pkl.34.3`: Rename `MapResult.mapped_urls` to avoid confusion with `total`

**Follow-on tasks:**
- Create PR from `bd-work/p2-multi-remediation` → `main`
- `crates/services/types/service.rs` split due 2026-06-09 (in `.monolith-allowlist`)
- `CLAUDE.md` crawl queue cap section references stale file/function (`axon_rust-pkl.35`)
