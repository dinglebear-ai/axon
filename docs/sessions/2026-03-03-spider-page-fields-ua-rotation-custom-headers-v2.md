# Session: Custom Headers Consistency + Extract Engine Refactor
**Date:** 2026-03-03
**Branch:** `feat/sidebar`
**Continuation of:** `2026-03-03-spider-page-fields-ua-rotation-custom-headers.md`

## Session Overview

Tightened `custom_headers` (`--header`) coverage across all `Website::new()` call sites that bypass `configure_website()`. The previous session added `--header` support to `configure_website()` (crawl engine), but three independent Website creation paths — scrape, Chrome thin-page re-fetch, and extract engine — silently dropped the headers. This session wired `custom_headers` into all three, refactored the extract engine's 7-param function into an `ExtractWebConfig` struct, and updated all relevant CLAUDE.md files.

## Timeline

1. **Resumed from context compaction** — previous session identified 3 `Website::new()` bypass sites
2. **Read all 3 files** — `scrape.rs`, `thin_refetch.rs`, `content/engine.rs`
3. **Wired `custom_headers` into `scrape.rs:build_scrape_website()`** — 14-line block after proxy wiring
4. **Wired `custom_headers` into `thin_refetch.rs:build_single_page_website()`** — same 14-line block
5. **First verification** — `cargo check` + `cargo clippy` + `cargo test --lib` all green (683 tests)
6. **User requested extract engine refactor** — "ok do this and then check the CLAUDE.md"
7. **Introduced `ExtractWebConfig` struct** in `content/engine.rs` — replaces 7 loose params
8. **Refactored `run_extract_with_engine()`** — takes `(ExtractWebConfig, Arc<Engine>)` instead of 7 params
9. **Updated both callers** — `extract.rs` (CLI) and `extract/worker.rs` (AMQP worker)
10. **Updated re-export** in `content.rs` — added `ExtractWebConfig` to pub use
11. **Second verification** — all gates green again (683 tests, clippy clean, monolith pass)
12. **Updated CLAUDE.md files** — root, `crates/core/`, `crates/crawl/`

## Key Findings

- **`scrape.rs:21-63`** (`build_scrape_website`) — had UA override + proxy but no `custom_headers`. Fix: added 14-line `with_headers()` block at line 48.
- **`thin_refetch.rs:32-58`** (`build_single_page_website`) — same gap. Fix: added same block at line 48.
- **`content/engine.rs:146-205`** (`run_extract_with_engine`) — bare `Website::new()` with only `with_limit` + SSRF blacklist. No UA, no proxy, no headers. Required signature refactor to add headers cleanly.
- Both callers of `run_extract_with_engine` (`extract.rs:186`, `worker.rs:147`) already clone all strings into owned data for async spawning — moving to an owned-string config struct was a natural fit.
- `content/engine.rs` is 220 lines after changes — well within 500-line monolith limit.

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| `ExtractWebConfig` with owned `String` fields (not `&str` refs) | Both callers clone strings for async blocks anyway; owned struct avoids lifetime complexity and moves cleanly into `tokio::spawn` |
| Kept `engine: Arc<DeterministicExtractionEngine>` as separate param | Engine is shared state across multiple extract runs; bundling it into config would force unnecessary cloning |
| Same 14-line `with_headers()` block in all 4 sites (not extracted to helper) | Each site has slightly different context; a shared helper would need to take `&Config` which extract engine doesn't have. Three similar blocks is better than a premature abstraction. |
| Did not add UA/proxy to `content/engine.rs` | Extract engine is a simpler path (HTTP-only, no Chrome). Custom headers cover the auth use case. UA rotation and proxy are crawl-specific concerns. |

## Files Modified

| File | Change |
|------|--------|
| `crates/cli/commands/scrape.rs:48-62` | Added `custom_headers` → `HeaderMap` wiring in `build_scrape_website()` |
| `crates/crawl/engine/thin_refetch.rs:48-62` | Added `custom_headers` → `HeaderMap` wiring in `build_single_page_website()` |
| `crates/core/content/engine.rs:16-28` | New `ExtractWebConfig` struct definition |
| `crates/core/content/engine.rs:160-220` | Refactored `run_extract_with_engine()` to take `ExtractWebConfig`; added `custom_headers` wiring |
| `crates/core/content.rs:11` | Re-exported `ExtractWebConfig` |
| `crates/cli/commands/extract.rs:6,178-197` | Updated import + caller to construct `ExtractWebConfig` |
| `crates/jobs/extract.rs:5` | Updated import to include `ExtractWebConfig` |
| `crates/jobs/extract/worker.rs:139-158` | Updated caller to construct `ExtractWebConfig` |
| `CLAUDE.md:112` | Added `--header` flag to Crawl & Scrape global flags table |
| `crates/core/CLAUDE.md:36,65` | Updated `engine.rs` description + added `custom_headers` to Spider tuning field group |
| `crates/crawl/CLAUDE.md:34` | Updated independent-path note to list all 3 bypass sites with `custom_headers` sync reminder |

## Commands Executed

| Command | Result |
|---------|--------|
| `cargo check --all-targets` | Clean (both rounds) |
| `cargo clippy --all-targets -- -D warnings` | Clean (both rounds) |
| `cargo test --lib` | 683 passed, 0 failed, 3 ignored (both rounds) |
| `python3 scripts/enforce_monoliths.py --staged` | Passed |

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| `--header` on `axon scrape` | Headers silently dropped | Headers applied to single-page scrape |
| `--header` on Chrome thin-page re-fetch | Headers silently dropped | Headers applied to Chrome re-fetch |
| `--header` on `axon extract` | Headers silently dropped | Headers applied to extract crawl |
| `run_extract_with_engine` signature | 7 loose params (`&str`, `&str`, `u32`, `&str`, `&str`, `&str`, `Arc<Engine>`) | 2 params (`ExtractWebConfig`, `Arc<Engine>`) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --all-targets` | No errors | Clean compile | PASS |
| `cargo clippy -D warnings` | No warnings | Clean | PASS |
| `cargo test --lib` | All pass | 683 passed, 0 failed | PASS |
| `enforce_monoliths.py --staged` | Pass | "Monolith policy check passed." | PASS |

## Risks and Rollback

- **`ExtractWebConfig` is a public API change** — any external consumers of `run_extract_with_engine` (none currently; only 2 internal callers) would break. Rollback: revert `content/engine.rs` + both callers.
- **Custom headers on extract could send auth tokens to arbitrary URLs** — same risk as crawl; mitigated by SSRF validation running before any network activity.

## Decisions Not Taken

- **Shared `apply_custom_headers(website, headers)` helper** — Rejected: would need to live in a place importable by both `crates/cli` and `crates/crawl` and `crates/core`; three identical 14-line blocks are simpler than a cross-crate helper for a pattern used in exactly 4 places.
- **Adding UA/proxy to extract engine** — Rejected: extract is HTTP-only, UA rotation is implicit via `ua_generator` feature, and proxy is a crawl/scrape concern. Custom headers cover the auth use case that motivated this work.

## Open Questions

- Does spider's `ua_generator` feature apply to `Website::new()` + `crawl_raw()` in the extract engine? The feature is compiled in globally, so `get_ua()` should return random UAs everywhere — but this is untested for the extract path specifically.

## Next Steps

- `docs/spider-feature-flags.md` still needs updating to reflect the 4 newly enabled features (`ua_generator`, `headers`, `glob`, `time`) from the previous session
- Manual test: `axon scrape https://example.com --header "X-Test: axon"` — verify headers pass through
- Manual test: `axon extract https://example.com --header "Authorization: Bearer test"` — verify headers reach spider
