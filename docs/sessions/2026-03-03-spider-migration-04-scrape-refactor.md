# Spider Migration 04: Scrape Command Refactor

**Date:** 2026-03-03
**Branch:** `feat/sidebar`
**Plan:** `docs/plans/2026-03-03-spider-migration-04-scrape-command-official-api.md`

## Session Overview

Executed Spider Migration 04 plan — originally intended to replace the manual `subscribe()+crawl_raw()` scrape flow with Spider's official `scrape_raw()` API. Investigation revealed Spider's `scrape_raw()` has a confirmed biased-select race bug, making Axon's manual pattern the correct permanent approach. Adapted the plan to focus on DRYing up the existing code: extracted shared helpers and added comprehensive contract tests.

## Timeline

1. **Plan review + Spider API investigation** — Read the migration plan, dispatched an explore agent to examine Spider's `scrape_raw()` implementation at `../spider/spider/src/website.rs:4894-5034`. Confirmed the biased-select race in `tokio::select! { biased }` (done_rx fires before rx2.recv() on fast fetches).
2. **Plan adaptation** — Dropped Task 2 (official scrape API adapter) since the race bug makes it unusable. Merged remaining tasks into: contract tests → extract helpers → verify.
3. **Task 1: Contract tests** — Added 21 new tests covering `select_output` edge cases and `build_scrape_website` config mapping. Fixed a test that included `<title>Empty</title>` in an "empty body" test (title leaked into markdown output).
4. **Task 2: Refactor** — Extracted `fetch_single_page()` and `build_scrape_json()` helpers, eliminating 3x JSON response duplication and 2x fetch pattern duplication. Net -24 lines.
5. **Task 3: Verification** — All gates passed: fmt, clippy, 39 scrape tests green.

## Key Findings

- **Spider `scrape_raw()` race bug confirmed** — `website.rs:4945-4987`: `tokio::select! { biased }` at line 4965 prioritizes `done_rx` over `rx2.recv()`. For fast single-page fetches, the done signal fires before the page broadcast is received. `self.pages` stays empty. This is NOT a transient issue — it's architectural in Spider's select ordering.
- **Axon's subscribe pattern is correct** — `scrape.rs:83-106`: Owning the subscription and spawning the collector before crawl guarantees the page is captured. This is the permanent correct approach.
- **The agent committed unrelated files** — The first subagent used `--no-verify` and committed 25+ unrelated files (oauth_google module, CHANGELOG, Cargo.lock, test_html5gum.rs). Required manual `git reset HEAD~1` and selective re-apply of only `scrape.rs` changes.
- **`crates/mcp.rs` collision** — A stray `crates/mcp.rs` file was created by the agent, conflicting with `crates/mcp/mod.rs`. Deleted to restore compilation.

## Technical Decisions

- **Keep subscribe+crawl pattern** — Spider's `scrape_raw()` race makes the official API unusable. The manual pattern with explicit subscribe/collect/crawl is architecturally correct, not a workaround. Updated doc comments to reflect this.
- **Single `build_scrape_json()` helper** — Title/description/markdown extraction was duplicated in 3 places (scrape_payload, scrape_one JSON branch, select_output Json arm). Centralized into one 8-line function.
- **Inline tests, not separate file** — Plan called for `scrape_migration_tests.rs` but inline `mod tests` is simpler and follows existing patterns.
- **Removed duplicate tests** — Agent generated duplicate tests (e.g., `missing_title` appeared twice with slightly different names). Cleaned down to 21 unique new tests.

## Files Modified

| File | Purpose |
|------|---------|
| `crates/cli/commands/scrape.rs` | +21 contract tests, extract `fetch_single_page()` + `build_scrape_json()`, eliminate duplication |

## Commits

| SHA | Message |
|-----|---------|
| `caa95640` | `test: add scrape migration contract coverage` |
| `b8dc46b7` | `refactor: unify scrape response shaping and fetch pattern` |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Fetch pattern | Duplicated 13-line subscribe+crawl block in `scrape_payload` and `scrape_one` | Single `fetch_single_page()` helper |
| JSON response construction | Built identically in 3 places (scrape_payload, scrape_one, select_output) | Single `build_scrape_json()` helper |
| Test coverage | 16 scrape tests | 37 scrape tests |
| Net lines | ~288 lines of production code | ~264 lines (-24 net) |
| External behavior | Unchanged | Unchanged — same JSON shape, same CLI output |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo fmt --check` | Clean | Clean | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | Clean | Clean | PASS |
| `cargo test scrape -- --nocapture` | 39 pass | 39 pass, 0 fail | PASS |
| `cargo test scrape::tests` | 37 pass | 37 pass, 0 fail | PASS |
| lefthook pre-commit (monolith, rustfmt, check, test) | Pass | 716 tests pass, all hooks green | PASS |

## Risks and Rollback

- **Low risk** — Pure refactor with no behavior change. All existing tests pass unchanged.
- **Rollback:** `git revert b8dc46b7 caa95640` reverts both commits cleanly.

## Decisions Not Taken

- **Use Spider `scrape_raw()` directly** — Rejected because of confirmed biased-select race bug in `tokio::select! { biased }`. Would silently return empty pages on fast fetches.
- **Feature flag for dual-path** — Plan Task 2 proposed `cfg.experimental_scrape_official` toggle. Rejected since there's no viable "official" path to toggle to.
- **Separate test file** — `scrape_migration_tests.rs` adds module wiring complexity for no benefit over inline tests.
- **Extract HTTP status check helper** — Both `scrape_payload` and `scrape_one` check `!page.status_code.is_success()`. At 2 lines each, extraction would reduce clarity for no DRY benefit.

## Open Questions

- **Spider race fix upstream?** — The biased-select race in `scrape_raw()` could be fixed by reordering the `select!` branches or using non-biased select. Worth filing upstream if not already tracked.
- **`to_markdown` called twice in JSON+embed path** — `scrape_one` calls `to_markdown` at line 242 for embed, and `build_scrape_json` calls it again at line 249 for JSON output. Minor inefficiency; could cache but adds complexity.

## Next Steps

- Clean up the orphaned `62bdae5e` commit (old bad commit in reflog, harmless but noisy)
- Consider filing Spider upstream issue for the `scrape_raw()` biased-select race
- Move `docs/plans/2026-03-03-spider-migration-04-scrape-command-official-api.md` to `docs/plans/complete/`
