# Tightening Audit — Remaining Issues + Broader Codebase Review
Last Modified: 2026-02-28
Session: 2026-02-28 | feat/crawl-download-pack

## Session Overview

Continuation session after the 8-agent code review team landing. Completed the pending `save-to-md` Axon embed + Neo4j steps from the prior session, then conducted a full audit of remaining open issues and broader codebase tightening opportunities. No code was changed this session — purely analysis and documentation.

---

## Timeline

| Activity | Detail |
|----------|--------|
| Session resume | Continued from prior context-limit cutoff mid-`save-to-md` workflow |
| Axon embed completed | `docs/sessions/2026-02-28-code-review-fixes-8-agent-team.md` → job `8aab8b53`, `cortex` collection, 1 chunk, verified |
| Neo4j entities created | 9 entities + 10 relations capturing the prior session's P0/P1/P2 fix work |
| LOE issues audit | Listed 4 open items from prior session's documented open questions |
| Broader tightening audit | Scanned TODOs, unwraps, SQL patterns, Redis timeouts, heartbeat consistency, test gaps, blocking I/O |

---

## Key Findings

### Prior Session save-to-md completion
- Embed job `8aab8b53-def2-4828-a7e6-7996e21eee26` completed in 174ms, 1 doc / 1 chunk → `cortex` collection
- Retrieve confirmed: `docs/sessions/2026-02-28-code-review-fixes-8-agent-team.md` (1 chunk returned)
- Neo4j: 9 entities, 10 relations created for the P0/P1/P2 fix work

### LOE Issues (from prior session open questions)
1. **M-6 `max_sitemaps`**: `crates/jobs/crawl/runtime/robots.rs:100-101` has hardcoded `512usize`. `max_sitemaps` field does NOT exist in `crates/core/config/types.rs`. Field exists only in spider_agent's re-exported Config (used by `crates/crawl/engine/sitemap.rs:198`). Jobs Config is missing it entirely.
2. **10 unwrap/expect in tests**: `worker_lane/mod.rs` lines 226-321 (9, semaphore `.acquire_owned().await.unwrap()` in test code); `amqp_consumer.rs:361` (1, UUID literal `.expect()`). All in `#[cfg(test)]` — warning-only.
3. **5 functions over 80-line warning threshold**: `discover_sitemap_urls_with_robots`, `process_embed_job`, `process_extract_job`, `setup_refresh_job_context`, `process_single_refresh_url`.
4. **Advisory lock integration test gap**: No test covers ≥2 worker processes racing `ensure_schema()` simultaneously.

### Broader Tightening Findings

#### A — Consistency Bugs

**A1. Redis timeout inconsistency** — three callers lack `tokio::time::timeout` on `get_multiplexed_async_connection()`:
- `crates/jobs/crawl/runtime/db.rs:238`
- `crates/jobs/extract.rs:203`
- `crates/jobs/embed.rs:216`
Pattern is established in `job_context.rs`, `amqp_consumer.rs`, `extract/worker.rs`, `embed/worker.rs` (all use 3s timeout). These three are the cancel-signal paths.

**A2. `embed/worker.rs` uses raw SQL heartbeat** — `embed/worker.rs:170` inlines `UPDATE axon_embed_jobs SET updated_at=NOW()` directly. All other workers (extract, ingest, refresh) use `touch_running_job(&pool, TABLE, id)` from `common`. The TODO at line 159 explicitly flags this.

**A3. `refresh/mod.rs:371` silently drops `create_refresh_schedule` error** — `let _ = create_refresh_schedule(...)` swallows the failure entirely. Should at minimum `log_warn`.

#### B — Missing Abstractions

**B1. Inline heartbeat pattern duplicated** — spawn-watch-channel heartbeat task pattern is repeated in embed, extract, ingest, and refresh workers. The TODO at `embed/worker.rs:159` proposes `common::spawn_heartbeat_task(pool, table, id, interval_secs) -> JoinHandle`.

**B2. `std::fs` blocking I/O in async paths**:
- `crates/jobs/refresh/url_processor.rs:36,38` — `std::fs::canonicalize()` in async function
- `crates/vector/ops_v2/source_display.rs:161` — `std::fs::read_to_string()` in async path
- `crates/core/logging.rs:43` — documented TODO (PERF-MED-4): `rotate_if_needed` uses `fs::rename` + `File::create` in sync writer

#### C — Test Gaps (14 files with no tests)

| File | Lines | Priority |
|------|-------|----------|
| `crawl/runtime/worker/process.rs` | 448 | High |
| `crawl/runtime/db.rs` | 311 | High |
| `jobs/common/watchdog.rs` | 219 | High |
| `jobs/common/job_ops.rs` | 173 | High |
| `crawl/runtime/worker/loops.rs` | 196 | Medium |
| `crawl/runtime/worker/job_context.rs` | 192 | Medium |
| `jobs/extract/worker.rs` | 337 | Medium |
| `jobs/embed/worker.rs` | 269 | Medium |
| `refresh/schedule.rs` | 252 | Medium |
| `crawl/runtime/robots.rs` | 254 | Medium |
| `worker_lane/amqp.rs` | 142 | Low |
| `worker_lane/poll.rs` | 108 | Low |
| `worker_lane/delivery.rs` | 83 | Low |
| `crawl/runtime/worker/postprocess.rs` | 82 | Low |

All 14 are files created/split by the agent team. Pure-logic functions (URL validation, config parsing, status transitions) can be unit-tested without live services. DB/AMQP-touching functions need integration tests.

#### D — Monolith Warnings
- `embed/worker.rs:134` TODO: 96-line `process_embed_job` over the 80-line warning threshold. Self-annotated for splitting.

---

## Technical Decisions

- **No code changed this session** — audit only. All findings documented for follow-up.
- **SQL format! injection risk assessed as safe**: All `format!()` SQL calls in `crates/jobs/common/` interpolate only `JobTable::as_str()` (enum with static strings) and `JobStatus::as_str()` (same). Table names and status values are compile-time constants, not user input.
- **`max_sitemaps` gap is more significant than noted in prior session**: The field exists in the spider_agent Config re-export (used by the crawl engine directly), but is absent from `crates/core/config/types.rs` Config struct used by the jobs subsystem. The CLI flag `--max-sitemaps` is documented and accepted but has no effect on the jobs worker path.

---

## Files Modified

*(No files modified this session — analysis only.)*

---

## Commands Executed

```bash
# LOE audit
grep -rn "max_sitemaps" crates/
grep -n "\.unwrap()\|\.expect(" crates/jobs/crawl/runtime/worker/amqp_consumer.rs crates/jobs/worker_lane/mod.rs

# Broader audit
grep -rn "TODO\|FIXME\|HACK" crates/ --include="*.rs" | grep -v "test|#\["
grep -rn "std::fs::" crates/ --include="*.rs" | grep -v "test|target/"
grep -rn "get_multiplexed_async_connection" crates/ --include="*.rs" | grep -v "test|target/"
grep -B3 "get_multiplexed_async_connection" crates/jobs/ -r  # timeout vs bare comparison
# Check heartbeat consistency
grep -rn "touch_running_job" crates/ --include="*.rs"
grep -n "touch_running_job|updated_at.*NOW|SET updated_at" crates/jobs/embed/worker.rs
# Test coverage
for f in $(find crates/jobs -name "*.rs"); do [check test presence]; done
# SQL injection assessment
grep -n "enum JobTable|JobTable::|fn as_str" crates/jobs/common/mod.rs crates/jobs/status.rs
```

---

## Behavior Changes (Before/After)

*(No behavior changes — audit session only.)*

---

## Verification Evidence

| Item | Status | Detail |
|------|--------|--------|
| Prior session embed (job `8aab8b53`) | ✅ Completed | 1 doc / 1 chunk in `cortex`, 174ms |
| Prior session retrieve verify | ✅ Confirmed | 1 chunk returned |
| Prior session Neo4j | ✅ 9 entities, 10 relations | all created successfully |
| SQL injection risk assessment | ✅ Safe | `JobTable`/`JobStatus` are enums with `&'static str` returns |
| `max_sitemaps` in jobs Config | ❌ Missing | Field absent from `crates/core/config/types.rs` |
| Redis timeout consistency | ❌ 3 callers bare | `db.rs:238`, `extract.rs:203`, `embed.rs:216` |
| `touch_running_job` in embed worker | ❌ Missing | Uses raw SQL inline instead |

---

## Source IDs + Collections Touched

| Source ID | Collection | Action | Outcome |
|-----------|------------|--------|---------|
| `docs/sessions/2026-02-28-code-review-fixes-8-agent-team.md` | `cortex` | embed + retrieve | ✅ confirmed |

---

## Risks and Rollback

- **No code changes made** — no rollback needed.
- **A1 Redis timeout risk**: Under Redis failure, cancel operations in `db.rs`, `extract.rs`, `embed.rs` can hang indefinitely (no timeout guard). Low probability but unresolvable without intervention.

---

## Decisions Not Taken

- **Did not start fixing any issues** — user asked for audit only, not implementation.
- **Did not assess `ops_v2` vs `ops` split** — that's a separate structural concern not surfaced in this scan.

---

## Open Questions

- **`max_sitemaps` CLI flag behavior**: When a user passes `--max-sitemaps 100` does it affect job workers at all, or only the inline sync crawl path through `crawl/engine/sitemap.rs`? Likely only the sync path.
- **`ops_v2` vs `ops` modules**: `source_display.rs` exists in both `crates/vector/ops/` and `crates/vector/ops_v2/` — are these being consolidated or intentionally parallel?
- **`ingest/sessions/gemini.rs:353,378,391`** uses `std::fs::File::create` in `#[cfg(test)]` blocks — confirm these are test-only (low risk).

---

## Next Steps

Prioritized tightening backlog by effort and impact:

| # | Item | LOE | Risk if deferred |
|---|------|-----|-----------------|
| 1 | Add Redis 3s timeout to `db.rs:238`, `extract.rs:203`, `embed.rs:216` | 30m | Hung cancel under Redis failure |
| 2 | Replace raw SQL heartbeat in `embed/worker.rs` with `touch_running_job()` | 15m | Divergent behavior |
| 3 | Log swallowed `create_refresh_schedule` error in `refresh/mod.rs:371` | 10m | Silent failures |
| 4 | Add `max_sitemaps: usize` to jobs Config + wire through `robots.rs` | 1h | `--max-sitemaps` flag has no effect on job workers |
| 5 | Extract `spawn_heartbeat_task` helper to `common/` | 45m | Code duplication |
| 6 | Fix `std::fs` blocking calls in async paths (`url_processor.rs`, `source_display.rs`) | 45m | Tokio runtime stalls |
| 7 | Unit tests for pure-logic functions in the 14 untested files | 4–8h | Low confidence in agent-written code |
| 8 | Split `process_embed_job` (96L, over 80L threshold) | 1h | Pre-commit warning |
| 9 | Convert test `.unwrap()` calls to `?` in `worker_lane/mod.rs` | 30m | Cosmetic |
| 10 | Advisory lock integration test (≥2 workers racing `ensure_schema`) | 2h | Confidence gap only |
