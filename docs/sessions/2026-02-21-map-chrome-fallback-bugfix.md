# Session: Map Command Chrome Fallback Bug Fix

**Date:** 2026-02-21
**Branch:** `perf/command-performance-fixes`
**Duration:** ~15 minutes

---

## Session Overview

Debugged and fixed a critical performance regression in the `axon map` command. The command was triggering an unnecessary Chrome re-crawl after every successful HTTP map, causing 45+ second hangs on a command that should complete in under 5 seconds.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | User reports `axon map https://ayrloom.com/` hung for >45s after finding 146 URLs |
| +2 min | Located "HTTP map looked thin" message in `map.rs:45` |
| +4 min | Traced `should_fallback_to_chrome` logic — identified `markdown_files == 0` as always-true condition for map |
| +6 min | Confirmed `crawl_and_collect_map` never sets `markdown_files` (only `pages_seen`) |
| +8 min | Confirmed Chrome slowness: `with_wait_for_idle_network0(15s)` per page in `configure_website` |
| +10 min | Applied one-line fix: replaced `should_fallback_to_chrome(...)` with `final_summary.pages_seen == 0` |
| +12 min | `cargo check` clean, 147 tests passing |
| +14 min | User confirmed fix: `axon map https://ayrloom.com/` completed fast, 146+ URLs returned |

---

## Key Findings

1. **`crawl_and_collect_map` never sets `markdown_files`** (`engine.rs:268-289`) — it only increments `pages_seen`. The `markdown_files` field is exclusively set in `run_crawl_once` (the content crawl path).

2. **`should_fallback_to_chrome` first check** (`engine.rs:236`): `if summary.markdown_files == 0 { return true; }` — this unconditionally returned `true` for every map invocation, regardless of URLs found.

3. **Chrome mode is expensive for map**: `configure_website` (`engine.rs:208`) sets `with_wait_for_idle_network0(15s)` — 15 seconds of network-idle waiting per page. With 146 pages, this caused 45s+ hangs plus `spider::utils: mouse movement timeout exceeded` WARN spam.

4. **The HTTP map was working perfectly**: 146 URLs discovered correctly and fast. Chrome was being triggered for no reason.

5. **Malformed URL in crawled output**: `https://ayrloom.com/pages/where-to-buy-connecticut%22` — a URL-encoded double-quote (`%22`) from a malformed `href` attribute in their HTML. Not an axon bug, data quality issue on the target site.

---

## Technical Decisions

### Use `pages_seen == 0` instead of `should_fallback_to_chrome` for map

**Why not reuse `should_fallback_to_chrome`?** That function was designed for content crawls where `markdown_files` measures content richness. The map command does URL discovery only — content richness is irrelevant. The only valid reason to retry map in Chrome is if HTTP returned zero pages at all (fully JS-gated site).

**Why not add `map_mode: bool` parameter to `should_fallback_to_chrome`?** Over-engineering. The predicate for map is a one-liner. Adding a mode flag would add complexity to a shared function for a trivially simple case.

**Why not increment `markdown_files` in `crawl_and_collect_map`?** Semantically wrong — `markdown_files` should reflect content processing, not link discovery. Fixing the symptom in the wrong place.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/cli/commands/map.rs` | Replaced `should_fallback_to_chrome(&final_summary, cfg.max_pages)` with `final_summary.pages_seen == 0` on line 42 | Fix Chrome fallback always firing |
| `crates/cli/commands/map.rs` | Removed unused `should_fallback_to_chrome` import from line 6 | Clean import after logic change |

---

## Commands Executed

```bash
cargo check --bin axon
# Finished `dev` profile in 3.38s — 0 errors

cargo test --lib
# test result: ok. 147 passed; 0 failed; 0 ignored
```

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| `axon map <url>` with HTTP finding URLs | Always triggered Chrome re-crawl regardless of URL count | Chrome skipped — HTTP result used directly |
| `axon map <url>` with HTTP finding 0 URLs | Chrome re-crawl triggered | Chrome re-crawl triggered (correct behavior preserved) |
| `axon map <url>` wall-clock time (normal site) | 45s+ (Chrome re-crawl running) | <5s (HTTP only) |
| WARN log spam | `spider::utils: mouse movement timeout exceeded` printed repeatedly | No warnings |
| Explicit `--render-mode chrome` | Chrome used directly | Chrome used directly (unchanged) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | 0 errors | 0 errors, 3.38s | ✅ PASS |
| `cargo test --lib` | 147 pass, 0 fail | 147 pass, 0 fail | ✅ PASS |
| `axon map https://ayrloom.com/` | Fast, no Chrome | Fast, 146+ URLs, no Chrome | ✅ PASS (confirmed by user) |

---

## Source IDs + Collections Touched

None — this session was a bug fix, not an embed/query session.

---

## Risks and Rollback

**Risk:** A site that is partially JS-gated (returns some links via HTTP but many more via Chrome) will no longer get a Chrome retry. Previously, any site would get Chrome if HTTP `markdown_files` stayed at 0 — which was always, so this "protection" was non-functional anyway.

**Actual risk level:** Low. The pre-fix behavior was broken — Chrome always fired, never conditionally based on URL count. The new condition (`pages_seen == 0`) is strictly better: Chrome only when HTTP found nothing.

**Rollback:** Revert `map.rs` to restore `should_fallback_to_chrome` call and import. Single file, trivial.

---

## Decisions Not Taken

- **Track a `url_count` metric in `CrawlSummary` and use it in `should_fallback_to_chrome`**: Would work but adds complexity to a shared struct/function for a case that's already handled cleanly inline.
- **Add a separate `should_fallback_to_chrome_for_map` function**: Overkill for a one-liner condition.
- **Make Chrome fallback configurable via flag**: Not requested, YAGNI.

---

## Open Questions

- Should the map command have a smarter Chrome fallback threshold? E.g., retry if `pages_seen < 5`? Currently `0` is the only trigger. Open for discussion if users hit JS-gated sites that return a few decoy links via HTTP.

---

## Next Steps

- None — fix is complete, verified, and clean.
- The branch `perf/command-performance-fixes` continues accumulating performance fixes; this is one of them.
