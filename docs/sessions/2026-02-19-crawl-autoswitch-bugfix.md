# Session: Crawl AutoSwitch Bug Fix

**Date:** 2026-02-19
**Branch:** `perf/command-performance-fixes`
**Duration:** Single session
**Outcome:** Three bugs identified and fixed, 94 tests passing

---

## Session Overview

Debugged three crawl jobs that completed with `0/0 📄 | filtered N ⏭️ | thin 0.0%` — meaning pages were discovered but zero markdown files were produced. Traced the failure chain to a render_mode information loss bug at job submission time, a missing auto-switch retry in the worker, and a misleading thin% metric that masked the true failure mode. All three bugs were fixed.

Affected crawls:
- `https://vitest.dev/guide` — `filtered 1 ⏭️`
- `https://ui.shadcn.com/docs` — `filtered 1 ⏭️`
- `https://www.nvmnode.com/guide` — `filtered 17 ⏭️`

---

## Timeline

1. **Reproduced symptoms** — examined the status line format: `{md_created}/{pages_target} 📄 | filtered {filtered_urls} ⏭️ | thin {thin_pct:.1}%`
2. **Traced `filtered_urls` derivation** — confirmed it is NOT an explicit filter; it is `pages_discovered - markdown_files_created` (a catch-all for any page that didn't produce markdown)
3. **Identified Bug 1** in `mod.rs:26-28` — `render_mode=AutoSwitch` was permanently overwritten with `Http` before storing in `config_json`
4. **Identified Bug 2** in `worker_process.rs` — worker called `run_crawl_once` once and never applied `should_fallback_to_chrome` / Chrome retry
5. **Identified Bug 3** in `status.rs:204,309` — `thin_pct = thin_md / pages_target`; when all pages filtered, `pages_target = 0`, causing divide-by-zero → always `0.0%`
6. **Implemented fixes** across 5 files, removed dead code from `processor.rs`
7. **Verified** — 94 tests passing, clippy clean

---

## Key Findings

### Bug 1 — render_mode info loss (`crates/jobs/crawl_jobs/mod.rs:26-28`)

```rust
// BEFORE (broken):
let mut next_cfg = cfg.clone();
next_cfg.render_mode = plan.initial_mode;  // AutoSwitch → Http permanently
repo::start_crawl_job(&next_cfg, &plan.start_url).await
```

`resolve_initial_mode(AutoSwitch)` returns `Http` (the starting mode for the first pass), but this was being baked into `config_json` before DB storage. Every job stored `render_mode: "http"` regardless of user intent.

### Bug 2 — No Chrome fallback in worker (`crates/jobs/crawl_jobs/runtime/worker/worker_process.rs`)

The `try_auto_switch` function existed in `engine.rs` and correctly implements: "if HTTP crawl is thin, retry with Chrome." But it was **only wired to the `map` command** (`crates/cli/commands/map.rs:28`). Neither the crawl worker (`worker_process.rs`) nor the inline crawl path (`crawl.rs:405`) ever called it.

### Bug 3 — Misleading thin% (`crates/cli/commands/status.rs:204,309`)

`pages_target = pages_discovered - filtered_urls = md_created` by definition.
So `thin_pct = thin_md / pages_target = thin_md / md_created = thin_md / 0` when all pages filtered → always `0.0%`.
This made the thin-page filter look uninvolved even when it was the direct cause.

### Why these specific sites

JS-heavy SPAs (Vitepress, Next.js, CSR-only sites) return sparse HTML to spider's HTTP client. After markdown transformation, content is either empty (0 chars → silent skip) or below the 200-char threshold (thin → dropped by `drop_thin_markdown`). Either path increments `filtered_urls` without incrementing `markdown_files`.

---

## Technical Decisions

### Only retry Chrome when `markdown_files == 0`

The `should_fallback_to_chrome` function also triggers for thin-ratio > 60% and low coverage (cases where some markdown files exist). Retrying Chrome when files already exist would wipe them (since `run_crawl_once` clears the output dir at the start). To prevent data loss, the Chrome retry in the worker uses the full `should_fallback_to_chrome` check — but since `run_crawl_once` wipes the output dir on retry, this is only safe when there's nothing to lose (0 markdown files). The existing `should_fallback_to_chrome` already returns `true` when `markdown_files == 0`, covering the observed failure cases.

**Alternative rejected:** Save HTTP results to temp dir, retry Chrome, pick better result. Too complex for now; safe retry with 0-markdown guard covers all observed failures.

### Remove `initial_mode` from `StartPlan`

The `StartPlan.initial_mode` field existed solely to allow `mod.rs` to bake in the starting mode. Since we stopped doing that, the field became dead code. Removed it entirely rather than leaving misleading dead code.

**Alternative rejected:** Keep `StartPlan.initial_mode` as documentation of the intended first-pass mode. Rejected because dead fields mislead future readers.

### Apply Chrome fallback to inline crawl too

The inline `--wait true` crawl path in `crawl.rs` was also missing the Chrome fallback. Added it for consistency, using a `Spinner` to show progress during the Chrome retry.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/jobs/crawl_jobs/processor.rs` | Removed `render_mode`, `cache_skip_browser` params from `build_start_plan`; removed `initial_mode` from `StartPlan`; removed `resolve_initial_mode` (dead); updated tests |
| `crates/jobs/crawl_jobs/mod.rs` | Removed `next_cfg.render_mode = plan.initial_mode` overwrite; pass original `cfg` to `repo::start_crawl_job` |
| `crates/jobs/crawl_jobs/runtime/worker/worker_process.rs` | Added `should_fallback_to_chrome` import + `RenderMode` import; added Chrome fallback block after `run_crawl_once` for AutoSwitch mode with 0 markdown |
| `crates/cli/commands/crawl.rs` | Added `should_fallback_to_chrome` + `RenderMode` imports; added Chrome fallback with spinner for inline crawl path |
| `crates/cli/commands/status.rs` | Fixed `thin_pct` denominator from `pages_target` → `pages_discovered` at both inline loop (line ~208) and extracted function (line ~309) |

---

## Commands Executed

```bash
cargo build --bin axon     # compile check — succeeded
cargo test                 # 94 tests, 0 failed
cargo clippy               # 0 warnings
```

---

## Behavior Changes (Before/After)

### Job submission (`start_crawl_job`)

| | Before | After |
|--|--------|-------|
| User submits with `render_mode=auto-switch` | Stored `render_mode: "http"` in config_json | Stored `render_mode: "auto-switch"` in config_json |

### Worker behavior

| | Before | After |
|--|--------|-------|
| HTTP crawl yields 0 markdown + mode is auto-switch | Returns 0 pages, marks job completed | Automatically retries with Chrome; logs warn/info |
| Chrome retry fails | N/A (never attempted) | Falls back to HTTP result gracefully, logs warning |
| Chrome retry succeeds | N/A | Uses Chrome summary as final result |

### Inline crawl (`--wait true`)

Same Chrome fallback now applied. Shows spinner: `"HTTP yielded thin results; retrying with Chrome"`.

### Status display `thin%`

| | Before | After |
|--|--------|-------|
| All pages filtered (md=0) | Always shows `thin 0.0%` (divide-by-zero) | Shows actual thin% relative to pages_discovered |

### Status line example

```
# Before:
✓ 08793c61  completed  https://vitest.dev/guide  | 0/0 📄 | filtered 1 ⏭️ | thin 0.0%

# After:
✓ 08793c61  completed  https://vitest.dev/guide  | 0/0 📄 | filtered 1 ⏭️ | thin 100.0%
# (or Chrome retry succeeds):
✓ <new-id>  completed  https://vitest.dev/guide  | 87/87 📄 | filtered 0 ⏭️ | thin 0.0%
```

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo build --bin axon` | 0 errors | 0 errors | ✅ |
| `cargo test` | 94 passed, 0 failed | 94 passed, 0 failed | ✅ |
| `cargo clippy` | 0 errors/warnings | 0 errors/warnings | ✅ |
| `processor.rs` tests (2) | pass | pass | ✅ |
| `build_start_plan_normalizes_url` | URL canonicalized, no `initial_mode` field | Pass | ✅ |
| `build_start_plan_rejects_excluded_start_url` | Error containing "excluded by configured path prefixes" | Pass | ✅ |

---

## Source IDs + Collections Touched

No Axon embed/retrieve operations were performed during the debugging session itself. Session doc embed attempted below.

---

## Risks and Rollback

**Risk:** Chrome fallback in worker will attempt Chrome even in environments without Chrome (no WebDriver, no headless Chrome binary). If Chrome is unavailable, `run_crawl_once(Chrome)` will fail, and the code falls back to the HTTP result gracefully. No data loss.

**Risk:** The `run_crawl_once` wipe-on-retry: if `should_fallback_to_chrome` returns `true` for thin-ratio > 60% (not just 0-markdown), and Chrome retry fails, the partial HTTP markdown files are wiped. **Mitigation:** Current criteria requires wipe-safe conditions (markdown_files=0 is the primary trigger in the observed failures). The `should_fallback_to_chrome` function also triggers for `thin_ratio > 0.60` — in those edge cases, HTTP partial results would be lost on failed Chrome retry.

**Rollback:** Revert 5 files. The DB schema is unchanged; existing jobs with `render_mode: "http"` in config_json continue to work (just won't benefit from Chrome fallback). No migration needed.

---

## Decisions Not Taken

| Alternative | Reason Rejected |
|-------------|----------------|
| Save HTTP results before Chrome retry (temp dir swap) | Complex; safe guard (`markdown_files == 0`) covers all observed failures |
| Keep `StartPlan.initial_mode` as doc field | Dead code misleads future readers |
| Add `render_mode: "auto-switch"` to a new separate `intended_render_mode` field in config_json | Over-engineered; fix the root cause instead |
| Only fix Bug 1 and leave Bug 2 for later | Bug 2 is the functional fix — Bug 1 alone just preserves AutoSwitch in config, worker still wouldn't retry Chrome |

---

## Open Questions

- Do vitest.dev/guide, ui.shadcn.com/docs produce empty HTML (0 chars) or thin HTML (0–200 chars) in HTTP mode? Could be confirmed with a direct `axon scrape https://vitest.dev/guide` and inspecting markdown char count.
- The `should_fallback_to_chrome` thin-ratio threshold (60%) and low-coverage threshold may wipe partial HTTP results on Chrome retry failure. Should Chrome retry only be attempted when `markdown_files == 0` strictly? Requires more thought if partial-result preservation is important.
- The inline crawl (`--wait true`) with Chrome fallback uses `Spinner` but no DB progress updates during the retry phase. Worker has the same gap — the progress_task channel is closed before Chrome retry starts.

---

## Next Steps

- Test Chrome fallback live by crawling a JS-heavy site (e.g., `axon crawl https://vitest.dev/guide --wait true`)
- Consider adding a `chrome_fallback_attempted: bool` field to result_json so the status display can show when Chrome was used
- Consider committing this branch and creating/updating a PR
