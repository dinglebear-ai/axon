# Session: Monolith Split — stats.rs & audit.rs

**Date:** 2026-02-21
**Branch:** `perf/command-performance-fixes`
**Duration:** ~20 min

---

## Session Overview

Performed a comprehensive monolith policy audit across the full codebase, confirmed the `.monolith-allowlist` was complete and accurate, then dispatched two parallel agents to split the two largest remaining allowlisted monoliths:

- `crates/vector/ops/stats.rs` (575 lines) → `stats/` module (4 files, all ≤231 lines)
- `crates/cli/commands/crawl/audit.rs` (502 lines) → `audit/` module (5 files, all ≤218 lines)

Both refactors were pure structural splits with zero logic changes. All 147 tests pass. Both entries pruned from `.monolith-allowlist`.

---

## Timeline

1. **Allowlist audit** — Ran full codebase scan with custom Python script against all `.rs` files (excluding `target/`). Found zero missing violations; all `target/` build artifacts (typenum, markup5ever, html5ever) are correctly excluded.
2. **Split strategy design** — Read both files before dispatching; designed natural module boundaries and wrote focused, self-contained agent prompts.
3. **Parallel dispatch** — Launched two `systems-programming:rust-pro` agents with `isolation: worktree` concurrently.
4. **Integration** — Both worktrees merged to main tree automatically. `cargo check` and `cargo test --lib` clean. Removed two entries from `.monolith-allowlist`.

---

## Key Findings

- **All source file violations were already allowlisted** — the 60 "file violations" from the initial scan were entirely `target/` build artifacts (typenum, markup5ever, html5ever, libsqlite3-sys). No source files had unlisted violations.
- **`audit.rs` already had a submodule** — `audit/audit_diff.rs` existed before this session; the split extended that pattern to `sitemap.rs`, `backfill.rs`, `manifest_audit.rs`.
- **`fetch_text_with_retry` is shared** — used by both sitemap discovery and robots backfill; placed in `audit/mod.rs` as `pub(super)` utility to avoid duplication.
- **Rust-analyzer showed false errors** — while agents' worktrees were live, rust-analyzer scanned them and reported E0364/E0603/E0616 errors. These cleared when worktrees were removed; `cargo check` was always clean.

---

## Technical Decisions

- **`stats/` split by concern**: `pg.rs` (all Postgres metric collection), `display.rs` (all terminal output), `qdrant_fetch.rs` (Qdrant HTTP queries), `mod.rs` (orchestrator only). Each file owns one concern.
- **`audit/` split by pipeline stage**: `sitemap.rs` (discovery), `backfill.rs` (fetch+write), `manifest_audit.rs` (fingerprint+snapshot), `mod.rs` (entry points + shared `fetch_text_with_retry`).
- **Parallel agents over sequential** — the two files touch completely different modules with no shared state; parallel dispatch cut wall time in half.
- **Do not edit `.monolith-allowlist` inside agents** — avoided potential write conflicts by having the orchestrator (this session) handle the pruning after both agents returned clean.

---

## Files Modified

### Deleted
| File | Lines | Reason |
|------|-------|--------|
| `crates/vector/ops/stats.rs` | 575 | Replaced by `stats/` module |
| `crates/cli/commands/crawl/audit.rs` | 502 | Replaced by `audit/` module (dir already existed) |

### Created — stats/ module
| File | Lines | Contents |
|------|-------|----------|
| `crates/vector/ops/stats/mod.rs` | 78 | `run_stats_native` (pub), `run_stats_native_impl` orchestrator |
| `crates/vector/ops/stats/pg.rs` | 231 | `PostgresMetrics`, pool/table utils, 5 `collect_*_metrics` helpers |
| `crates/vector/ops/stats/display.rs` | 226 | All `print_*` functions, `fmt_count`, `avg_stat_text` |
| `crates/vector/ops/stats/qdrant_fetch.rs` | 48 | `fetch_qdrant_snapshots` |

### Created — audit/ module
| File | Lines | Contents |
|------|-------|----------|
| `crates/cli/commands/crawl/audit/mod.rs` | 98 | Re-exports, `now_epoch_ms`, `fetch_text_with_retry`, `run_crawl_audit`, `run_crawl_audit_diff` |
| `crates/cli/commands/crawl/audit/sitemap.rs` | 218 | `SitemapDiscoveryStats/Result`, `SitemapScope`, `discover_sitemap_urls_with_robots` |
| `crates/cli/commands/crawl/audit/backfill.rs` | 102 | `RobotsBackfillStats`, `append_robots_backfill` |
| `crates/cli/commands/crawl/audit/manifest_audit.rs` | 113 | `ManifestAuditEntry`, `CrawlAuditSnapshot`, `fnv1a64_hex`, `persist_audit_snapshot` |

### Modified
| File | Change |
|------|--------|
| `.monolith-allowlist` | Removed 2 entries (`stats.rs`, `audit.rs`) |

---

## Commands Executed

```bash
# Full codebase monolith scan (custom Python, excludes target/)
python3 - <<'EOF'   # → 0 file violations, 0 fn hard violations, 16 fn warnings (source files)

# Compile + test verification
cargo check --bin axon          # → Finished dev profile, no errors
cargo test --lib                # → 147 passed; 0 failed

# Line count verification
wc -l crates/vector/ops/stats/*.rs crates/cli/commands/crawl/audit/*.rs
# All files ≤231 lines
```

---

## Behavior Changes (Before / After)

| Area | Before | After |
|------|--------|-------|
| `stats.rs` | Single 575-line flat file | `stats/` module — 4 focused files |
| `audit.rs` | Single 502-line flat file in `crawl/audit/` dir | `audit/mod.rs` + 3 new submodules |
| `.monolith-allowlist` | 7 entries | 5 entries (2 resolved) |
| Public API | `stats::run_stats_native`, `audit::discover_sitemap_urls_with_robots`, `audit::run_crawl_audit`, `super::audit::append_robots_backfill` | Identical — no caller changes |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | No errors | `Finished dev profile` | ✅ Pass |
| `cargo test --lib` | All pass | 147 passed, 0 failed | ✅ Pass |
| All split files ≤500 lines | ≤500 each | Max 231 lines | ✅ Pass |
| `.monolith-allowlist` entries | 5 remaining | 5 remaining | ✅ Pass |

---

## Source IDs + Collections Touched

| Source ID | Collection | Chunks | Status |
|-----------|-----------|--------|--------|
| `docs/sessions/2026-02-21-monolith-split-stats-audit.md` | `cortex` | 1 | ✅ Embedded + verified (query score 0.89) |

---

## Risks and Rollback

- **Risk:** Zero — pure structural refactor, no logic changes, all tests pass.
- **Rollback:** `git revert` or restore files from git history. The old flat files existed at `crates/vector/ops/stats.rs` and `crates/cli/commands/crawl/audit.rs` on the commit before this session.

---

## Decisions Not Taken

- **Single `stats/pg.rs` split into per-table files** — `collect_crawl_metrics` is the only large function (68 lines); splitting further would create 5 tiny files for marginal benefit. One `pg.rs` file is the right granularity.
- **Move `fetch_text_with_retry` to a shared HTTP utility** — it's only used within the `audit` module; promoting it to `crates/core/http.rs` would be over-engineering. Kept in `audit/mod.rs` as `pub(super)`.
- **Merge audit submodules into fewer files** — each stage of the audit pipeline (discover → backfill → fingerprint → report) is conceptually distinct; keeping them separate serves future maintainability.

---

## Open Questions

- 16 functions are in the 80–116 line warning zone (approaching the 120-line hard limit). Notably `run_map()` at 116 lines and `discover_sitemap_urls_with_robots()` at 111 lines are 4–9 lines from hard failure. Should these be addressed before they become CI blockers?

---

## Next Steps

Remaining `.monolith-allowlist` entries to refactor:

| File | Issue | Priority |
|------|-------|----------|
| `crates/jobs/crawl_jobs/runtime/worker/worker_process.rs` | `process_job()` 342 lines | High — primary refactor target |
| `crates/jobs/crawl_jobs/runtime/worker/worker_loops.rs` | `run_amqp_worker_lane()` 138 lines | High |
| `crates/crawl/engine.rs` | `run_crawl_once()` 157 lines | Medium |
| `crates/vector/ops/commands/ask.rs` | `build_ask_context` >80 lines | Low — atomic by design |
| `scripts/qdrant-quality.py` | Large Python script | Low — non-Rust, pre-existing |

Also: address the 16 function warnings before they hit CI, especially `run_map()` (116 lines) and `discover_sitemap_urls_with_robots()` (111 lines).
