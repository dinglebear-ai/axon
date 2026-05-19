# Session: Spider Feature Flags Audit + Inline Chrome AutoSwitch Implementation

**Date:** 2026-03-03
**Branch:** feat/sidebar
**Duration:** ~4 hours

---

## Session Overview

Two major workstreams:

1. **Spider.rs feature flag audit** — catalogued all 78 spider feature flags, identified which 11 are actually used in axon_rust, and answered 4 deep follow-up questions about the architecture (Tavily routing, cfg(feature) gates, storage stack, AutoSwitch vs smart flag).

2. **Inline Chrome AutoSwitch implementation** — replaced the batch "re-crawl everything" fallback with a per-page concurrent Chrome refetch that fires inline while the HTTP crawl is still running, reusing already-fetched HTML via CDP `Page.setContent()` — matching spider's `smart` flag behavior.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Created `docs/spider-feature-flags.md` — 78 flags across 9 categories |
| +20m | Dispatched 4 parallel agents to find every flag in use → 9 spider + 2 spider_agent |
| +40m | Updated doc with full audit: which flags enabled, where used in source, why others unused |
| +60m | User follow-up questions → dispatched 4 more parallel agents to research each |
| +90m | Agent implemented first (broken) version — still batch, not per-page |
| +120m | Relaunched with correct spec: per-page concurrent Chrome via `Page.setContent()` |
| +180m | Implementation complete; rust-reviewer caught 3 CI violations + 2 pre-existing clippy failures |
| +200m | rust-reviewer implemented all fixes; lint gate clean, 683 tests passing |
| +220m | Proof testing: chakra-ui.com (252 pages, 4% thin), framer.com (721+ pages, 0% thin), tanstack.com forced demo |
| +240m | Postgres duration storage discussion; switched to Postgres MCP |

---

## Key Findings

### Spider Feature Flag Audit

- **78 total flags** in spider.rs across 9 categories
- **11 in use**: 9 via `spider` crate, 2 via `spider_agent` crate
- **Zero `#[cfg(feature)]` gates** in axon_rust source — all feature selection is at Cargo.toml level (correct for a binary-first crate with no `[features]` section)
- `spider` crate has NO `search_tavily` feature — that only exists in `spider_agent`; using `spider_agent` is the correct/only approach
- Spider's `cache_mem` is in-memory HTTP dedup within a single crawl run — NOT persistent storage
- **Storage stack**: markdown files on disk (`output_dir/markdown/*.md`) + Qdrant vectors (searchable) + Postgres (job metadata only, no document content)

### AutoSwitch vs spider's `smart` Flag

| | axon old AutoSwitch | spider `smart` | axon new AutoSwitch |
|---|---|---|---|
| Decision scope | Batch (post-crawl) | Per-page (real-time) | Per-page (real-time) ✅ |
| Fallback action | Full re-crawl | Selective re-render | Selective re-render ✅ |
| HTTP content reuse | No (re-fetches) | Yes (Page.setContent) | Yes (Page.setContent) ✅ |
| Chrome failure | Loses HTTP result | Unknown | Keeps HTTP result ✅ |

### Proof of Inline Chrome Firing (tanstack.com)

```
03:28:34.717  HTTP fetches begin
03:28:34.769  thin_refetch: inline Chrome render spawned for .../browser/why
03:28:34.822  thin_refetch: inline Chrome render spawned for .../advanced/pool
... (8 total, fired 52ms after HTTP started)
03:28:35.138  thin_refetch: waiting for 9 in-flight Chrome render(s) to complete
              ↑ crawl loop still running when Chrome tasks were live in JoinSet
pages_seen=10  markdown_files=2  thin_pages=8  (HTTP-only: markdown_files=0  thin_pages=10)
```

Chrome recovered 2 pages via `Page.setContent()` — no second network request.

---

## Technical Decisions

### `Page.setContent()` instead of Chrome navigation
Reuses already-fetched HTTP bytes. Chrome executes the JS without making a second HTTP GET. Matches exactly how spider's `smart` flag works internally.

### Bounded concurrency: 4 Chrome tabs max
`THIN_REFETCH_CONCURRENCY = 4` semaphore in `collector.rs`. Prevents opening hundreds of Chrome tabs on large crawls.

### Batch fallback preserved
If `drop_thin_markdown = false` (thin URLs not tracked), or Chrome isn't available at crawl time, the original post-crawl batch refetch remains as a safety net.

### Binary-first, no `[features]` section
Zero `#[cfg(feature)]` gates is correct for this project — it's a binary crate that commits to all spider features unconditionally. `#[cfg]` guards are for library crates exposing optional APIs to downstream consumers.

### Two duration metrics, not one
- `result_json->>'elapsed_ms'` = spider crawl phase only (internal timer)
- `finished_at - started_at` = full job lifecycle (queue wait + crawl + embed dispatch + DB)
- These are complementary, not contradictory.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `docs/spider-feature-flags.md` | Created | Full 78-flag audit with usage status |
| `crates/crawl/engine/cdp_render.rs` | Created (~419L) | Raw CDP WebSocket helpers + `render_html_with_chrome()` via `Page.setContent()` |
| `crates/crawl/engine/thin_refetch.rs` | Modified | Added `Arc<Config>` pattern, post-write counter increment, re-exported `render_html_with_chrome` |
| `crates/crawl/engine/collector.rs` | Modified | Inline Chrome spawn in broadcast recv loop; `process_page()` and `write_page_to_manifest()` extracted; semaphore acquire fixed |
| `crates/crawl/engine.rs` | Modified | `CrawlSummary.thin_urls: HashSet<String>`; passes `chrome_ws_url` to `CollectorConfig` for AutoSwitch |
| `crates/cli/commands/crawl/sync_crawl.rs` | Modified | `maybe_chrome_fallback()` uses surgical path first; `run_sync_crawl` split to stay under 120L; `.expect()` → `ok_or_else` |
| `crates/vector/ops/commands/streaming.rs` | Pre-existing fixes | `judge_llm_non_streaming` moved before test module; `unnecessary_unwrap` fixed |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo fmt --check` | Clean | Clean | ✅ |
| `cargo clippy -D warnings` | 0 warnings | 0 warnings | ✅ |
| `cargo test --lib` | All pass | 683 passed, 0 failed | ✅ |
| `python3 scripts/enforce_monoliths.py` | No violations | Clean | ✅ |
| HTTP crawl tanstack.com `--min-markdown-chars 30000` | High thin count | 10/10 thin | ✅ |
| AutoSwitch tanstack.com `--min-markdown-chars 30000` | Inline Chrome fires | 9 spawns during crawl, 2 recovered | ✅ |
| chakra-ui.com full crawl | Completes | 252/252 pages, 4% thin | ✅ |
| framer.com full crawl | Completes | 721+ pages crawled, 0% thin | ✅ |

---

## Behavior Changes (Before/After)

**Before:** AutoSwitch re-crawled the entire site with Chrome if thin ratio exceeded threshold — all good HTTP pages discarded, full second network pass.

**After:** AutoSwitch spawns Chrome renders inline per thin page while HTTP crawl is still running, passing already-fetched HTML via `Page.setContent()`. Good HTTP pages kept. Chrome only touches thin pages. If Chrome unavailable, gracefully falls back to HTTP result.

---

## Risks and Rollback

- **Risk:** `cdp_render.rs` uses raw CDP WebSocket — tightly coupled to Chrome's wire protocol. If Chrome API changes, this breaks silently (no typed client).
- **Risk:** `Page.setContent()` on a page with absolute resource URLs (fonts, scripts from CDN) may behave differently than navigating to the URL — some JS may fail to load cross-origin resources.
- **Rollback:** Set `--render-mode http` or `--render-mode chrome` to bypass AutoSwitch entirely. The batch fallback path is still present for `drop_thin_markdown=false` cases.

---

## Decisions Not Taken

- **Enable spider's `smart` feature flag** — would require feature-gating inside spider's internal page loop; we have no hook into spider's per-page rendering decisions from the outside. Building our own gave us control over thresholds, logging, and graceful degradation.
- **Re-navigate to URL with Chrome** — simpler to implement but makes a second network request. `Page.setContent()` reuses the HTTP bytes already in memory.
- **`#[cfg(feature)]` guards in source** — unnecessary for a binary-first crate. Feature commitment belongs in Cargo.toml.

---

## Open Questions

- `Page.setContent()` behavior on SPA pages with CDN-loaded JS bundles — if the script src is relative or requires same-origin cookies, Chrome may not fully execute the app even after `setContent`. Needs testing on a true shell SPA (authenticated app like Linear/Notion).
- Most public doc sites (chakra-ui, framer, tanstack) are SSR/SSG — thin pages are genuinely rare at the default 200-char threshold. The inline Chrome path is armed but rarely fires in practice on these targets.

---

## Next Steps

- Test on a true CSR-only SPA (authenticated app, empty `<div id="root">` on HTTP) to validate full content recovery
- Consider adding a `thin_rescued` counter to `CrawlSummary` and surfacing it in the web UI
- `streaming.rs` pre-existing `Box<dyn Error>` violations in internal ops module should be cleaned up in a separate pass (anyhow::Result)
- Off-by-one in `check_sources_repetition` overlap window: `saturating_sub(10)` should be `saturating_sub(11)` for 11-byte needle `\n## sources`
