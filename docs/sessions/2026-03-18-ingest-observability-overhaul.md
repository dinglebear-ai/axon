# Ingest Pipeline Observability Overhaul — Session Log

**Date:** 2026-03-18
**Project:** axon_rust
**Branch:** main
**Base commit:** `217ae733` (v0.27.0)
**Final commit:** `a3ac1acd`

## Session Overview

Executed the 9-task "Ingest Pipeline Observability Overhaul" plan using subagent-driven development with rust-pro agents. The plan addresses GitHub ingest jobs getting stuck at near-completion (e.g., 150/155 files) due to issues/PRs pagination blocking `tokio::join!` with zero progress reporting. All 9 tasks completed, reviewed, simplified, and verified.

## Timeline

1. **Task 1+2 (parallel):** PhaseReporter foundation + Config fields (`github_max_issues`/`github_max_prs`)
2. **Task 3+4+6+7 (parallel):** GitHub issues pagination caps, file/wiki/youtube/reddit/sessions reporter threading, content-aware heartbeat, status display enrichment
3. **Task 5:** Wire all callers in `process.rs` and `services/ingest.rs` — atomic commit with Tasks 3+4
4. **Task 8:** Worker lane heartbeat upgrade (blind → content-aware)
5. **Task 9:** Integration tests for progress reporting and heartbeat
6. **Simplify review:** Fixed PR label bug, extracted `GITHUB_SUBTASK_COUNT`, replaced `strip_ansi()` with `console::strip_ansi_codes()`, combined heartbeat DB queries

## Key Findings

- **`tokio::join!` borrow semantics:** All 5 branches in `run_github_subtasks()` borrow `cfg`, `common`, `octo` by shared reference — only `reporter` and `tasks_done` (Arc<AtomicUsize>) are cloned per branch
- **Pre-existing `!Send` span in prewarm.rs:** An `EnteredSpan` held across await points blocked compilation under `-D warnings`. Fixed in commits `14b9f9ee`/`bc519aa6`/`ec3b979a`
- **Pre-existing test compilation error:** `run_single_url_extract` in extract module has wrong arg count in test binary. Unrelated to our changes — library compiles fine. Not fixed.
- **Heartbeat DB efficiency:** Reviewer flagged 2 queries per tick. Combined into single `UPDATE ... RETURNING result_json` in `touch_and_read_result_json()`

## Technical Decisions

- **No central phase enum:** Each ingest source defines its own `const PHASE_*: &str` constants locally. Avoids coupling sources to a shared type.
- **PhaseReporter wraps `Option<Sender>`:** `PhaseReporter::noop()` returns a no-op reporter for synchronous CLI paths. Zero overhead when no listener exists.
- **Content-aware heartbeat is diagnostic only:** Logs warnings after 6 unchanged intervals (3 min at 30s). Does NOT cancel jobs — that's a future enhancement.
- **`github_max_issues`/`github_max_prs` default 100, 0=unlimited:** Prevents unbounded pagination from blocking the `tokio::join!` completion.

## Files Modified

| File | Purpose |
|------|---------|
| `crates/ingest/progress.rs` | **NEW** — PhaseReporter struct + 7 unit tests |
| `crates/jobs/common/heartbeat.rs` | **NEW** — Content-aware heartbeat + blind heartbeat (moved from job_ops) + 7 tests |
| `crates/ingest/github.rs` | Extracted `GITHUB_SUBTASK_COUNT`, `run_github_subtasks()` with PhaseReporter |
| `crates/ingest/github/issues.rs` | `take_from_page()`/`should_break()` pagination helpers, max caps |
| `crates/ingest/github/files.rs` + `files/batch.rs` | PhaseReporter threading, removed old `send_progress` |
| `crates/ingest/github/wiki.rs` | PhaseReporter threading |
| `crates/ingest/youtube.rs` | PhaseReporter threading |
| `crates/ingest/reddit.rs` | PhaseReporter threading |
| `crates/ingest/sessions.rs` | PhaseReporter threading |
| `crates/jobs/ingest/process.rs` | Shared mpsc channel, all sources wired through PhaseReporter |
| `crates/services/ingest.rs` | Sync CLI paths pass `PhaseReporter::noop()` |
| `crates/jobs/worker_lane.rs` | `spawn_content_aware_heartbeat` replaces blind heartbeat |
| `crates/cli/commands/status/metrics.rs` | `build_rich_active_suffix()`, `phase_detail()`/`fetch_detail()`, 4 new tests |
| `crates/core/config/` (6 files) | `github_max_issues`/`github_max_prs` config fields, env vars, CLI args |
| `crates/web/execute/sync_mode/prewarm.rs` | Fixed `!Send` span, switched to `anyhow::Result` |
| `crates/jobs/common.rs` | Module declarations for heartbeat |
| `crates/jobs/common/job_ops.rs` | Removed moved heartbeat code |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check -p axon` | Clean | Clean | PASS |
| `cargo test --lib -p axon` | 1404 tests pass | 1404 tests pass | PASS |
| `cargo clippy -p axon -- -D warnings` | No warnings | Clean | PASS |
| `cargo fmt -p axon -- --check` | No changes | Clean | PASS |
| `cargo build --release -p axon` | Builds | Builds | PASS |

## Commits

| SHA | Description |
|-----|-------------|
| `44b0e0cc` | feat(config): add github_max_issues and github_max_prs limits |
| `4c305c9d` | feat(status): show per-task phase and progress in ingest status |
| `bc519aa6` | feat(prewarm): add tracing span for structured log correlation |
| `14b9f9ee` | fix(prewarm): eliminate silent fallbacks and error masking |
| `ec3b979a` | refactor(prewarm): switch to anyhow::Result with .context() chains |
| `61f796c7` | feat(ingest): wire PhaseReporter across all ingest sources |
| `0cf80dd9` | fix: address all 13 PR review comments from cubic-dev-ai |
| `b1eabe32` | test(ingest): integration tests for progress reporting and heartbeat |
| `a3ac1acd` | refactor: simplify review — fix PR label bug, use console strip_ansi, extract GITHUB_SUBTASK_COUNT, combine heartbeat queries |

## Risks and Rollback

- **Low risk:** All changes are additive. PhaseReporter is opt-in via function signatures. Config fields have safe defaults.
- **Rollback:** `git revert a3ac1acd..44b0e0cc` reverts all 9 commits cleanly.
- **Pre-existing extract test breakage:** Unrelated to this work but may cause confusion in CI.

## Decisions Not Taken

- **Auto-cancellation on stale heartbeat:** Deferred — diagnostic-only logging is safer for v1. Auto-cancel needs careful timeout tuning per source type.
- **Central phase enum:** Rejected in favor of local `const` strings per source. Avoids coupling and doesn't require recompilation of unrelated sources when adding new phases.
- **Hilt/Dagger DI:** Not relevant (Rust project, not Android).

## Open Questions

- Pre-existing `run_single_url_extract` test arg mismatch needs separate fix
- Should content-aware heartbeat eventually auto-cancel after N stale intervals?
- Crawl, Extract, Embed, Refresh workers still need PhaseReporter adoption (documented in plan as follow-up)

## Next Steps

- Adopt PhaseReporter in remaining workers (crawl, extract, embed, refresh)
- Consider auto-cancellation policy for content-stale jobs
- Fix pre-existing extract module test compilation issue
- Migrate remaining `Box<dyn Error>` to `anyhow::Result` across codebase
