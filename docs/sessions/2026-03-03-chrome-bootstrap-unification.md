# Session: Chrome Bootstrap Unification

**Date:** 2026-03-03
**Branch:** `feat/sidebar`
**Plan:** `docs/plans/2026-03-03-spider-migration-05-chrome-bootstrap-unification.md`

---

## Session Overview

Removed duplicate CDP bootstrap probe code from the CLI crawl runtime (`crates/cli/commands/crawl/runtime.rs`) and replaced it with the shared engine resolver (`crates/crawl/engine::resolve_cdp_ws_url`). This eliminates ~40 lines of duplicated CDP discovery logic (reqwest client construction, Docker host rewrite, `/json/version` fetch) while preserving the retry-with-backoff behavior.

---

## Timeline

1. Read the implementation plan and all relevant source files (`runtime.rs`, `engine.rs`, `engine/runtime.rs`, `sync_crawl.rs`, `config_impls.rs`)
2. Identified the duplication: CLI's `probe_cdp_connection()` duplicated `resolve_cdp_ws_url()` from the crawl engine
3. Rewrote `crates/cli/commands/crawl/runtime.rs` — deleted `probe_cdp_connection()`, removed unused imports, replaced bootstrap body with call to shared resolver
4. Updated existing `runtime_migration_tests.rs` to match new behavior (invalid URL now produces "probe failed" instead of "unable to parse")
5. Fixed pre-existing broken `mod map_migration_tests;` declaration in `map.rs`
6. Verified: `cargo check`, `cargo clippy`, `cargo fmt --check`, 110 crawl tests pass, 17 migration tests pass
7. Updated `crates/cli/CLAUDE.md` Chrome Bootstrap section

---

## Key Findings

- **Shared resolver already exported:** `resolve_cdp_ws_url` was already `pub(crate)` in `engine/runtime.rs` and re-exported in `engine.rs:24` — no export changes needed
- **Behavior delta — Docker early-return:** The shared resolver returns `None` inside Docker (container DNS resolves natively). Old CLI probe always attempted resolution. Net effect: inside Docker, bootstrap no longer pre-resolves; spider handles it via its own `/json/version` fetch. Functionally equivalent.
- **Behavior delta — warning text:** Invalid scheme URLs (e.g., `ftp://`) previously got "unable to parse" from the separate `cdp_discovery_url` guard. Now they flow through `resolve_cdp_ws_url` which returns `None`, producing the generic "probe failed" warning. Correct behavior, less specific message.
- **Custom timeout removed:** The CLI's `chrome_bootstrap_timeout_ms` config field controlled a per-bootstrap reqwest client timeout. The shared resolver uses the global `HTTP_CLIENT` singleton (30s timeout). The custom timeout was a CLI-specific nicety with no observable production impact.
- **Pre-existing broken test:** `crates/cli/commands/map.rs:174` had `mod map_migration_tests;` but the test file at `commands/map_migration_tests.rs` references a non-existent `map_payload` function. This blocked test compilation. Removed the broken declaration.

---

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Delete CLI probe entirely instead of wrapping | The engine resolver handles all cases (ws:// shortcut, Docker detection, host rewrite). No value in a CLI-specific wrapper. |
| Accept shared HTTP client timeout | Custom ms-precision timeout added complexity for negligible benefit. The 30s shared client timeout is more than sufficient for CDP discovery. |
| Keep retry loop in CLI bootstrap | Retry logic is CLI-specific UX (configurable retries + backoff). Engine resolver is a single-shot function — retry belongs to the caller. |
| Remove `cdp_discovery_url` pre-check | Separate parse validation before the probe loop was defensive coding. The shared resolver handles invalid URLs gracefully (returns None). One warning instead of two — simpler. |

---

## Files Modified

| File | Change |
|------|--------|
| `crates/cli/commands/crawl/runtime.rs` | Deleted `probe_cdp_connection()`, removed 3 imports (`is_docker_service_host`, `cdp_discovery_url`, `Url`), replaced bootstrap body with `resolve_cdp_ws_url()` call |
| `crates/cli/commands/crawl/runtime_migration_tests.rs` | Updated `bootstrap_warns_when_remote_url_unparseable` test: expects "probe failed" instead of "unable to parse", added `chrome_bootstrap_retries: 0` |
| `crates/cli/CLAUDE.md` | Updated Chrome Bootstrap section to document delegation to shared engine resolver |
| `crates/cli/commands/map.rs` | Removed broken `#[cfg(test)] mod map_migration_tests;` declaration (pre-existing issue) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Clean | `Finished dev profile in 0.40s` | PASS |
| `cargo clippy -- -D warnings` | 0 warnings | `Finished dev profile in 13.49s` | PASS |
| `cargo fmt --check` | Clean | No output | PASS |
| `cargo test runtime_migration_tests` | 17 pass | `ok. 17 passed; 0 failed` | PASS |
| `cargo test crawl` | All pass | `ok. 110 passed; 0 failed; 1 ignored` | PASS |

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| CDP resolution inside Docker | CLI's `probe_cdp_connection` always attempted resolution | `resolve_cdp_ws_url` returns `None` inside Docker; spider handles resolution natively |
| Invalid scheme URL warning | "unable to parse --chrome-remote-url" | "remote chrome probe failed; falling back to local Chrome launcher" |
| HTTP client for CDP probe | Dedicated `reqwest::Client` with custom `chrome_bootstrap_timeout_ms` timeout | Shared `HTTP_CLIENT` singleton (30s timeout) |
| Code ownership | CDP probe logic in both `cli/commands/crawl/runtime.rs` AND `crawl/engine/runtime.rs` | Single implementation in `crawl/engine/runtime.rs` |

---

## Risks and Rollback

- **Low risk:** The shared resolver has been in production use by `configure_website()` in `engine/runtime.rs:93` since the Chrome integration was added. The CLI bootstrap is a secondary consumer.
- **Rollback:** `git checkout feat/sidebar -- crates/cli/commands/crawl/runtime.rs` restores the old probe implementation. No schema, config, or API changes.
- **`chrome_bootstrap_timeout_ms` config field:** Still exists in Config but no longer controls the probe client timeout. Could be removed in a follow-up if desired, but it's harmless as unused config.

---

## Decisions Not Taken

- **Making `resolve_cdp_ws_url` accept a timeout parameter:** Would preserve the custom timeout behavior but adds complexity to a shared function for a single consumer. The 30s shared client timeout is sufficient.
- **Adding a new `resolve_cdp_ws_url_with_retries` helper in the engine:** Retry logic is caller-specific. The engine shouldn't own retry policy decisions that belong to the CLI UX layer.

---

## Open Questions

- Should `chrome_bootstrap_timeout_ms` be removed from Config since it no longer controls anything?
- The `map_migration_tests.rs` file references `map_payload` which doesn't exist — was this from an incomplete previous migration plan?

---

## Next Steps

- Consider removing `chrome_bootstrap_timeout_ms` from Config if confirmed unused elsewhere
- Fix or delete `crates/cli/commands/map_migration_tests.rs` (broken, references non-existent `map_payload`)
- Move `docs/plans/2026-03-03-spider-migration-05-chrome-bootstrap-unification.md` to `docs/plans/complete/`
