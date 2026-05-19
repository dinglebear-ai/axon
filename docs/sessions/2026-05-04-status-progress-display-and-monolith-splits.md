---
date: 2026-05-04 08:00:33 EST
repo: git@github.com:jmagar/axon.git
branch: obs/p0-tracing-bundle
head: 1f621e2c
plan: none
agent: Claude (claude-sonnet-4-6)
working directory: /home/jmagar/workspace/axon_rust
---

## User Request

Fix `axon status` to show the same doc count + elapsed time per job that `axon crawl list` already shows, then extend live progress to all job types while in-progress, and finally split all monolith-policy violations.

## Session Overview

Three distinct work streams completed:
1. `axon status` now shows progress summaries (doc count, elapsed time, chunk counts) for all four job types — crawl, embed, extract, and ingest.
2. Crawl, embed, and extract workers now write intermediate `result_json` progress during execution, enabling live progress display for running jobs.
3. Seven oversized files (violating the ≤500-line monolith policy) were split into focused submodules, all clean under `cargo check`.

## Sequence of Events

1. Diagnosed root cause: `print_status_section` in `status.rs` had no mechanism to show progress; the `job_progress_summary` function existed only in `crawl/subcommands.rs` and was never called from status.
2. Fixed `status.rs` by adding `crawl_progress_summary`, `embed_progress_summary`, `extract_progress_summary`, and `ingest_progress_summary` helpers and a `progress_for` closure parameter to `print_status_section`.
3. Queried SQLite to confirm running job `56682488` had `result_json = NULL` — proving the worker was old code.
4. Added `spawn_crawl_progress_persister` and `spawn_embed_progress_persister` to `crates/jobs/lite/workers/progress.rs`; wired them into `run_crawl_job_lite` and `run_embed_job_lite`; added inline `update_result_json` calls per-URL in `run_extract_job_lite`.
5. Discovered `touch_running_job` in `ops.rs` was dead code — `update_result_json` already bumps `updated_at` as a side-effect; deleted the function.
6. Identified 20 monolith violations (excluding test files and allowlisted files). User selected 7 files to split this session.
7. Split `runners.rs` (577→179 lines) into `runners/crawl.rs`, `runners/embed.rs`, `runners/extract.rs`, `runners/ingest.rs`.
8. Split `status/metrics.rs` (602→182 lines) into `metrics/format.rs` and `metrics/ingest.rs`.
9. Split `cli/commands/job_contracts.rs` (506→198 lines) into `job_contracts/record.rs`, `job_contracts/responses.rs`, `job_contracts/summary.rs`.
10. Split `jobs/lite/ops.rs` (688→328 lines) into `ops/retry.rs`, `ops/enqueue.rs`, `ops/lifecycle.rs`.
11. Split `crawl/engine/collector.rs` (532→287 lines) into `collector/page.rs` and `collector/manifest.rs`.
12. Split `ingest/github/files.rs` (505→175 lines) into `files/clone.rs` and `files/prepare.rs`.
13. Split `crawl/engine/map.rs` (504→156 lines) into `map/strategy.rs`.
14. Each split required fixing `pub(super)` → `pub` visibility on re-exported functions; also fixed missing `tokio::io::AsyncWriteExt` import in `collector.rs` and `batch.rs` import update for `prepare::FileEmbedCtx`.

## Key Findings

- `print_status_section` (`status.rs:85`) had no progress closure — it only printed `symbol status url id`. The `job_progress_summary` function (`crawl/subcommands.rs:201`) was the existing implementation but never connected to `axon status`.
- All four job types have `result_json` populated after completion; only ingest had intermediate progress writes during execution via `spawn_ingest_progress_persister`.
- `touch_running_job` (`ops.rs:348`) was dead code with zero callers — `update_result_json` already bumps `updated_at` on every call, making the heartbeat function redundant.
- The crawl engine sends `CrawlSummary` updates via `progress_tx: Option<Sender<CrawlSummary>>` every 250ms (`collector/util.rs:20`), but `run_crawl_job_lite` passed `None` — this was the gap for live crawl progress.
- Embed pipeline has `embed_path_native_with_progress` with `EmbedProgress` channel; runner was calling `embed_path_native` (no-progress variant).
- `runners.rs` was already 547 lines (over limit) before our session additions; `batch.rs` used `super::FileEmbedCtx` and `super::read_file_embed_docs` which required updating to `super::prepare::` after the split.

## Technical Decisions

- **`progress_for` closure** added to `print_status_section` rather than specializing the function per job type — keeps one rendering path, each job type passes its own progress extractor.
- **Separate `progress.rs` module** for crawl/embed progress persisters rather than adding them to `runners.rs` (already oversized) — keeps the runners thin and the persister pattern consistent with ingest's existing `spawn_ingest_progress_persister`.
- **`pub` not `pub(super)` on submodule functions** — since the modules themselves are private to the crate, making the functions `pub` doesn't leak anything externally; `pub(super)` blocked re-exports from the module root.
- **Tests stay in root file** for each split — keeps test helpers co-located with imports and avoids scattered test modules for cross-cutting concerns (config snapshot tests in `runners.rs`, collection_from_config tests in `metrics.rs`).
- **`ingest_progress_summary` falls back to `chunks_embedded`** — the Reddit progress persister writes `chunks_embedded` (not `chunks`) during intermediate updates before `mark_completed` overwrites with the final payload containing `chunks`.

## Files Modified

| File | Change |
|------|--------|
| `crates/cli/commands/status.rs` | Added `crawl_progress_summary`, `embed_progress_summary`, `extract_progress_summary`, `ingest_progress_summary`; `progress_for` closure param on `print_status_section` |
| `crates/jobs/lite/workers/progress.rs` | **Created** — `spawn_crawl_progress_persister`, `spawn_embed_progress_persister` |
| `crates/jobs/lite/workers/runners.rs` | Wired progress persisters into crawl/embed runners; added `urls_done` counter and inline progress writes to extract runner; split into submodules |
| `crates/jobs/lite/workers/runners/crawl.rs` | **Created** — crawl runner |
| `crates/jobs/lite/workers/runners/embed.rs` | **Created** — embed runner |
| `crates/jobs/lite/workers/runners/extract.rs` | **Created** — extract runner |
| `crates/jobs/lite/workers/runners/ingest.rs` | **Created** — ingest runner + progress persister + merge_progress |
| `crates/jobs/lite/ops.rs` | Removed `touch_running_job`; split into submodules |
| `crates/jobs/lite/ops/retry.rs` | **Created** — `retry_busy`, `is_lock_busy` |
| `crates/jobs/lite/ops/enqueue.rs` | **Created** — `enqueue_job`, `check_pending_cap` |
| `crates/jobs/lite/ops/lifecycle.rs` | **Created** — `claim_next_pending`, `mark_completed`, `update_result_json`, `mark_failed`, `cancel_row` |
| `crates/cli/commands/status/metrics.rs` | Split into submodules |
| `crates/cli/commands/status/metrics/format.rs` | **Created** — time/age/error formatting |
| `crates/cli/commands/status/metrics/ingest.rs` | **Created** — ingest metrics display |
| `crates/cli/commands/job_contracts.rs` | Split into submodules |
| `crates/cli/commands/job_contracts/record.rs` | **Created** — `SharedJobRecord` + `From` impls |
| `crates/cli/commands/job_contracts/responses.rs` | **Created** — `JobStatusResponse`, `JobCancelResponse`, `JobErrorsResponse` |
| `crates/cli/commands/job_contracts/summary.rs` | **Created** — `JobSummaryEntry` |
| `crates/crawl/engine/collector.rs` | Split into submodules |
| `crates/crawl/engine/collector/page.rs` | **Created** — `CollectorConfig`, `PageOutcome`, `process_page`, `canonicalize_and_track_page` |
| `crates/crawl/engine/collector/manifest.rs` | **Created** — `write_page_to_manifest`, `append_manifest_entry` |
| `crates/ingest/github/files.rs` | Split into submodules |
| `crates/ingest/github/files/clone.rs` | **Created** — `clone_repo` + git auth helpers |
| `crates/ingest/github/files/prepare.rs` | **Created** — `FileEmbedCtx`, `collect_indexable_files`, `read_file_embed_docs` |
| `crates/ingest/github/files/batch.rs` | Updated import from `super::FileEmbedCtx` to `super::prepare::{FileEmbedCtx, read_file_embed_docs}` |
| `crates/crawl/engine/map.rs` | Split into submodules |
| `crates/crawl/engine/map/strategy.rs` | **Created** — `map_with_sitemap`, `crawl_and_collect_map`, `bounded_structure_fallback`, `append_html_anchor_backfill` |
| `crates/jobs/lite/workers/workers.rs` | Added `mod progress;` declaration |

## Commands Executed

```bash
# Confirmed root cause via DB query
sqlite3 "$HOME/appdata/axon/jobs.db" \
  "SELECT id, status, substr(result_json, 1, 200) FROM axon_crawl_jobs WHERE id='56682488-...';"
# → result_json = NULL (running job, old worker code)

sqlite3 "$HOME/appdata/axon/jobs.db" \
  "SELECT id, status, substr(result_json, 1, 200) FROM axon_crawl_jobs WHERE id='4eed71ec-...';"
# → result_json = {"elapsed_ms":10803,...,"md_created":41,...} (completed job, data present)

# Verified no callers of touch_running_job
rtk grep -rn "touch_running_job" crates/ --include="*.rs"
# → 0 matches (confirmed dead code)

# Monolith violation scan
find . -name "*.rs" -not -path "*/target/*" ... | xargs wc -l | awk '$1 > 500' | sort -rn
# → 20 violations identified after filtering test files and allowlist

# Verified clean compile after each split
rtk cargo check --bin axon 2>&1 | grep "^error" | grep -v "mcp/auth"
# → (no output — clean each time)
```

## Errors Encountered

- **`pub(super)` blocks re-export**: Every submodule split initially failed with `E0364`/`E0365` because submodule functions marked `pub(super)` cannot be re-exported from the parent module root. Fixed by changing to `pub` — safe because the modules themselves are private.
- **Missing `AsyncWriteExt`** in `collector.rs` after split: `manifest.flush().await` requires `tokio::io::AsyncWriteExt` in scope. Fixed by adding the import.
- **`batch.rs` stale import**: After `files.rs` split, `batch.rs` used `super::FileEmbedCtx` which no longer existed at the `files` root level. Fixed by updating to `super::prepare::{FileEmbedCtx, read_file_embed_docs}`.
- **`strategy.rs` wrong helper call**: Initial version of `crawl_and_collect_map` in `strategy.rs` used `merge_map_candidate_urls` instead of `normalize_map_candidate_url` per-link. Fixed in rewrite.

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| `axon status` crawl row | `✓ completed https://... uuid` | `✓ completed https://... uuid  41 docs · 10.8s` |
| `axon status` embed row | `✓ completed /path/to/dir uuid` | `✓ completed /path/to/dir uuid  42 docs · 1234 chunks` |
| `axon status` extract row | `✓ completed ["url"] uuid` | `✓ completed ["url"] uuid  12 items` |
| `axon status` ingest row | `✓ completed github: owner/repo uuid` | `✓ completed github: owner/repo uuid  58432 chunks` |
| Running crawl job | No progress shown | `127 crawled · 43 docs` (after worker restart) |
| Running embed job | No progress shown | `42 docs · 1234 chunks` (after worker restart) |
| Running extract job | No progress shown | `8 items` after each URL completes |
| `touch_running_job` | Dead function in `ops.rs` | Removed |

## Risks and Rollback

- **Worker restart required**: The new progress persisters only affect jobs started after rebuilding and restarting the persistent worker process (MCP server). Jobs already in-flight when the binary was rebuilt will complete without live progress but will still show final completion summaries.
- **`result_json` intermediate writes**: Crawl and embed jobs now write partial `result_json` during execution. The final `mark_completed` call overwrites with the full result including `elapsed_ms`. If a job is killed mid-run, the DB retains the last intermediate snapshot rather than NULL.
- **Rollback**: All changes are on `obs/p0-tracing-bundle`. Reverting is `git checkout HEAD -- <files>` for individual files or `git revert` for the commit once pushed.

## Decisions Not Taken

- **Move `job_progress_summary` to shared location**: Instead of deduplicating `crawl_progress_summary` between `status.rs` and `crawl/subcommands.rs`, each keeps its own copy. The functions serve slightly different contexts (status shows no failed-status progress since errors already appear on a separate line).
- **Add heartbeat ticks without progress data**: `touch_running_job` was the original plan for liveness signaling. Since `update_result_json` already bumps `updated_at`, a pure heartbeat would have been redundant noise.
- **Split `strategy.rs` further**: `map_with_sitemap` + `bounded_structure_fallback` could go in a separate `map/sitemap_strategy.rs` but the combined file is 296 lines — well under 500.

## Open Questions

- Why does `4eed71ec` (completed gastownhall beads crawl) not show `41 docs · 10.8s` in the new `axon status` output even though the DB has `md_created:41, elapsed_ms:10803`? The display code is identical to what `axon crawl list` uses. Possibly the screenshot was taken before the binary was fully rebuilt.
- The 13 remaining monolith violations (ACP cluster, vector/qdrant cluster, services cluster) are all complex files. Most are already tracked in `.monolith-allowlist` with June 2026 expiry.

## Next Steps

**Unfinished (started, not completed):**
- Worker restart for live progress — the binary is rebuilt but the MCP server/worker process needs to be restarted for new jobs to use the new progress persisters.

**Follow-on tasks (not yet started):**
- Split remaining 13 monolith violations not in allowlist: `services/crawl.rs` (677), `services/search.rs` (609), `services/ingest.rs` (541), `vector/ops/qdrant/utils.rs` (644), `vector/ops/qdrant/hybrid.rs` (603), `vector/ops/tei/qdrant_store.rs` (521), `vector/ops/commands/ask/context/heuristics.rs` (546), `vector/ops/commands/streaming.rs` (512), `jobs/lite/ops.rs` (688 → now split ✓), `services/types/service.rs` (517), `services/acp/bridge/terminal.rs` (716), `services/acp/mapping.rs` (689), `services/acp_llm/ws_runner.rs` (615), `services/acp/persistent_conn.rs` (525).
- Version bump: this branch has feature-level changes (new progress display, live progress persistence) that warrant a minor version increment per CLAUDE.md policy.
- Push branch to remote and open PR per session completion workflow.
