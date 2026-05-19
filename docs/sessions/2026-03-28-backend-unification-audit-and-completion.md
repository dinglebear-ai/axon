# Session: Backend Unification Audit and Completion
Date: 2026-03-28
Branch: feat/lite-mode
Versions: v0.33.7 → v0.33.8

---

## Session Overview

Multi-agent audit of the axon_rust backend unification and services-first contract, followed by parallel remediation of all identified gaps. Started with 4 parallel analysis agents across independent domains, filed 5 beads from findings, then dispatched 5 parallel fix agents to close all issues in one pass. 1576 tests green throughout. Two commits pushed: `fcc7ba5b` (main refactor) and `de437d0f` (dead code cleanup).

---

## Timeline

1. **Audit dispatch** — 4 parallel agents analyzed: services layer completeness, JobBackend unification, CLI command compliance, MCP/web route compliance
2. **Findings filed** — 4 beads created from audit: sync-path bypasses, ServiceJobRuntime abstraction clarity, FullBackend Graph stubs, refresh lite-mode crash risk
3. **CancellationToken bug found and fixed inline** — discovered during audit validation that `LiteServiceRuntime::cancel_job` bypassed `CancelStore`, filed `axon_rust-trg` and fixed immediately
4. **5-way parallel fix dispatch** — all remaining open issues addressed simultaneously: `rs8.16`, `r21`, `93c`, `gx1`, `6nr`
5. **commit `fcc7ba5b`** — all fixes landed, 1576/1576 tests pass
6. **quick-push cleanup** — linter surfaced 2 residual files; dead `extract_status_raw`/`extract_list_raw` removed, `lift_ss` normalized
7. **commit `de437d0f`** — v0.33.8 pushed, parent bead `rs8` closed

---

## Key Findings

- **`LiteServiceRuntime::cancel_job` silent bug** (`crates/services/runtime.rs:182`): called `cancel_row()` directly, updating only the DB row. The in-process `CancellationToken` watched by the worker was never fired — cancels appeared to succeed but workers ran to completion. `LiteBackend::cancel_job` correctly used `CancelStore::cancel()` which does both.
- **`JobBackend` is a 3-method interface in practice**: only `enqueue`, `wait_for_job`, and `job_errors` are delegated through the trait. `FullServiceRuntime` and `LiteServiceRuntime` both bypass `JobBackend` for `list_jobs`, `job_status`, `cancel_job`, `cleanup_jobs`, `clear_jobs` — calling backend-specific query functions directly to return `ServiceJob` instead of the lossy `JobStatusRow`/`JobSummary`.
- **FullBackend Graph 4 stubs** (`crates/jobs/full.rs`): `enqueue`, `cancel_job`, `cleanup_jobs`, `clear_jobs` for `JobKind::Graph` returned runtime error strings. `LiteBackend` handled all 6 job types uniformly.
- **`services/refresh.rs` zero lite-mode awareness**: 9 functions called raw Postgres unconditionally. Guarded only at the MCP layer via `ServiceCapabilities` — service functions themselves would crash on a missing Postgres connection in lite mode.
- **3 sync-path CLI bypasses**: `migrate.rs` (375 lines of raw Qdrant HTTP), `extract.rs` `run_extract_sync` (called `run_extract_with_engine` directly), `crawl/sync_crawl.rs` (called `run_crawl_once`, `append_sitemap_backfill`, etc. directly).

---

## Technical Decisions

- **`CancelStore` accessor added to `LiteBackend`** rather than making `cancel_store` pub: keeps the field private while giving the service layer exactly what it needs (`pub fn cancel_store() -> &Arc<CancelStore>`).
- **Option (a) for refresh hardening** (add guards, not full port): porting refresh to `ServiceJobRuntime` was the right long-term fix but out of scope; adding `require_full_mode()` guards at the top of 9 functions provides defense-in-depth now without restructuring.
- **`ServiceJobRuntime` documentation over restructuring** for `gx1`: `JobBackend` is load-bearing for lite mode's clean delegation pattern — removing or privatizing it would be churn. Updated doc comments and CLAUDE.md files instead.
- **New service modules for sync paths**: `crates/services/migrate.rs` (new), `crates/services/crawl_sync.rs` (new), `services::extract::extract_sync()` (added to existing file) — kept CLI command files as thin formatters only.
- **`parse_graph_config()`** added to `crates/jobs/graph.rs` to bridge `JobPayload::Graph { config_json }` (which contains url + source_type) to `enqueue_graph_job(pool, cfg, url, source_type)` signature without changing any public API.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/jobs/lite.rs` | Added `pub fn cancel_store()` accessor; removed `table_for()` dead wrapper; all 6 `Self::table_for(kind)` calls replaced with `kind.table_name()` |
| `crates/jobs/full.rs` | Wired 4 Graph stubs: `enqueue`, `cancel_job`, `cleanup_jobs`, `clear_jobs` |
| `crates/jobs/graph.rs` | Added `cancel_graph_job`, `cleanup_graph_jobs`, `clear_graph_jobs`, `parse_graph_config` |
| `crates/jobs/backend.rs` | Added doc comment clarifying `JobBackend` reduced role |
| `crates/jobs/CLAUDE.md` | Added callout block: `JobBackend` is NOT canonical, `ServiceJobRuntime` is |
| `crates/services/runtime.rs` | `LiteServiceRuntime::cancel_job` → `CancelStore::cancel()`; removed `cancel_row` import; added module-level doc comment on two-layer architecture |
| `crates/services/runtime/full.rs` | Replaced inline table-name match with `kind.table_name()`; normalized all `\|e\| e.to_string().into()` to `lift_ss` |
| `crates/services/refresh.rs` | Added `require_full_mode(cfg)` helper; 9 functions guarded against lite-mode invocation |
| `crates/services/extract.rs` | Added `extract_sync()` service function; removed dead `extract_status_raw`/`extract_list_raw` + unused imports |
| `crates/services/migrate.rs` | **New file** — full Qdrant migration logic extracted from CLI |
| `crates/services/crawl_sync.rs` | **New file** — full sync crawl orchestration extracted from CLI |
| `crates/services/CLAUDE.md` | Updated `ServiceJobRuntime` section: marked as canonical, explained 3-method delegation pattern |
| `crates/services/types/service.rs` | Added `MigrateResult`, `ExtractSyncResult`, `CrawlSyncResult` result types |
| `crates/services.rs` | Registered new `migrate` and `crawl_sync` modules |
| `crates/crawl.rs` | Registered new `chrome_bootstrap` submodule |
| `crates/crawl/chrome_bootstrap.rs` | **New file** — Chrome bootstrap logic extracted from `crawl/runtime.rs` |
| `crates/cli/commands/migrate.rs` | 375 → 37 lines; now calls `migrate_service::migrate()` |
| `crates/cli/commands/extract.rs` | Sync path now calls `extract_service::extract_sync()` |
| `crates/cli/commands/crawl/sync_crawl.rs` | 428 → 10 lines; now calls `crawl_sync::crawl_sync()` |
| `crates/cli/commands/crawl/runtime.rs` | Chrome bootstrap calls moved to `chrome_bootstrap.rs` |
| `CHANGELOG.md` | Added v0.33.7 and v0.33.8 entries |
| `Cargo.toml` | Version bumped 0.33.7 → 0.33.8 |

---

## Commands Executed

```bash
# Audit
bd ready                        # 6 issues ready at session start

# Fixes
cargo check                     # clean after each agent fix pass

# Beads lifecycle
bd create ...                   # 5 beads created from audit findings + 1 for cancel bug
bd close axon_rust-trg          # cancel bug
bd close axon_rust-rs8.16 axon_rust-r21 axon_rust-93c axon_rust-gx1 axon_rust-6nr
bd close axon_rust-rs8          # parent closed once all children done

# Version
sed -i 's/version = "0.33.7"/version = "0.33.8"/' Cargo.toml
cargo check -q                  # update Cargo.lock

# Commits
git commit fcc7ba5b  # refactor: complete backend unification
git commit de437d0f  # chore: remove dead extract raw fns; normalize lift_ss
git push             # feat/lite-mode → origin
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `cancel_job` in lite mode | DB row updated, worker kept running | DB row updated AND `CancellationToken` fired → worker stops |
| Graph ops (full mode) | `enqueue`/`cancel`/`cleanup`/`clear` returned runtime error strings | All 4 wired to real Postgres functions |
| Refresh in lite mode | Would crash with Postgres connection error | Returns clean `"refresh is not supported in lite mode"` error |
| `migrate` CLI | 375 lines of raw Qdrant HTTP in CLI handler | CLI is 37-line formatter; logic in `services::migrate` |
| `sync_crawl` CLI | 428 lines calling crawl engine directly | CLI is 10-line wrapper; logic in `services::crawl_sync` |
| `extract` sync path | Called `run_extract_with_engine` directly from CLI | Routed through `services::extract::extract_sync()` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` (after cancel fix) | No errors | 0 errors | ✅ |
| `cargo check` (after all 5 agents) | No errors | 0 errors | ✅ |
| `cargo test` (fcc7ba5b) | 1576 pass, 0 fail | 1576 pass, 0 fail | ✅ |
| `cargo test` (de437d0f) | 1576 pass, 0 fail | 1576 pass, 0 fail | ✅ |
| `git push` | pushed to feat/lite-mode | `fcc7ba5b..de437d0f` pushed | ✅ |
| lefthook pre-commit hooks | all pass | monolith ✅, rustfmt ✅, no-mod-rs ✅, symlinks ✅ | ✅ |

---

## Source IDs + Collections Touched

Axon embed attempted below (session doc self-embed).

---

## Risks and Rollback

- **`cancel_store()` accessor**: public accessor added to `LiteBackend` — minimal risk; exposes `Arc<CancelStore>` which is already `Send+Sync`. Rollback: revert `lite.rs` accessor and `runtime.rs` cancel_job body.
- **Graph wiring in FullBackend**: `parse_graph_config()` parses `config_json` to extract `url`/`source_type` — if the JSON shape changes, it will return an error rather than panic. Rollback: revert `full.rs` Graph match arms to error stubs.
- **Sync path refactors**: behavior-preserving moves only; same logic, different call path. CLI commands now 10-37 lines. If a regression surfaces, `git revert fcc7ba5b` restores the full CLI implementations.

---

## Decisions Not Taken

- **Option (b) for refresh** (port to `ServiceContext`/`ServiceJobRuntime`): would make refresh work in lite mode but requires significant restructuring of `services/refresh.rs`. Deferred — refresh is explicitly unsupported in lite mode anyway.
- **Option (b)/(c) for JobBackend** (elevate or add richer types): `JobBackend` serves its purpose as a clean 3-method enqueue/wait/errors interface. Restructuring to force delegation for all methods would add complexity without runtime benefit.
- **Splitting `6nr` into 3 agents**: could have dispatched `migrate`, `extract_sync`, and `crawl_sync` as separate agents. Kept as one to maintain coherence across the "sync bypass" problem class.

---

## Open Questions

- The unwrap-warn hook flagged 7 `unwrap()` calls in `crates/services/migrate.rs` — all are in `#[cfg(test)]` test code. The hook is "warning only" but the pattern should be confirmed as test-only.
- GitHub Dependabot reports 1 high + 3 moderate vulnerabilities on the default branch. These are pre-existing and unrelated to this session's changes.
- `FullServiceRuntime::has_active_jobs` creates a fresh `PgPool` per call (identified in audit). Not fixed this session — filed conceptually under `gx1` but deferred.

---

## Next Steps

- `bd ready` is clear for the backend unification track
- Consider opening PR from `feat/lite-mode` → `main` — all rs8 children closed, services-first contract complete, 1576 tests green
- Address Dependabot vulnerabilities (separate track)
- Long-term: port refresh to `ServiceJobRuntime` so it works in lite mode (option b of `r21`)
