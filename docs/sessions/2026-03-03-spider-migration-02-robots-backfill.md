# Spider Migration 02: Robots Backfill → Engine Sitemap Pipeline

**Date:** 2026-03-03
**Branch:** `feat/sidebar`
**Base SHA:** `c9ebd58b` → **Head SHA:** `370ee1af`
**Plan:** `docs/plans/2026-03-03-spider-migration-02-robots-backfill.md`

## Session Overview

Executed the Spider Migration 02 plan using subagent-driven development (5 tasks, TDD RED→GREEN→REFACTOR). Migrated the CLI's `append_robots_backfill()` from `crates/cli/commands/crawl/audit/backfill.rs` into the engine layer at `crates/crawl/engine/sitemap.rs` as `append_sitemap_backfill()`. This consolidates sitemap backfill logic into the engine, eliminating a 117-line CLI-owned fetch loop that duplicated engine-level concerns.

## Timeline

1. **Read plan + explore codebase** — Understood existing backfill.rs, engine sitemap.rs, sync_crawl.rs call site, worker-path robots.rs, test patterns
2. **Task 1 (RED)** — Created `sync_backfill_migration_tests.rs` with 2 failing tests calling non-existent `append_sitemap_backfill`
3. **Task 2 (GREEN)** — Implemented `BackfillStats` + `append_sitemap_backfill()` in engine/sitemap.rs, re-exported from engine.rs, rewired sync_crawl.rs
4. **Task 3 (CLEANUP)** — Deleted backfill.rs (117 lines), cleaned audit.rs (removed dead module, unused imports, orphaned function)
5. **Task 4 (VERIFY)** — Confirmed metrics contract intact: job_contracts tests pass, worker path untouched
6. **Task 5 (DOCS)** — Updated crates/cli/CLAUDE.md module layout and sync_crawl section

Each task went through: implementer → spec reviewer → code quality reviewer.

## Key Findings

- **Engine already had all primitives**: `discover_sitemap_urls()`, `fetch_text_with_retry()`, `to_markdown()`, `ManifestEntry` — the CLI loop was duplicating these
- **Worker path (`crates/jobs/crawl/runtime/robots.rs`) is separate** — has different retry semantics (linear backoff, raw JSON manifest lines vs typed ManifestEntry). Out of scope for this migration.
- **Double `validate_url` call** caught by code quality review — `fetch_text_with_retry` already validates internally (`sitemap.rs:46`)
- **`build_client` vs `http_client` singleton** — new function creates fresh client; TODO added for future consolidation when `discover_sitemap_urls` also migrates

## Technical Decisions

- **Engine placement over shared module**: Function lives in `crates/crawl/engine/sitemap.rs` (not a new file) because it composes existing sitemap discovery with fetch+write — all sitemap concerns in one place
- **`BackfillStats` as return type**: Provides granular diagnostic counters (discovered_urls, candidates, fetched_ok, written, failed) instead of the CLI's opaque `RobotsBackfillStats`
- **No worker-path changes**: The worker's `robots.rs` has intentionally different backoff/retry semantics — consolidating it requires a separate migration with its own test coverage
- **Kept `build_client` for now**: Switching to `http_client()` LazyLock singleton would require changing `discover_sitemap_urls` too — consistency matters more than micro-optimization

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/crawl/engine/sitemap.rs` | Modified (+128 lines) | Added `BackfillStats` struct + `append_sitemap_backfill()` function |
| `crates/crawl/engine.rs` | Modified (+1 line) | Added `pub use sitemap::{BackfillStats, append_sitemap_backfill}` |
| `crates/cli/commands/crawl/sync_crawl.rs` | Modified (-30/+12 lines) | Replaced `append_robots_backfill` call with engine `append_sitemap_backfill` |
| `crates/cli/commands/crawl/audit.rs` | Modified (-33 lines) | Removed backfill module, dead_code annotations, orphaned `fetch_text_with_retry` |
| `crates/cli/commands/crawl/audit/backfill.rs` | Deleted (-117 lines) | Old CLI-owned backfill loop — fully replaced by engine function |
| `crates/cli/commands/crawl/sync_backfill_migration_tests.rs` | Created (+254 lines) | 2 integration tests: metrics contract + manifest dedup |
| `crates/cli/commands/crawl.rs` | Modified (+3 lines) | Added `#[cfg(test)] mod sync_backfill_migration_tests` |
| `crates/cli/CLAUDE.md` | Modified | Updated module layout tree + sync_crawl docs |

## Commands Executed

```bash
cargo test crawl          # 90 passed, 3 failed (pre-existing Postgres integration)
cargo clippy              # clean
cargo fmt --check         # clean for our files (pre-existing mcp/server.rs issue)
cargo test job_contracts  # 7/7 pass
```

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test sync_backfill_migration` | 2 pass | 2 pass | PASS |
| `cargo test crawl` (non-Postgres) | 90 pass | 90 pass | PASS |
| `cargo test job_contracts` | 7 pass | 7 pass | PASS |
| `cargo clippy` | 0 warnings | 0 warnings | PASS |
| backfill.rs deleted | file gone | confirmed | PASS |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Sync crawl backfill | CLI-owned fetch loop in `audit/backfill.rs` | Delegates to `crawl::engine::append_sitemap_backfill()` |
| Backfill stats type | `RobotsBackfillStats` (3 fields) | `BackfillStats` (5 fields: +candidates, +fetched_ok) |
| Manifest dedup | Checked only `seen_urls` | Checks both `seen_urls` AND existing manifest entries |
| URL validation | Double-validated (caller + fetch_text_with_retry) | Single validation inside fetch_text_with_retry |

## Risks and Rollback

- **Low risk**: The engine function follows the same flow as the deleted CLI loop. Tests verify metrics and manifest output.
- **Rollback**: `git revert 370ee1af~5..370ee1af` restores backfill.rs and old call site.
- **Worker path unaffected**: `crates/jobs/crawl/runtime/robots.rs` is completely untouched.

## Decisions Not Taken

- **Worker-path consolidation**: `robots.rs` in `crates/jobs/crawl/runtime/` has its own backfill with different retry/backoff semantics — requires separate migration plan
- **`http_client()` singleton migration**: Would require changing `discover_sitemap_urls` simultaneously for consistency — deferred
- **`sitemap_written` vs `robots_written` key fix**: Pre-existing mismatch in status display — separate PR

## Open Questions

- Should the worker-path `robots.rs` be migrated to use the same engine function? Different retry semantics make this non-trivial.
- The `sitemap_written` / `robots_written` key mismatch in `subcommands.rs` metrics display vs worker `result_builder` — is this causing incorrect status output in production?

## Next Steps

- [ ] Migrate worker-path `crates/jobs/crawl/runtime/robots.rs` to engine function (Spider Migration 03?)
- [ ] Fix `sitemap_written` vs `robots_written` key mismatch
- [ ] Consolidate `build_client` → `http_client()` singleton across sitemap module
- [ ] Update `docs/ARCHITECTURE.md` with note about two remaining backfill paths
