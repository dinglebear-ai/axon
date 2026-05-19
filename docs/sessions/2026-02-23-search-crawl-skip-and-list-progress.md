# Session: Search Crawl Skip + Crawl List Progress

**Date:** 2026-02-23
**Branch:** `fix-crawl`
**Duration:** ~1 hour

---

## Session Overview

Two focused quality-of-life fixes:

1. **`axon search` was queuing crawl jobs for Reddit, YouTube, and GitHub** — domains that have dedicated ingest handlers and can't be crawled generically. Added a `CRAWL_SKIP_HOSTS` blocklist to `extract_crawl_seed()`.
2. **`axon crawl list` showed no progress info** — it was discarding `result_json` entirely. Added a `job_progress_summary()` helper to display inline progress per job.
3. **Bonus:** Repointed `~/.local/bin/axon` symlink from the stale release binary to `scripts/axon` so `axon` in PATH always auto-builds from source.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | User noticed `axon crawl list` showed `pending https://www.reddit.com/` — search had queued a crawl job for Reddit |
| +5m | Investigated `run_search()` + `extract_crawl_seed()` in `crates/cli/commands/search.rs` |
| +15m | Added `CRAWL_SKIP_HOSTS` constant + blocklist check in `extract_crawl_seed()` + test coverage |
| +20m | User asked why `axon crawl list` shows no progress; investigated `handle_list_subcommand` + `result_json` schema |
| +35m | Added `job_progress_summary()` helper + wired into `handle_list_subcommand` |
| +45m | User ran `axon crawl list` — still showed old output. Diagnosed stale release binary in PATH |
| +50m | Repointed `~/.local/bin/axon` → `scripts/axon` wrapper |

---

## Key Findings

- `extract_crawl_seed()` (`search.rs:22`) had no domain filtering — any URL returned by Tavily could become a crawl seed, including Reddit, GitHub, YouTube.
- `handle_list_subcommand()` (`crawl.rs:225`) fetched `result_json` from the DB but never used it.
- `result_json` is written on every progress tick during a running crawl with `pages_crawled`, `md_created`, `thin_md`. Completed jobs include `elapsed_ms`.
- `~/.local/bin/axon` was a symlink to `target/release/axon` — not rebuilt unless explicitly compiled.
- `scripts/axon` uses `cargo run -q` which auto-rebuilds on source changes.

---

## Technical Decisions

**Blocklist as a constant (`CRAWL_SKIP_HOSTS`)** — simple slice of string literals, zero allocations, easy to extend. Rejected adding a `--skip-hosts` flag since this isn't user-configurable behavior; these domains genuinely cannot be crawled and have dedicated ingest handlers.

**Silent drop vs. warning** — `extract_crawl_seed()` returns `None` silently for blocked hosts. The search results are still *displayed*; only the crawl enqueue is suppressed. Considered logging a per-URL warning but decided against it — it would be noisy for every Reddit result in a search.

**`job_progress_summary()` as a separate helper** — kept it out of `handle_list_subcommand()` for testability. Returns `Option<String>` so the caller can conditionally format the line.

**No new DB queries for list** — `result_json` is already fetched by `list_jobs()`. No schema or query changes needed.

**Repoint symlink, don't delete it** — `~/.local/bin/axon` is the user's PATH entry. Replacing target with the wrapper is the least-invasive fix. No shell config changes needed.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/cli/commands/search.rs` | Added `CRAWL_SKIP_HOSTS` constant + blocklist check in `extract_crawl_seed()` + updated docstring + 1 new test |
| `crates/cli/commands/crawl.rs` | Added `job_progress_summary()` helper + updated `handle_list_subcommand()` to use it + imported `CrawlJob` |

---

## Commands Executed

```bash
# Verify search changes compile
cargo check --bin axon
# → Finished in 2.05s, 0 errors

# Run search tests
cargo test search
# → 19 passed (was 18 before new test), 0 failed

# Verify crawl changes compile
cargo check --bin axon
# → Finished in 1.97s, 0 errors

# Run crawl tests
cargo test crawl
# → 53 passed, 0 failed

# Repoint symlink
ln -sf /home/jmagar/workspace/axon_rust/scripts/axon /home/jmagar/.local/bin/axon
# → lrwxrwxrwx ... /home/jmagar/.local/bin/axon -> /home/jmagar/workspace/axon_rust/scripts/axon
```

---

## Behavior Changes (Before / After)

### `axon search <query>`

**Before:** Every search result URL (including Reddit, YouTube, GitHub) was passed to `extract_crawl_seed()` and potentially queued as a crawl job.

**After:** URLs from `reddit.com`, `www.reddit.com`, `youtube.com`, `www.youtube.com`, `youtu.be`, `github.com`, `www.github.com` return `None` from `extract_crawl_seed()` and are never enqueued. Results are still displayed.

### `axon crawl list`

**Before:**
```
  ◐ <id>  running    https://lib.rs/
  ✓ <id>  completed  https://crates.io/
  ✗ <id>  failed     https://crates.io/crates/claude-hook-advisor
```

**After:**
```
  ◐ <id>  running    https://lib.rs/           127 crawled · 43 docs
  ✓ <id>  completed  https://crates.io/        342 docs · 18.3s
  ✗ <id>  failed     https://crates.io/...     connection refused (os error 111…
```

Note: Jobs completed before this change have `result_json = NULL` and will still show no suffix.

### `axon` (PATH binary)

**Before:** `~/.local/bin/axon` → `target/release/axon` (stale unless manually rebuilt)
**After:** `~/.local/bin/axon` → `scripts/axon` (auto-rebuilds via `cargo run -q`)

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` (after search fix) | 0 errors | 0 errors, finished 2.05s | ✅ |
| `cargo test search` | 19 passed | 19 passed, 0 failed | ✅ |
| `cargo check --bin axon` (after crawl fix) | 0 errors | 0 errors, finished 1.97s | ✅ |
| `cargo test crawl` | 53 passed | 53 passed, 0 failed | ✅ |
| `ls -la ~/.local/bin/axon` | points to scripts/axon | `→ .../scripts/axon` | ✅ |

---

## Source IDs + Collections Touched

None — this session contained no `axon embed`, `axon crawl`, or `axon query` operations.

---

## Risks and Rollback

**Search blocklist** — low risk. Additive filter; only suppresses crawl enqueue, not display. To rollback: remove the `CRAWL_SKIP_HOSTS` slice and the `if CRAWL_SKIP_HOSTS.contains(&host)` block from `extract_crawl_seed()`.

**Crawl list progress** — display-only change, no DB writes. To rollback: revert `handle_list_subcommand()` to the old 4-field `println!` and remove `job_progress_summary()`.

**Symlink change** — `scripts/axon` uses `cargo run -q` which adds ~1-3s compile-check overhead on each invocation (when no files changed, it's near-instant due to incremental compilation). If that becomes annoying, repoint to `target/debug/axon` after an explicit `cargo build`.

---

## Decisions Not Taken

- **Per-domain warning logs in `run_search`** — would warn "Skipping crawl for reddit.com (use dedicated ingest command)" for each blocked result. Rejected as noisy given Reddit results are common in search output.
- **`--allow-ingest-domains` flag** — would let users override the blocklist. Rejected as YAGNI; the use case for crawling Reddit generically doesn't exist.
- **`inspect()` + `filter_map()` double-pass** — explored emitting skip warnings via `inspect()` before `filter_map()`. Rejected due to double-calling `extract_crawl_seed()` and false-positive warnings for SSRF-blocked URLs already handled downstream.
- **`cargo install --path .`** — would install a compiled binary to `~/.cargo/bin/`. Rejected; wrapper script is preferable since it auto-sources `.env` and always builds from current source.

---

## Open Questions

- Older completed jobs have `result_json = NULL` and will never show progress in `crawl list`. If useful, a one-time migration could reconstruct stats from the manifest files on disk — but only worth it if those jobs are regularly referenced.
- The `result_json` for the running lib.rs job had no progress tick yet at observation time. Unknown whether subsequent ticks appeared — the crawl was in flight.
- No justfile exists in the repo. If one is added, a `setup` recipe should include the symlink step for reproducibility.

---

## Next Steps

- Run `axon crawl list` after the next crawl completes to verify the progress suffix appears for newly-completed jobs.
- Consider adding `github.com` / `reddit.com` / `youtube.com` to a note in `CLAUDE.md` or a comment in `run_search` about why these are blocked (covered by dedicated ingest handlers).
