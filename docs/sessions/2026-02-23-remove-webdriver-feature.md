# Remove WebDriver Feature from Spider

**Date:** 2026-02-23
**Branch:** `fix-crawl`

## Session Overview

Removed the `webdriver` feature from spider and all associated code paths across the axon_rust codebase. The WebDriver integration was dead weight — the codebase uses Chrome via CDP (`axon-chrome` with `chrome-headless-shell`), and even the WebDriver branch in `engine.rs` still required CDP under the hood (spider uses chromiumoxide regardless). Removal simplifies the codebase, reduces compile time, and eliminates a fallback path that was never independently functional.

## Timeline

1. **Read & plan** — Reviewed all 13 files containing webdriver references to understand the dependency graph
2. **Config layer** — Removed `webdriver_url` from CLI args (`cli.rs`), Config struct + Default + Debug (`types.rs`), HOST_MAP + into_config + test (`parse.rs`)
3. **Health module** — Removed `BrowserBackendSelection` enum, `webdriver_url_from_env()`, `browser_backend_selection()`, and 2 tests from `health.rs`
4. **Engine** — Deleted the entire `else if let Some(ref wd_url) = ...` WebDriver branch from `configure_website()` in `engine.rs`
5. **CLI commands** — Removed webdriver print line from `crawl.rs`, `WebDriverFallback` variant + fallback branch from `runtime.rs`
6. **Doctor** — Rewrote `doctor.rs` removing webdriver probe, `browser_backend_label()`, webdriver params from `build_browser_runtime()`, `build_services_status()`, `gather_doctor_probes()`, and `build_doctor_report()`. Cleaned `render.rs` removing `webdriver_status_label()` and webdriver render block.
7. **Cleanup** — Removed `ChromeRuntimeMode` enum entirely (single-variant after removal, `mode` field never read). Removed `"webdriver"` from `Cargo.toml` spider features, `axon-webdriver` from HOST_MAP, env var from `.env.example`, and references from `README.md` and `crates/crawl/CLAUDE.md`.
8. **Verification** — `cargo check` (0 warnings), `cargo test --lib` (337 passed, 0 failed)

## Key Findings

- `engine.rs`: WebDriver branch at line ~230 used `spider::features::webdriver_common::{WebDriverBrowser, WebDriverConfig}` but still called `with_chrome_connection()` for CDP — confirming WebDriver was never independent
- `health.rs`: `BrowserBackendSelection` enum only had 2 variants (`Chrome`, `WebDriverFallback`); `browser_backend_selection()` was only called by `doctor.rs` — no runtime dispatch depended on it
- `runtime.rs`: `ChromeRuntimeMode::WebDriverFallback` was set when `cfg.webdriver_url.is_some()` and chrome probe failed, but the `mode` field was never pattern-matched downstream — dead code
- `navigator.webdriver` reference in `types.rs:219` is a JavaScript browser property patched by Chrome stealth mode — not related to our WebDriver feature, correctly kept

## Technical Decisions

- **Removed `ChromeRuntimeMode` enum entirely** rather than leaving a single-variant enum — the `mode` field on `ChromeBootstrapOutcome` was never read after `WebDriverFallback` removal, generating a compiler warning
- **Simplified `build_browser_runtime()`** to only take `diagnostics` param — removed `selection`, `fallback_enabled`, `fallback_ready` fields from the JSON output since they no longer carry information
- **Kept `render_optional_status_line()`** in `render.rs` — still used by chrome and openai status lines
- **Kept `redact_url` import** in `render.rs` — still used by `chrome_status_label()`

## Files Modified

| File | Change |
|------|--------|
| `Cargo.toml` | Removed `"webdriver"` from spider features |
| `crates/core/config/cli.rs` | Removed `--webdriver-url` CLI arg |
| `crates/core/config/types.rs` | Removed `webdriver_url` field, Default, Debug |
| `crates/core/config/parse.rs` | Removed HOST_MAP entry, `into_config` field, test assertion |
| `crates/core/health.rs` | Removed `BrowserBackendSelection` enum, `webdriver_url_from_env()`, `browser_backend_selection()`, 2 tests |
| `crates/crawl/engine.rs` | Deleted WebDriver branch in `configure_website()`, removed import |
| `crates/cli/commands/crawl.rs` | Removed webdriver print_option line, removed import |
| `crates/cli/commands/crawl/runtime.rs` | Removed `ChromeRuntimeMode` enum, `mode` field, WebDriver fallback branch |
| `crates/cli/commands/doctor.rs` | Rewrote — removed webdriver probe, selection logic, simplified 4 functions |
| `crates/cli/commands/doctor/render.rs` | Removed `webdriver_status_label()`, webdriver render block, selection display |
| `.env.example` | Removed `AXON_WEBDRIVER_URL` |
| `README.md` | Updated 3 sections: feature bullet, env var table, CLI flag table |
| `crates/crawl/CLAUDE.md` | Removed "Chrome CDP Wiring" section about WebDriver branch |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| `--webdriver-url` flag | Accepted, stored in Config | Removed — unknown flag error if passed |
| `AXON_WEBDRIVER_URL` env var | Read and normalized | Ignored |
| `axon doctor` JSON | Included `webdriver` service block + `selection`/`fallback_enabled`/`fallback_ready` in browser_runtime | No webdriver block; browser_runtime has only `diagnostics` |
| `axon doctor` human output | Showed webdriver status line + "selection: chrome/webdriver" | No webdriver line; no selection line |
| `axon crawl` config dump | Showed `webdriverFallbackUrl` | Removed |
| Chrome bootstrap failure | Fell back to WebDriverFallback mode if `webdriver_url` set | Always falls back to local Chrome launcher |
| Compile time | Compiled spider with webdriver feature | Skips webdriver feature — faster |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | 0 errors, 0 warnings | 0 errors, 0 warnings | PASS |
| `cargo test --lib` | All pass | 337 passed, 0 failed | PASS |
| `grep -rni webdriver crates/ .env.example Cargo.toml README.md` | Only `navigator.webdriver` in types.rs | Only `navigator.webdriver` in types.rs:219 | PASS |

## Risks and Rollback

- **Risk:** If anyone was using `AXON_WEBDRIVER_URL` or `--webdriver-url`, this is a breaking change
- **Mitigation:** The WebDriver path was never independently functional (still required CDP), so no real functionality is lost
- **Rollback:** `git checkout fix-crawl~1 -- Cargo.toml crates/ .env.example README.md`

## Decisions Not Taken

- **Keep `BrowserBackendSelection` as single-variant enum** — Rejected: no code branched on it, would just be dead weight
- **Keep `ChromeRuntimeMode` with only `Chrome` variant** — Rejected: `mode` field was never read, generated compiler warning

## Open Questions

- The `adblock` feature appeared in `Cargo.toml` spider features (wasn't in the original snapshot) — unrelated to this change, pre-existing on the branch

## Next Steps

- Run `cargo clippy` for full lint pass
- Update `CLAUDE.md` architecture section if webdriver references exist there
- Update memory files to remove webdriver-related notes
