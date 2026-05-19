# Phil Coulson - Issue Resolution Report

**Date:** 2026-02-19
**Branch:** chore/housekeeping
**Pair:** Phil Coulson + Natasha Romanoff (Pair 2 - CLI Command Issues)

## Summary

Resolved 10 issues (2 major, 8 minor) across 8 files. Focus areas: code deduplication (5 shared functions consolidated into `content.rs` and `common.rs`), security (hardcoded private IP and personal path removed), and correctness fixes (cache staleness, retry logic, fingerprint sentinels, config parsing). All changes pass `cargo check`, `cargo clippy`, `cargo fmt --check`, and `cargo test` (109 tests, 0 failures).

---

## Issues Resolved

### #87 [Minor] `commands/doctor.md` - Hardcoded private Tailscale IP

**Problem:** Documentation contained a real Tailscale IP `100.74.16.82` in TEI URL examples, leaking internal network topology.

**Fix:** Replaced all instances of `100.74.16.82` with `<TEI-HOST>` placeholder and `$TEI_URL/health` for health check examples.

**File:** `commands/doctor.md`

---

### #86 [Minor] `commands/doctor.md` - Hardcoded personal path

**Problem:** Documentation referenced `/home/jmagar/workspace/axon` -- a personal filesystem path that would not exist on other machines.

**Fix:** Replaced with `$PWD` to be environment-agnostic.

**File:** `commands/doctor.md`

---

### #8 [Major] Duplicated `extract_loc_values` across 4 files

**Problem:** `extract_loc_values` was copy-pasted in `crawl_jobs.rs`, `crawl.rs`, and `crawl_jobs/sitemap.rs`. The implementations lacked case-insensitive tag matching and `&amp;` XML entity decoding, so `<LOC>` tags and encoded URLs silently failed.

**Fix:** Consolidated into `crates/core/content.rs` as the single authoritative implementation. Added `to_ascii_lowercase()` for case-insensitive `<loc>`/`<LOC>` matching and `.replace("&amp;", "&")` for XML entity decoding. Removed all duplicate copies. Updated imports in all consuming files.

**File:** `crates/core/content.rs`, `crates/jobs/crawl_jobs.rs`, `crates/cli/commands/crawl.rs`, `crates/jobs/crawl_jobs/sitemap.rs`

---

### #20 [Major] Duplicated `normalize_prefix`, `is_excluded_url_path`, `canonicalize_url`, `extract_robots_sitemaps`

**Problem:** Four helper functions were independently implemented in `crawl_jobs.rs`, `crawl.rs`, and `crawl_jobs/sitemap.rs`. Any bug fix in one copy would not propagate to others.

**Fix:** Consolidated all four functions into `crates/core/content.rs` as public functions. Removed all duplicate copies. Updated imports in all consuming files. `crawl_jobs/sitemap.rs` re-exports `canonicalize_url` and `is_excluded_url_path` for crate-internal use.

**File:** `crates/core/content.rs`, `crates/jobs/crawl_jobs.rs`, `crates/cli/commands/crawl.rs`, `crates/jobs/crawl_jobs/sitemap.rs`

---

### #9 [Minor] Duplicated `discover_sitemap_urls_with_robots` orchestration

**Problem:** Similar sitemap discovery orchestration existed in multiple files.

**Fix:** Resolved by consolidating all 5 helper functions these orchestrators depend on (#8 and #20). The orchestration functions themselves have legitimately different return types (`RobotsDiscoveryResult` vs `SitemapDiscoveryResult`) and different caller contracts, making full deduplication inappropriate. With shared helpers consolidated, the remaining orchestration code is thin dispatch logic.

**File:** No additional changes needed beyond #8 and #20.

---

### #11 [Minor] Duplicated `stale_watchdog_payload` and `stale_watchdog_confirmed` in `crawl_jobs.rs`

**Problem:** Two watchdog helper functions were defined in both `jobs/common.rs` (private) and `crawl_jobs.rs` (local copy). The `common.rs` versions were not accessible due to visibility.

**Fix:** Changed `fn stale_watchdog_payload` and `fn stale_watchdog_confirmed` to `pub(crate) fn` in `common.rs`. Removed duplicate implementations from `crawl_jobs.rs` and updated imports to use `common::stale_watchdog_confirmed` and `common::stale_watchdog_payload`.

**File:** `crates/jobs/common.rs`, `crates/jobs/crawl_jobs.rs`

---

### #19 [Minor] `crates/cli/commands/crawl.rs` - Cache returned regardless of staleness

**Problem:** The cache hit path returned previous results without checking whether the cached data was stale. A crawl from weeks ago would be silently reused.

**Fix:** Added a 24-hour TTL check using the manifest file's mtime via `std::time::SystemTime`. If the manifest is older than 24 hours, the cache is treated as stale and a fresh crawl is triggered.

**File:** `crates/cli/commands/crawl.rs`

---

### #21 [Minor] `crates/cli/commands/crawl.rs` - Fingerprint hashes empty bytes for missing files

**Problem:** When `file_path` was empty or the file didn't exist, the code hashed empty bytes, producing a valid-looking fingerprint that could collide with other empty-file hashes.

**Fix:** Return sentinel strings: `"no-file-path"` when the path is empty, `"file-not-found"` when the file read fails. These are clearly distinguishable from real content hashes.

**File:** `crates/cli/commands/crawl.rs`

---

### #22 [Minor] `crates/cli/commands/scrape.rs` - `.max(1)` silently overrides `fetch_retries = 0`

**Problem:** `let retries = cfg.fetch_retries.max(1)` ensured at least 1 retry even when the user explicitly set `--fetch-retries 0`, violating the principle of least surprise.

**Fix:** Removed `.max(1)` so `let retries = cfg.fetch_retries` respects the user's configured value, including zero retries.

**File:** `crates/cli/commands/scrape.rs:34`

---

### #25 [Minor] `crates/core/config.rs` - `disable_by_empty` only checks first element

**Problem:** `let disable_by_empty = input.len() == 1 && matches!(input[0].trim(), "" | "/")` only triggered when the list had exactly one element. A list like `["real-prefix", ""]` would not disable, even though it contained an empty sentinel.

**Fix:** Changed to `let disable_by_empty = input.iter().any(|v| matches!(v.trim(), "" | "/"))` to check whether ANY value in the list is a disable sentinel.

**File:** `crates/core/config.rs`

---

## Validation

| Check | Result |
|-------|--------|
| `cargo check` | Pass (0 errors) |
| `cargo clippy` | Pass (0 new warnings from changed files) |
| `cargo fmt --check` | Pass (clean) |
| `cargo test` | Pass (109 tests, 0 failures) |

## Files Modified

| File | Issues |
|------|--------|
| `commands/doctor.md` | #87, #86 |
| `crates/core/content.rs` | #8, #20 |
| `crates/jobs/crawl_jobs.rs` | #8, #20, #11 |
| `crates/jobs/crawl_jobs_legacy.rs` | #8, #20, #11 (synced copy) |
| `crates/cli/commands/crawl.rs` | #8, #20, #19, #21 |
| `crates/jobs/crawl_jobs/sitemap.rs` | #8, #20 |
| `crates/jobs/common.rs` | #11 |
| `crates/cli/commands/scrape.rs` | #22 |
| `crates/core/config.rs` | #25 |

## Notes

- `crawl_jobs.rs` and `crawl_jobs_legacy.rs` are kept as identical copies (legacy dispatch pattern). All changes to `crawl_jobs.rs` were synced to `crawl_jobs_legacy.rs` via `cp`.
- The `crawl_jobs/sitemap.rs` module re-exports `canonicalize_url` and `is_excluded_url_path` from `content.rs` for crate-internal consumers. Tests in that module import directly from `content.rs`.
