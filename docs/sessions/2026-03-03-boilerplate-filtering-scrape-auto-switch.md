# Boilerplate Filtering + Scrape Auto-Switch

**Date:** 2026-03-03
**Branch:** `feat/sidebar`

## Session Overview

Implemented a two-part improvement to content quality in the crawl/scrape pipeline:
1. **ARIA + HTML5 boilerplate filtering** — 13 CSS selectors passed via `ignore_tags` to lol_html, stripping structural boilerplate (navigation, banners, footers, modals, iframes, noscript, hidden elements) before markdown conversion.
2. **Scrape auto-switch** — When `render_mode == AutoSwitch` and Chrome is configured, the `scrape` command now detects thin markdown output and retries with Chrome rendering, keeping the better result.

This was a multi-session effort spanning 3 context windows. The plan was defined in `/home/jmagar/.claude/plans/transient-coalescing-lantern.md`.

## Timeline

1. **Session 1**: Read target files, added `BOILERPLATE_SELECTORS` constant (9 ARIA selectors), wired `ignore_tags` into all 4 transform paths (`content.rs`, `collector.rs`, `thin_refetch.rs`, `cdp_render.rs`), added 7 tests to `content/tests.rs`, created `scrape/tests.rs` with moved tests.
2. **Session 2**: Restructured `scrape.rs` → `scrape.rs` + `scrape/tests.rs` (modern module pattern, not `mod.rs`). Added auto-switch logic: `build_chrome_scrape_website()`, `scrape_with_chrome()`, `can_auto_switch_chrome()`, thin detection in `scrape_one()` and `scrape_payload()`.
3. **Session 3 (this)**: Expanded `BOILERPLATE_SELECTORS` from 9 → 13 entries. Added `noscript`, `iframe`, `[hidden]`, `[data-nosnippet]`. Attempted `[role="banner"] ~ [role="banner"]` but lol_html doesn't support the `~` sibling combinator — removed. Added 4 new tests. All 41 content tests pass, 16 scrape tests pass.

## Key Findings

- **lol_html selector support**: Supports attribute selectors (`[role="navigation"]`), element selectors (`noscript`), but does NOT support CSS combinators (`~`, `+`, `>`). The `~` general sibling combinator in `[role="banner"] ~ [role="banner"]` caused `UnsupportedCombinator('~')` panic at `spider_transformations/content.rs:233`.
- **`TransformConfig` already has `filter_svg: true` and `filter_images: true`** (`content.rs:55-56`), so SVG/image garbage in markdown was already handled.
- **Medium is a CSR SPA** — article content doesn't exist in the HTTP response at all. No amount of HTML filtering helps. Only Chrome rendering (auto-switch) can fix this, and Chrome CDP connectivity currently returns `NoResponse` errors.
- **Spider's `main_content: true`** strips `<nav>`, `<header:first-of-type>`, `<footer>`, `<body > aside:not(:first-of-type)>` — 4 tag-based selectors. Our ARIA selectors catch the `<div role="navigation">` pattern that modern React/div-soup sites use instead.

## Technical Decisions

- **No class-based selectors**: Lesson learned from `[class*='ad']` matching Tailwind `shadow-*` classes. All selectors are either ARIA attributes (spec-defined), HTML5 elements, or explicit data attributes.
- **`[data-nosnippet]`**: Google's explicit "don't use this content" directive. Sites that mark content with this attribute are saying it's not the primary content.
- **`noscript` element stripping**: Removes tracking pixels (`<img src="tracker.gif">`) and "Please enable JavaScript" fallback text that clutters markdown output.
- **`iframe` element stripping**: Embedded ads, widgets, and third-party content. The `src` URL leaks into markdown as a bare link if not stripped.
- **Modern module pattern for scrape tests**: `scrape.rs` + `scrape/tests.rs` (not `scrape/mod.rs` + `scrape/tests.rs`). The codebase uses the modern pattern in 30+ locations vs 14 `mod.rs` files.

## Files Modified

| File | Purpose |
|------|---------|
| `crates/core/content.rs:30-47` | `BOILERPLATE_SELECTORS` constant (13 entries), `to_markdown()` wired with `ignore_tags: Some(BOILERPLATE_SELECTORS)` |
| `crates/core/content/tests.rs:341-492` | 11 new tests (7 ARIA + 4 HTML5 element filtering) |
| `crates/crawl/engine/collector.rs:73` | `ignore_tags: Some(BOILERPLATE_SELECTORS)` in `process_page()` |
| `crates/crawl/engine/thin_refetch.rs:109` | `ignore_tags: Some(BOILERPLATE_SELECTORS)` in `fetch_url_with_chrome()` |
| `crates/crawl/engine/cdp_render.rs:354` | `ignore_tags: Some(BOILERPLATE_SELECTORS)` in `render_html_with_chrome()` |
| `crates/cli/commands/scrape.rs` | Auto-switch logic: `build_chrome_scrape_website()`, `scrape_with_chrome()`, `can_auto_switch_chrome()`, thin detection in `scrape_one()` and `scrape_payload()` |
| `crates/cli/commands/scrape/tests.rs` | Moved test module (16 tests) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test -p axon content::tests` | All pass | 41 passed, 0 failed | PASS |
| `cargo test -p axon scrape::tests` | All pass | 16 passed, 0 failed | PASS |
| `cargo test --lib` | No new failures | 679 passed, 34 failed (all pre-existing DB/DNS tests) | PASS |
| `cargo clippy -p axon` | No new warnings | 1 pre-existing warning (large Err variant) | PASS |
| `cargo fmt -- --check` | Clean | Clean | PASS |

## BOILERPLATE_SELECTORS — Full List

```rust
pub const BOILERPLATE_SELECTORS: &[&str] = &[
    // ARIA landmark roles
    "[role=\"navigation\"]",    // div soup equivalent of <nav>
    "[role=\"banner\"]",        // site header/branding
    "[role=\"contentinfo\"]",   // site footer
    "[role=\"complementary\"]", // sidebar/aside
    "[role=\"search\"]",        // search forms
    "[role=\"dialog\"]",        // cookie consent, modals
    "[role=\"alertdialog\"]",   // alert modals
    "[role=\"form\"]",          // newsletter signup, login forms
    "[aria-hidden=\"true\"]",   // explicitly hidden from a11y tree
    // HTML5 elements
    "noscript",                 // tracking pixels, fallback text
    "iframe",                   // embedded ads, widgets
    // Explicit signals
    "[hidden]",                 // HTML5 hidden attribute
    "[data-nosnippet]",         // Google's "don't snippet" directive
];
```

## Risks and Rollback

- **Low risk**: All selectors are spec-defined or semantically unambiguous. No class/id substring matching.
- **`iframe` stripping**: Could theoretically remove legitimate embedded content (e.g., YouTube embeds in blog posts). Acceptable tradeoff — markdown rendering of iframes is garbage anyway (just the src URL).
- **`noscript` stripping**: Could remove legitimate fallback content on sites that support non-JS browsing. Very rare in practice.
- **Rollback**: Revert the `BOILERPLATE_SELECTORS` constant to the empty slice `&[]` or revert `ignore_tags` back to `None` in `to_markdown()`.

## Decisions Not Taken

- **Post-transform deduplication** (stripping repeated markdown lines): Adds complexity, risk of stripping intentionally repeated content.
- **Link density filtering** (blocks >60% hyperlinks = navigation): Requires a second parsing pass after markdown conversion, adds latency.
- **Class-based selectors** (`.cookie-banner`, `.newsletter-form`): Too fragile, class names vary wildly across sites.
- **Content quality scoring** (text-to-markup ratio): Complex heuristic, hard to tune without false positives.

## Open Questions

- **Chrome CDP connectivity**: `NoResponse` errors prevent live testing of the scrape auto-switch feature. The Chrome container responds to HTTP health checks (port 6000) but spider can't establish CDP WebSocket sessions for `crawl()`. Separate infrastructure issue.
- **`filter_svg: true` behavior**: Does `TransformConfig.filter_svg` strip inline SVGs or just SVG `<img>` references? Needs investigation if SVG garbage still appears in markdown.
- **lol_html combinator support**: Only attribute and element selectors work. No `~`, `+`, `>`, or descendant combinators. This limits selector expressiveness.

## Next Steps

- Fix Chrome CDP connectivity (separate issue — `NoResponse` from spider)
- Live test auto-switch with working Chrome to verify Medium/CSR SPA scraping
- Consider adding `[role="status"]` (toast notifications) and `[role="tooltip"]` if they appear in production scrapes
