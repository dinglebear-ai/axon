# Tony Stark - Issue Resolution Report

**Date:** 2026-02-19
**Branch:** chore/housekeeping
**Pair:** Tony Stark + Bruce Banner (Pair 1 - Rust Crate Issues)

## Summary

Resolved 9 issues (1 critical, 4 major, 4 minor) across 7 files. All changes pass `cargo check`, `cargo clippy`, `cargo fmt --check`, and `cargo test` (120 tests, 0 failures).

---

## Issues Resolved

### #2 [CRITICAL] `crates/core/content.rs` - `extract_attr` quote bug

**Problem:** After finding a pattern like `href="`, the code grabbed the first character of the VALUE as the closing delimiter instead of using the quote character from the pattern.

**Fix:** Extract `quote_char` from `pattern.chars().last()` and search for it directly in `rest`, instead of consuming the first char of the value as the delimiter.

**File:** `crates/core/content.rs:358-364`

---

### #13 [MAJOR] `mod.rs:145` - Cron scheduler dies on first error

**Problem:** `run_once()` used `?` to propagate errors, killing the entire cron scheduler on the first failure.

**Fix:** Wrapped `run_once()` in `match` with `Err` arm logging via `log_warn` and continuing the loop.

**File:** `mod.rs:145-149`

---

### #72 [MAJOR] `mod.rs:126` - `record_command_run` blocks CLI startup

**Problem:** Every command blocked up to 2s waiting for Postgres telemetry before starting.

**Fix:** Fire-and-forget with `tokio::spawn` on a cloned config. Telemetry runs in background without blocking the main command path.

**File:** `mod.rs:126-130`

---

### #48 [MAJOR] `crates/jobs/batch_jobs.rs:559` - Embed failure kills batch

**Problem:** Embedding errors aborted the entire batch job, discarding all successfully scraped results.

**Fix:** Wrapped `embed_path_native` in `if let Err(e)` with `log_warn`, making embed failures non-fatal. Scrape results are preserved and the job completes.

**File:** `crates/jobs/batch_jobs.rs:556-560`

---

### #50 [MAJOR] `crates/vector/ops.rs:663` - Duplicate candidates in `select_diverse_candidates`

**Problem:** Pass 2 re-iterated all candidates including those already selected in Pass 1, causing duplicates when `max_per_url >= 2`.

**Fix:** Added `selected_indices: HashSet<usize>` to track indices selected in Pass 1. Pass 2 skips already-selected indices via `selected_indices.contains(&i)`.

**File:** `crates/vector/ops.rs:672-701`

---

### #18 [Minor] `crates/cli/commands/common.rs:46` - Unbounded recursive expansion

**Problem:** `expand_url_glob_seed` called itself recursively with no depth limit, risking stack overflow on pathological input.

**Fix:** Introduced `MAX_EXPANSION_DEPTH = 10` constant and split into `expand_url_glob_seed` (public interface) and `expand_url_glob_seed_inner` (recursive with depth counter). Returns seed as-is when depth limit is reached.

**File:** `crates/cli/commands/common.rs:46-77`

---

### #31 [Minor] `crates/jobs/batch_jobs.rs:147` - `depth_bonus` always 0.10

**Problem:** `normalized_url.matches('/').count()` counted protocol slashes (`://`), so every URL had >= 3 slashes, making `3/12 = 0.25` always clamp to 0.10.

**Fix:** Strip the scheme by finding `://` and counting only path slashes after the authority. A root URL (`https://x.com/`) now correctly gets `depth_bonus = 1/12 ~= 0.08`.

**File:** `crates/jobs/batch_jobs.rs:147-151`

---

### #33 [Minor] `crates/vector/ops.rs:1764` - Query instructions duplicated in LLM request

**Problem:** Instructions ("Answer only from provided sources", "Cite sources") appeared in both the system message AND the context variable preamble, wasting tokens.

**Fix:** Removed instruction preamble from `context` (now starts with just `"Sources:\n..."`). Consolidated instructions into the system message. Removed redundant `"Question: "` prefix from user message.

**File:** `crates/vector/ops.rs:1764,1829-1830`

---

### #47 [Minor] `crates/crawl/engine.rs:434` - Symlink edge case in dir-clearing

**Problem:** `entry.file_type().await?` follows symlinks, reporting the TARGET's type. A symlink-to-directory would trigger `remove_dir_all`, deleting the target directory instead of just unlinking the symlink.

**Fix:** Replaced `entry.file_type()` with `tokio::fs::symlink_metadata()`. Check `is_symlink()` first (treated like files with `remove_file`), then `is_dir()` for real directories.

**File:** `crates/crawl/engine.rs:434-443`

---

## Validation

| Check | Result |
|-------|--------|
| `cargo check` | Pass (0 errors) |
| `cargo clippy` | Pass (0 new warnings from changed files) |
| `cargo fmt --check` | Pass (clean) |
| `cargo test` | Pass (120 tests, 0 failures) |

## Files Modified

| File | Issues |
|------|--------|
| `crates/core/content.rs` | #2 |
| `mod.rs` | #13, #72 |
| `crates/jobs/batch_jobs.rs` | #48, #31 |
| `crates/vector/ops.rs` | #50, #33 |
| `crates/cli/commands/common.rs` | #18 |
| `crates/crawl/engine.rs` | #47 |
