# Session: Remove Batch Job Backend Infrastructure

**Date:** 02/23/2026
**Branch:** `fix-crawl`
**Duration:** Two-part session (context compaction mid-way)

---

## Session Overview

Executed the "Remove Batch Job Infrastructure" plan to completion. The `axon batch` CLI command was removed in commit `5107ffc` (previous session), but the entire backend was left behind: the batch job worker (`crates/jobs/batch.rs` + `crates/jobs/batch/`), the queue injection rule engine, the s6 Docker worker, `batch_queue` config fields, status/doctor reporting, and all documentation references.

This session deleted all orphaned backend code, removed the queue injection rule engine that wired batch results to the extract queue, cleaned up all infrastructure config, and updated all documentation. Final state: `cargo check` clean, 334 tests passing, 0 clippy warnings, format clean.

**Critical distinction preserved throughout:** `batch_concurrency` config field was NOT removed — it controls HTTP concurrency for sitemap/crawl backfill and is unrelated to the batch AMQP job system.

---

## Timeline

1. **Verified compile state** — `cargo check` clean before starting
2. **Step 1**: Deleted 7 files/directories:
   - `crates/jobs/batch.rs`
   - `crates/jobs/batch/` (worker.rs, maintenance.rs, queue_injection.rs, tests.rs)
   - `commands/batch.md`
   - `docker/s6/s6-rc.d/batch-worker/` (run, type, finish)
   - `docker/s6/s6-rc.d/user/contents.d/batch-worker`
3. **Step 2**: Removed `pub mod batch;` from `crates/jobs.rs`
4. **Step 3**: Fixed all compile errors across 13 Rust source files — `JobTable::Batch`, `StatsTable::Batch`, `InjectionCandidate`, `apply_queue_injection`, `apply_queue_injection_with_pool`, `read_manifest_candidates`, mid-crawl injection block, `BatchJob`, `list_batch_jobs`, `print_batches`, `batch_doctor`, `batch_report`, `batch_queue`, `MID_CRAWL_INJECTION_*` constants — all removed
5. **`cargo check` clean** — 0.32s, no errors after Rust changes
6. **Step 4**: Infrastructure cleanup — `docker-compose.yaml` healthcheck, `.env.example`
7. **Step 5**: Documentation cleanup — `CLAUDE.md`, `README.md`, `crates/jobs/CLAUDE.md`, `docs/schema.md`, `COLLECTION-ROUTING.md`
8. **Step 6**: Final verification — `cargo check` ✓, grep scan (0 hits) ✓, `cargo test` 334/334 ✓, `cargo clippy` 0 warnings ✓, `cargo fmt --check` clean ✓

---

## Key Findings

- **`batch_enqueue_jobs()` in `common.rs` was NOT removed** — it is a general AMQP bulk-publish utility still called by `crates/jobs/crawl/runtime/db.rs:179` for crawl job bulk-enqueuing. The plan said to remove it, but caller inspection showed it must stay.
- **`read_manifest_candidates()` in `manifest.rs`** — function built `Vec<InjectionCandidate>` from manifest JSONL for the injection rule engine. Removed along with its test (`read_manifest_candidates_returns_expected_values_in_order`).
- **Mid-crawl injection block** in `worker_process.rs` fired at `MID_CRAWL_INJECTION_TRIGGER_PAGES=25` pages with minimum `MID_CRAWL_INJECTION_MIN_CANDIDATES=3` candidates. Entire block removed including `injection_attempted` variable and `Arc<Mutex<>>` state.
- **`CompletedResultContext` struct** in `result_builder.rs` had `mid_injection_state` field — removed. Now only holds: `summary`, `final_summary`, `robots_backfill_stats`, `robots_discovery_stats`, `final_prompt`.
- **Queue injection JSON fields** removed from crawl result JSON: `mid_queue_injection`, `queue_injection`, `extraction_observability`.

---

## Technical Decisions

- **Kept `batch_enqueue_jobs()`**: Inspected callers before removing. `crates/jobs/crawl/runtime/db.rs` still uses it. Removing would break crawl job submission — verified and kept.
- **Kept `batch_concurrency`**: Plan explicitly required this. Controls HTTP connection concurrency for sitemap/crawl backfill, not AMQP batch jobs.
- **Removed `AXON_QUEUE_INJECTION_RULES_JSON`**: The entire queue injection rule engine is gone. The env var was the only way to configure it. Removed from `.env.example`, `CLAUDE.md`, and `README.md`.
- **Worker count update**: Updated all docs from "5 workers (crawl/batch/extract/embed/ingest)" to "4 workers (crawl/extract/embed/ingest)".

---

## Files Modified

### Deleted Entirely
| File | Reason |
|------|--------|
| `crates/jobs/batch.rs` | Batch job module root |
| `crates/jobs/batch/worker.rs` | Batch AMQP worker |
| `crates/jobs/batch/maintenance.rs` | Stale job recovery for batch |
| `crates/jobs/batch/queue_injection.rs` | Rule engine routing batch→extract |
| `crates/jobs/batch/tests.rs` | Unit tests for queue injection |
| `commands/batch.md` | CLI docs for deleted command |
| `docker/s6/s6-rc.d/batch-worker/run` | s6 worker start script |
| `docker/s6/s6-rc.d/batch-worker/type` | s6 service type |
| `docker/s6/s6-rc.d/batch-worker/finish` | s6 worker finish script |
| `docker/s6/s6-rc.d/user/contents.d/batch-worker` | s6 bundle membership |

### Modified — Rust Source
| File | Changes |
|------|---------|
| `crates/jobs.rs` | Removed `pub mod batch;` |
| `crates/jobs/common.rs` | Removed `JobTable::Batch` + arm, `batch_queue` from `test_config()`, `axon_batch_jobs` from `count_stale_and_pending_jobs()` SQL |
| `crates/crawl/manifest.rs` | Removed `InjectionCandidate` import, `read_manifest_candidates()` fn + test |
| `crates/jobs/crawl/runtime.rs` | Removed `read_manifest_candidates` re-export, `MID_CRAWL_INJECTION_*` constants |
| `crates/jobs/crawl/runtime/worker/result_builder.rs` | Rewrote: removed `apply_queue_injection`, `mid_injection_state`, injection JSON fields from `CompletedResultContext` |
| `crates/jobs/crawl/runtime/worker/worker_process.rs` | Removed `apply_queue_injection_with_pool`, `injection_attempted`, mid-crawl injection block, `Arc` import, `mid_injection_state` Arc |
| `crates/vector/ops/stats/pg.rs` | Removed `StatsTable::Batch`, `batch_count` from `PostgresMetrics`, `collect_batch_metrics()` fn, batch from `tokio::join!` |
| `crates/vector/ops/stats/display.rs` | Removed `Batches:` display block |
| `crates/vector/ops/stats.rs` | Removed `"batches"` from JSON counts |
| `crates/cli/commands/status.rs` | Removed `BatchJob`, `list_batch_jobs`, `batch_metrics_suffix`, `print_batches()`, batch from all signatures |
| `crates/cli/commands/status/metrics.rs` | Deleted `batch_metrics_suffix()` |
| `crates/cli/commands/doctor.rs` | Removed `batch_doctor`, `batch_report` from `DoctorProbes`, `gather_doctor_probes`, `build_pipeline_status`, `build_queue_names`, `report_overall_ok` |
| `crates/cli/commands/doctor/render.rs` | Removed `"batch"` from pipelines array |
| `crates/core/config/help.rs` | Removed `batch [urls...]` help entry |
| `crates/core/config/types.rs` | Removed `batch_queue: String` field, default `"axon.batch.jobs"`, Debug impl line |
| `crates/core/config/cli.rs` | Removed `pub(super) batch_queue: Option<String>` clap arg |
| `crates/core/config/parse.rs` | Removed `batch_queue:` env var resolution block |

### Modified — Infrastructure + Docs
| File | Changes |
|------|---------|
| `docker-compose.yaml` | Removed `batch-worker` from healthcheck `s6-svstat` chain |
| `.env.example` | Removed `AXON_BATCH_QUEUE` entry + `AXON_QUEUE_INJECTION_RULES_JSON` entry with comments |
| `CLAUDE.md` | Removed `batch_jobs/` from architecture, `batch-worker` from s6 list, `--batch-queue` from flags table, workers 5→4, `AXON_BATCH_QUEUE` and `AXON_QUEUE_INJECTION_RULES_JSON` from env vars, `axon_batch_jobs` from schema table |
| `README.md` | Removed batch from features list, commands list, crate description, architecture tree, s6 tree, env vars table, queue injection row, workers section, flags table, stale recovery examples, `axon_batch_jobs` schema section |
| `crates/jobs/CLAUDE.md` | Removed `batch_jobs/` from module layout, removed "Queue Injection" section |
| `docs/schema.md` | Removed `axon_batch_jobs` table section, removed from cross-reference table |
| `COLLECTION-ROUTING.md` | Removed `batch <urls>` routing row |

---

## Commands Executed

| Command | Result |
|---------|--------|
| `cargo check` (after Rust changes) | `Finished 'dev' profile in 0.32s` — clean |
| `grep -r "batch_queue\|list_batch_jobs\|BatchJob\|..."` (Rust files, excluding batch_concurrency) | No output — zero lingering references |
| `cargo test` | `334 passed; 0 failed` |
| `cargo clippy` | `Finished 'dev' profile in 7.20s` — 0 warnings |
| `cargo fmt --check` | Clean |
| `cargo check` (final, after docs) | `Finished 'dev' profile in 0.55s` — clean |

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `axon help` | Listed `batch [urls...]` command | No batch entry |
| `axon status` | Showed batch job section | No batch section |
| `axon doctor` | Reported batch worker pipeline health | No batch pipeline |
| `axon stats` | Showed `Batches: N` count | No batches line |
| Crawl result JSON | Included `mid_queue_injection`, `queue_injection`, `extraction_observability` keys | Keys removed |
| `docker-compose` healthcheck | Verified `batch-worker` via s6-svstat | Only checks crawl/extract/embed/ingest workers |
| Worker container | 5 long-lived workers (crawl/batch/extract/embed/ingest) | 4 workers (crawl/extract/embed/ingest) |
| `Config.batch_queue` | String field present | Field removed |
| `AXON_BATCH_QUEUE` env var | Recognized and used | No longer parsed |
| `AXON_QUEUE_INJECTION_RULES_JSON` env var | Loaded injection rules | No longer parsed |
| `--batch-queue` CLI flag | Available globally | Removed |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` (post-Rust) | 0 errors | `Finished in 0.32s` | ✓ PASS |
| `grep batch_queue ...crates/` | 0 matches | No output | ✓ PASS |
| `grep list_batch_jobs ...crates/` | 0 matches | No output | ✓ PASS |
| `grep axon_batch_jobs ...crates/` | 0 matches | No output | ✓ PASS |
| `cargo test` | All pass | 334 passed, 0 failed | ✓ PASS |
| `cargo clippy` | 0 warnings | `Finished in 7.20s` | ✓ PASS |
| `cargo fmt --check` | Clean | Clean | ✓ PASS |
| `cargo check` (final) | 0 errors | `Finished in 0.55s` | ✓ PASS |

---

## Source IDs + Collections Touched

None — this session made no Axon embed/retrieve calls. Changes are to source code and documentation only.

---

## Risks and Rollback

- **Compile breakage risk**: Low. `cargo check` clean after each change group; all 334 tests pass.
- **`batch_enqueue_jobs()` retained**: This function in `common.rs` is a generic AMQP utility, not batch-specific. Its retention is correct and safe.
- **Rollback**: `git revert` of the commit(s) produced by this session would restore all deleted code. Alternatively `git checkout main -- <files>` for specific files.
- **Database impact**: `axon_batch_jobs` table still exists in any running Postgres instance. The code no longer creates it on startup or writes to it, but it won't be dropped automatically. Safe to leave or drop manually: `DROP TABLE IF EXISTS axon_batch_jobs;`

---

## Decisions Not Taken

- **Removing `batch_enqueue_jobs()`**: Plan said to remove it, but caller inspection showed `crates/jobs/crawl/runtime/db.rs` still uses it for crawl batch submission. Kept.
- **Dropping `axon_batch_jobs` table in migration**: Out of scope — no migration system, table is inert and harmless.
- **Removing TEI/Qdrant internal `batch_*` variables**: Correctly kept — these are embed infrastructure, not job infrastructure.

---

## Open Questions

- Running Postgres instances may still have the `axon_batch_jobs` table. No automatic migration to drop it.
- Any existing `axon_batch_jobs` rows in a live database are now orphaned but harmless.

---

## Next Steps

- Run `just verify` (full pre-PR gate: fmt-check + clippy + check + test) as final gate before merging `fix-crawl` → `main`
- Consider a `DROP TABLE IF EXISTS axon_batch_jobs;` migration if running a live database instance
- Update session memory file with removal confirmation
