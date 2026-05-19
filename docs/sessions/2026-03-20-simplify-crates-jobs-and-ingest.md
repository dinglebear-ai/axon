# Session: Deep Simplify — crates/jobs + crates/ingest

**Date:** 2026-03-20
**Scope:** Full code review and fix cycle across `crates/jobs/` (67 files, 18,277 lines) and `crates/ingest/` (22 files, 6,739 lines)
**Method:** 9-agent parallel review → compiled findings → 9-agent parallel fix (jobs) + 6-agent review → 3-agent fix (ingest)

---

## Session Overview

Comprehensive code quality sweep using 27 specialized Rust Pro subagents across two rounds per module. Each agent loaded `/acp`, `/rust-best-practices`, `/rust-async-patterns`, and `/rust-code-review` skills before analysis. All agents operated read-only in round 1, then write-enabled in round 2 to apply targeted fixes. Large refactors were explicitly deferred.

**Result:** 8 bugs fixed, 42+ warnings addressed, 1,429 tests passing, 0 clippy warnings, clean `cargo check`.

---

## Timeline

1. **Round 1 — `crates/jobs/` review:** 9 agents partitioned across all 67 .rs files, ran in parallel (~2-4 min each)
2. **Compiled findings:** 4 bugs, 28 warnings, ~50 info items catalogued
3. **Round 2 — `crates/jobs/` fixes:** 9 fix agents dispatched in parallel, each targeting a specific category of issues
4. **Verification:** `cargo check` clean, `cargo clippy` clean
5. **Round 1 — `crates/ingest/` review:** 6 agents across all 22 files
6. **Compiled findings:** 4 bugs, 14 warnings, ~25 info items
7. **Round 2 — `crates/ingest/` fixes:** 3 fix agents
8. **Final verification:** `cargo check`, `cargo clippy`, `cargo test --lib` — all 1,429 tests pass, 0 failures, 0 warnings

---

## Key Findings

### Bugs Fixed — `crates/jobs/`

| File | Line | Severity | Issue | Fix |
|------|------|----------|-------|-----|
| `graph/schema.rs` | 15 | **CRITICAL** | Missing `result_json JSONB` column in `axon_graph_jobs` — every graph job completion hit Postgres error | Added column to CREATE TABLE + ALTER TABLE migration |
| `crawl/runtime/worker/job_context.rs` | 49 | **HIGH** | Raw SQL `'canceled'` string instead of `JobStatus::Canceled.as_str()` | Parameterized bind with `$1` |
| `watch_worker.rs` | 58,70 | **HIGH** | `let _ =` silently discarded `Ok(false)` from `mark_watch_run_finished_with_pool` — masked state bugs | Added warning log on `false` return |
| `embed/worker.rs` | 208 | **HIGH** | Successful embed marked as failed when completion-UPDATE SQL fails — caused duplicate embeddings on retry | Caught SQL error with `log_warn` instead of propagating via `?` |

### Bugs Fixed — `crates/ingest/`

| File | Line | Severity | Issue | Fix |
|------|------|----------|-------|-----|
| `reddit/comments.rs` | 67 | **HIGH** | Missing `raw_json=1` — comment bodies got HTML-encoded entities (`&amp;`) in Qdrant | Added `&raw_json=1` to URL |
| `youtube.rs` | 200 | **HIGH** | Path-extracted video IDs (`/embed/`, `/shorts/`) bypassed char guard — query param injection | Added `[a-zA-Z0-9_-]` validation on all path-extracted IDs |
| `github/files/line_range.rs` | 8 | **MEDIUM** | Non-char-boundary `byte_offset` panics on UTF-8 slice | Added char boundary walk-back loop |
| `github/meta.rs` | 134 | **LOW** | Test named `payload_has_31_keys` but asserts 32 | Renamed to `payload_has_32_keys` |

### Key Warning Categories Fixed

- **Async span guards:** 3 files using `.entered()` across `.await` → converted to `.instrument()` (embed, extract, crawl workers)
- **Blocking I/O:** `std::fs::read_to_string` in `graph/taxonomy.rs` → `tokio::fs::read_to_string` (+ caller updates)
- **Duration panic:** `poll.rs` subtraction → `saturating_sub`
- **Negative timeout:** `watchdog.rs` clamp added (prevents matching ALL running jobs)
- **Pool size zero:** `pool.rs` added `.max(1)` floor
- **SQL parameterization:** 3 files with `format!` SQL → parameterized `$N` binds
- **Raw status strings:** 6+ test assertions using `"failed"` → `JobStatus::Failed.as_str()`
- **AMQP close codes:** 3 sites using non-standard `0` → spec-compliant `200`
- **Dead code:** Removed `GRAPH_QUEUE_DEFAULT`, unused `cfg` in watch test, unnecessary `#[allow]`
- **Redis clones:** 8 sites `.clone()` → `.as_str()` (avoids allocation)
- **Error chain destruction:** 17 sites in ingest/ where `.map_err(|e| anyhow!(e.to_string()))` destroyed source chains → replaced with `?` or `{e:#}`
- **Redundant clones:** reddit.rs `extra.clone()` (2 sites), youtube.rs `source_url`/`desc_url` clones
- **Duplicate embedding:** YouTube description embedded N times (per VTT) → moved outside loop
- **Truncating cast:** `u64 as u32` on PR comments → saturating conversion

---

## Files Modified

### `crates/jobs/` (Round 2 fixes)

| File | Changes |
|------|---------|
| `graph/schema.rs` | Added `result_json JSONB` column + ALTER TABLE migration |
| `crawl/runtime/worker/job_context.rs` | Parameterized `canceled` status + added `JobStatus` import |
| `watch_worker.rs` | Warning log on `Ok(false)` from mark_watch_run_finished (2 sites) |
| `embed/worker.rs` | SQL error on success path logged instead of propagated; `.instrument()` span; redis `.as_str()` |
| `extract/worker.rs` | `.instrument()` span; redis `.as_str()` |
| `crawl/runtime/worker/process.rs` | `.instrument()` span; redis `.as_str()` |
| `crawl/runtime/worker/job_context.rs` | Redis `.as_str()` |
| `crawl/runtime/db.rs` | Redis `.as_str()` |
| `embed.rs` | Redis `.as_str()` |
| `extract.rs` | Redis `.as_str()` |
| `worker_lane/poll.rs` | `saturating_sub` on Duration |
| `common/watchdog.rs` | `.clamp(0, ...)` on timeout; removed redundant `table_name` binding |
| `common/pool.rs` | `.max(1)` on pool size |
| `common/amqp.rs` | Removed dead `GRAPH_QUEUE_DEFAULT`; AMQP close code 200 (2 sites) |
| `worker_lane.rs` | AMQP close code 200 |
| `refresh/processor.rs` | Removed unnecessary `#[allow]`; SQL parameterized bind |
| `watch.rs` | Removed dead `cfg` binding in test |
| `extract/tests.rs` | Raw status strings → `JobStatus` enum |
| `common/tests/db_lifecycle.rs` | Raw status strings → `JobStatus` enum |
| `graph/taxonomy.rs` | `std::fs` → `tokio::fs` (async) |
| `graph/context.rs` | Updated caller for async `Taxonomy::from_path` |
| `graph/worker.rs` | Updated caller for async `Taxonomy::from_path` |

### `crates/ingest/` (Round 2 fixes)

| File | Changes |
|------|---------|
| `reddit/comments.rs` | Added `raw_json=1` to URL |
| `youtube.rs` | Video ID char validation; description embed moved outside loop; clone eliminations |
| `github/files/line_range.rs` | UTF-8 char boundary walk-back |
| `github/meta.rs` | Test rename 31→32 |
| `sessions/claude.rs` | 7 error chain fixes (→ `?`) |
| `sessions/codex.rs` | 5 error chain fixes (→ `?`) |
| `github/issues.rs` | 2 error chain fixes (→ `{e:#}`); saturating u64→u32 cast |
| `github/wiki.rs` | 1 error chain fix (→ `{e:#}`) |
| `github/files/batch.rs` | 1 error chain fix (→ `{e:#}`); added log_warn on discarded error |
| `github.rs` | 1 error chain fix (→ `{e:#}`) |
| `reddit.rs` | 2 redundant `extra.clone()` removed |
| `sessions.rs` | 1 error chain fix (→ `{e:#}`) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Clean compile | `Finished dev profile in 1.65s` | PASS |
| `cargo clippy -- -W clippy::all` | 0 warnings | 0 output | PASS |
| `cargo test --lib` | All tests pass | `1429 passed; 0 failed; 11 ignored` | PASS |

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Graph job completion | Postgres error on every successful graph job (missing column) | Jobs complete normally, result_json stored |
| Embed retry on SQL failure | Successful embed re-run on retry (duplicate Qdrant points) | SQL error logged, job stays `running` for watchdog recovery |
| Watch run finalization | Silent no-op when run ID doesn't match — no diagnostic output | Warning logged with watch_id + run_id |
| Reddit comment quality | HTML entities (`&amp;`, `&lt;`) embedded in Qdrant | Clean text via `raw_json=1` |
| YouTube video ID safety | Crafted `/embed/foo&bar=baz` URL injects query params | Invalid chars rejected, returns None |
| Tracing in async workers | Span timing incorrect across thread migrations | Correct async-aware `.instrument()` spans |
| Error diagnostics in ingest | Error source chains destroyed (17 sites) | Full chain preserved via `?` or `{e:#}` |
| YouTube description embedding | N duplicate Qdrant points per video (one per VTT file) | Single point per video |

---

## Risks and Rollback

- **Graph schema migration:** The ALTER TABLE runs on existing DBs. If `axon_graph_jobs` already has a `result_json` column (from a manual fix), the `IF NOT EXISTS` guard prevents errors. Rollback: revert `graph/schema.rs` only.
- **Taxonomy async change:** `Taxonomy::from_path` is now async. All callers were already async. If a new sync caller is added, it will fail to compile (safe failure mode).
- **Embed completion path:** Jobs that fail the completion UPDATE now stay as `running` instead of `failed`. The watchdog will eventually reclaim them. This is strictly better than marking successful work as failed.

---

## Decisions Not Taken

- **Per-call `make_pool` in CLI functions:** Multiple review agents flagged pool-per-call in `crawl/runtime/db.rs` and others. Deferred — requires API signature changes across many public functions.
- **Duplicate `sorted_urls`/`sorted_vec` helpers:** Identified in `process.rs` + `result_builder.rs`. Deferred — requires choosing a shared location and updating imports.
- **Double-sort (SQL ORDER BY + Rust re-sort):** Found in 4+ modules. Deferred — need to verify sort orders are identical before removing one.
- **`Config` wrapped in `Arc`:** Multiple agents noted `cfg.clone()` per job. Deferred — requires changing the `ProcessFn` type signature in worker_lane.
- **`GitHubPayloadParams` taking `&str` instead of `String`:** Would eliminate ~8 clones per issue/file. Deferred — lifetime annotations required.

---

## Open Questions

- Are the raw status strings in `crawl/runtime/db.rs` and `crawl/runtime.rs` (flagged by fix-2 agent) worth a follow-up pass? They're in SQL `format!` patterns with compile-time constants — safe but inconsistent.
- Should `ensure_schema_once` patterns (OnceLock + is_none) be migrated to `tokio::sync::OnceCell` across the board? Currently safe due to advisory locks but technically racy.
- The `preack_cap` in `worker_lane/amqp.rs` scales with `lane_count` (global) not per-lane concurrency — is this intentional or a bug?

---

## Next Steps

1. Follow-up simplify pass on remaining `crates/` modules (vector, crawl, services, web, mcp, cli)
2. Address deferred items (pool-per-call, duplicate helpers, double-sorts) as a separate refactor PR
3. Run `cargo test` with integration tests enabled to verify graph schema migration on a real Postgres
