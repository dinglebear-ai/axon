# Chrome CSR Crawl Investigation

**Date:** 2026-02-20
**Branch:** `perf/command-performance-fixes`

---

## Session Overview

Two-session investigation into why JS-heavy sites (vitest.dev, ui.shadcn.com) showed 95–97% thin
page rates during crawl. Root cause found and fixed in the first session: `readability: true` in
`build_transform_config()` was stripping documentation pages to just their `<title>`. Fixed in
`crates/core/content.rs`. Second session continued with the remaining CSR problem (shadcn.com),
diagnosing why even Chrome mode returns 100% thin pages there.

---

## Timeline

| Time | Activity |
|------|----------|
| Session 1 (prior) | Found `readability: true` was stripping VitePress docs to title-only |
| Session 1 (prior) | Fixed: `readability: false` in `build_transform_config()` |
| Session 1 (prior) | Wired Chrome CDP: `with_chrome_connection()` in engine.rs for both branches |
| Session 1 (prior) | Added `AXON_CHROME_REMOTE_URL`/`CHROME_URL` env vars → `100.120.242.29:9222` |
| Session 1 result | vitest.dev: 6 pages → 157 pages, 97% thin → 13% thin |
| Session 2 start | User asked: "why does it work for vitest but not shadcn?" |
| Session 2 | Diagnosed SSR vs CSR fundamental difference |
| Session 2 | Added `with_wait_for_idle_network(15s)` to Chrome/webdriver branches |
| Session 2 | Tested Chrome crawl of shadcn: pages_seen=10, markdown_files=0, thin_pages=10 |
| Session 2 | Confirmed `100.120.242.29:9222` is Browserless (not regular Chrome) |
| Session 2 | Found `scrape` command ignores `--render-mode chrome` (always plain HTTP) |
| Session 2 | Found shadcn `<main>` element contains only sidebar nav items in SSR HTML |
| Session 2 | Open: `main_content: true` treating sidebar-nav-only `<main>` as non-content |

---

## Key Findings

### Root Cause #1 — FIXED: `readability: true` stripped all doc pages
- **Location:** `crates/core/content.rs:31`
- **Before:** `readability: true` — Mozilla Readability scores VitePress/sidebar layouts as
  low-quality (no `<article>`, complex nested divs) and strips them to just the `<title>` tag
- **After:** `readability: false` — skips scoring; `main_content: true` handles structural
  extraction via `<main>`/`<article>`/`role=main` without the penalty
- **Impact:** vitest.dev: 97% thin → 13% thin, 6 → 157 markdown files

### Root Cause #2 — OPEN: shadcn.com thin despite Chrome
- **ui.shadcn.com** is Next.js CSR — static HTML has only 13 discoverable `<a href>` links
- The component sidebar (100+ links) is rendered by React client-side
- Even with Chrome mode, all 10 crawled pages returned `markdown_files=0, thin_pages=10`
- The scrape command always uses plain HTTP (`reqwest`); `--render-mode chrome` is silently ignored
- `<main>` element in shadcn SSR HTML contains only sidebar nav items (~266–294 words) —
  `main_content: true` likely identifies these as navigation and extracts just the page title
- No sitemap at `ui.shadcn.com/sitemap.xml` (returns 404)

### Browserless Endpoint
- `100.120.242.29:9222` is a **Browserless** service (Puppeteer-compatible headless Chrome)
- `/json/version` returns `webSocketDebuggerUrl: ws://100.120.242.29:9222`
- `chromey::Browser::connect_with_config` fetches `/json/version` when given an HTTP URL,
  uses returned WS URL to connect
- Browserless creates isolated sessions per CDP connection — `/json` always shows `about:blank`
  externally; spider sessions run in isolated contexts invisible to other connections
- The mass of `chromiumoxide::conn::raw_ws::parse_errors` in stderr are a CDP protocol
  incompatibility between chromiumoxide and Browserless, but do not prevent operation

### `with_wait_for_idle_network` Effect
- Added 15-second timeout in both webdriver and Chrome branches of `configure_website()`
- Crawl took ~21s for 10 pages (vs ~2s HTTP), confirming the wait IS firing
- But all 10 pages still thin — wait helps with timing, not with content extraction

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/core/content.rs:31` | `readability: true` → `readability: false` | Stop Readability from stripping doc pages to title-only |
| `crates/crawl/engine.rs:180–216` | `with_chrome_connection()` in both branches; `with_wait_for_idle_network(15s)` | Wire CDP to Browserless; wait for CSR JS hydration |
| `.env` | Added `AXON_CHROME_REMOTE_URL`, `CHROME_URL` | Point spider to Browserless at `100.120.242.29:9222` |
| `.env.example` | Added Chrome CDP env var documentation | Document new vars for future users |

---

## Commands Executed

```bash
# Verify readability fix
cargo build --bin axon

# Test vitest (HTTP, uncapped)
./scripts/axon crawl https://vitest.dev --wait true --cache false
# Result: pages_seen=181, markdown_files=157, thin_pages=24

# Inspect shadcn static HTML link count
curl -s https://ui.shadcn.com/ | python3 -c "... re.findall(<a href>)"
# Result: 13 internal links only

# Check shadcn sitemap
curl -v https://ui.shadcn.com/sitemap.xml  → HTTP 404

# Test Chrome crawl with wait_for_idle_network
RUST_LOG=info ./scripts/axon crawl https://ui.shadcn.com --render-mode chrome \
  --max-pages 10 --wait true --cache false
# Result: pages_seen=10, markdown_files=0, thin_pages=10, elapsed_ms=21534

# Check what scrape returns for shadcn
./scripts/axon scrape https://ui.shadcn.com/docs/installation
# Result: "Installation - shadcn/ui" (title only — HTTP only, ignores --render-mode)

# Inspect Browserless endpoint
curl http://100.120.242.29:9222/json/version
# webSocketDebuggerUrl: ws://100.120.242.29:9222
# Puppeteer-Version: 21.9.0  ← confirms it's Browserless
```

---

## Behavior Changes (Before / After)

| Metric | Before | After |
|--------|--------|-------|
| vitest.dev pages (HTTP crawl) | 6 pages, 97% thin | 157 pages, 13% thin |
| vitest.dev (uncapped) | ~23–29 pages | 181 pages seen, 157 with content |
| shadcn.com (HTTP) | 13 links, 100% thin | 13 links, 100% thin (unchanged) |
| shadcn.com (Chrome mode) | N/A | 10 pages, 100% thin (new behavior, not working) |
| Chrome crawl timing | N/A | ~2s/page with 15s idle-network wait |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| vitest.dev uncapped HTTP crawl | >100 pages, <30% thin | 181 seen, 157 files, 13% thin | ✅ PASS |
| shadcn Chrome crawl (10 pages) | >0 markdown files | 0 markdown files, 10 thin | ❌ FAIL |
| `cargo build --bin axon` | Clean compile | Clean | ✅ PASS |
| `cargo clippy` | 0 warnings | Not run this session | — |
| Browserless `/json/version` | WS URL in response | `ws://100.120.242.29:9222` | ✅ CONFIRMED |

---

## Source IDs + Collections Touched

Crawl embed jobs were queued during testing but not verified:
- shadcn Chrome crawl embed: job `f2fd30cd-cb2a-486d-9654-a5744e2122e3` (empty — 0 files)
- shadcn Chrome crawl embed: job `5cebb01b-c0ec-4ab2-a146-f86bbd731bbc` (empty)
- shadcn Chrome crawl embed: job `7a2a6b8e-fdf1-4966-b85d-a27c38e7d11c` (empty)
- Collection in use: `cortex` (default)

---

## Risks and Rollback

- `readability: false` affects ALL crawls globally. If any site relied on readability to strip
  nav boilerplate, it will now include more noise. Risk is low — the fix was to restore correct
  behavior for SSR doc sites.
- `with_wait_for_idle_network(15s)` adds up to 15s overhead per page in Chrome mode. For large
  Chrome crawls, total time increases proportionally.
- **Rollback:** `git revert` the `readability` change in `content.rs:31`; remove
  `with_wait_for_idle_network` calls from `engine.rs`.

---

## Decisions Not Taken

| Alternative | Rejected Because |
|-------------|-----------------|
| Keep `readability: true` with per-site override | No per-site config exists; adding it is overengineering |
| Lower `min_markdown_chars` threshold below 200 | Would pass garbage thin pages into Qdrant |
| Use `main_content: false` globally | Would flood Qdrant with nav/footer boilerplate from all sites |
| Use sitemap backfill for shadcn | shadcn has no sitemap (404) |
| Add `wait_for_dom_selector` for shadcn sidebar | Site-specific hardcoding; not generalizable |

---

## Open Questions

1. **Does Chrome actually render React content for shadcn?** We confirmed Browserless connects
   but cannot observe isolated sessions. The thin pages could mean: (a) React renders but
   `main_content: true` discards nav-heavy output, or (b) Chrome isn't rendering JS at all.
2. **`main_content: true` behavior on nav-only pages.** shadcn's `<main>` contains 266–294 sidebar
   nav words. Does `spider_transformations::main_content` correctly extract these, or does it
   classify them as navigation and return nothing?
3. **`scrape` command and Chrome.** The scrape command uses plain reqwest HTTP — `--render-mode
   chrome` is silently ignored. Should scrape support Chrome via spider's page API?
4. **Browserless CDP compatibility.** The mass of `raw_ws::parse_errors` suggest protocol
   incompatibilities. Do they cause silent failures in page capture, or just noise?
5. **`with_wait_for_idle_network` and link extraction order.** Does spider extract links BEFORE
   or AFTER the idle-network wait? If before, waiting doesn't help with link discovery.

---

## Next Steps

1. **Confirm Chrome rendering**: Navigate shadcn via Chrome DevTools MCP, inspect post-hydration
   DOM to verify link count and content availability.
2. **Investigate `main_content: true` behavior**: Test `to_markdown()` on a raw shadcn page HTML
   to see what the transform produces before the 200-char threshold check.
3. **Fix `scrape` Chrome support**: Wire spider's Chrome page API into the scrape command path.
4. **Consider `main_content: false` fallback**: For sites where `main_content: true` returns
   only a title, automatically fall back to full HTML-to-markdown conversion.
5. **Address pre-existing `cargo fmt` failures**: batch.rs, crawl.rs, audit.rs, worker_loops.rs,
   worker_process.rs, embed_jobs.rs — not introduced by this session but need cleanup.
