# Session: Map Command Engine Unification (Spider Migration 06)
**Date:** 2026-03-04
**Branch:** `feat/sidebar`
**Plan:** `docs/plans/2026-03-03-spider-migration-06-map-command-unification.md`
**Workflow:** Subagent-Driven Development (5 tasks, each with spec + quality review)

---

## Session Overview

Executed Spider Migration Plan 06: removed manual sitemap append/sort/dedup logic from the `map` CLI command and moved it into a new engine-owned function. The CLI `map` command is now a thin delegation layer — it calls `crawl::engine::map_with_sitemap()` and formats output only.

A final code review caught two issues (semantic ambiguity in `sitemap_urls` count, and a shadow test that didn't test the real engine path), both of which were fixed before closing.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Read plan, extracted 5 tasks, created TodoWrite |
| Task 1 | Created `map_migration_tests.rs` with 3 contract tests (mock server + pure unit) |
| Task 2 | Added `MapResult` struct + `map_with_sitemap()` to `crates/crawl/engine.rs`; simplified `map.rs` |
| Task 3 | Verified `discover_sitemap_urls_with_robots` re-exports already cleaned from CLI layer |
| Task 4 | Added `map_payload_json_has_expected_fields` pure unit contract test |
| Task 5 | Updated `docs/ARCHITECTURE.md`, ran full verification suite (`cargo fmt --check`, `clippy`, `test`) |
| Post-tasks | Final code review found 2 issues; fix agent corrected `sitemap_urls` semantics and deleted shadow test |

---

## Key Findings

- **Module path resolution**: `map.rs` + `mod map_migration_tests` resolves to `crates/cli/commands/map/map_migration_tests.rs` (not the flat path) — Rust uses the sibling-directory form for non-`mod.rs` modules.
- **Pre-existing compile errors**: `crates/web/execute/args.rs` and `exe.rs` used `log::warn!` but `log` was not in `[dependencies]` for the workspace. Fixed by adding `log = "0.4"` to root `Cargo.toml`.
- **`sitemap_urls` semantic bug**: The original implementation computed `urls.len().saturating_sub(pages_seen)` after dedup, yielding a "net-new" count rather than the raw sitemap count. Fixed by capturing `sitemap_url_list.len()` before the merge/dedup step.
- **Shadow test antipattern**: `map_autoswitch_only_falls_back_when_no_pages_seen` tested a local closure copy of the AutoSwitch condition, not the engine. Deleted — coverage comes from the two httpmock integration tests that call the real `map_with_sitemap` path.
- **`discover_sitemap_urls_with_robots`** was not truly dead — it still has two legitimate callers in `crawl/audit/` (manifest audit + its migration tests). Only the re-export chain to `map.rs` was removed.

---

## Technical Decisions

### Engine-owned URL set
**Decision:** Move all URL merge/sort/dedup into `crawl::engine::map_with_sitemap()` returning `MapResult`.
**Rationale:** CLI should format output only. Engine owns the URL set semantics. Testable in isolation without CLI noise.

### `sitemap_urls` = raw count (not net-new)
**Decision:** `MapResult::sitemap_urls` stores the raw count returned by `discover_sitemap_urls()` before any deduplication against crawler URLs.
**Rationale:** "Net-new URLs" (exclusive to sitemap) is hard to use meaningfully at the CLI output level. Raw count is unambiguous and the field name implies it.

### Keep `map_payload` + `run_map` both as thin delegation
**Decision:** Both functions individually call `map_with_sitemap` and construct their own output.
**Rationale:** `map_payload` is used by MCP/WebSocket paths; `run_map` has UX spinners. Merging them would mix presentation logic.

### AutoSwitch fallback condition
**Decision:** Engine uses `pages_seen == 0` (not `should_fallback_to_chrome()`) for map fallback.
**Rationale:** `map` has no thin-page concept. `should_fallback_to_chrome()` checks thin ratio + markdown files — always returns `true` for map, causing unnecessary Chrome re-crawls.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/crawl/engine.rs` | Added `MapResult` struct + `map_with_sitemap()` (~50 LOC) | Engine-owned URL discovery with dedup |
| `crates/cli/commands/map.rs` | Simplified 172→85 lines; removed all merge/dedup/crawl imports | CLI delegates to engine |
| `crates/cli/commands/map/map_migration_tests.rs` | Created (369 lines) | 3 contract tests: uniqueness, field consistency, JSON schema |
| `crates/cli/commands/crawl.rs` | Removed `pub(crate) use audit::discover_sitemap_urls_with_robots` | Re-export cleaned (no CLI callers remain) |
| `crates/cli/commands/crawl/audit.rs` | Removed `pub(crate) use sitemap::discover_sitemap_urls_with_robots` | Same re-export chain cleanup |
| `Cargo.toml` | Added `log = "0.4"` to workspace dependencies | Fixed pre-existing compile error in `crates/web/execute/` |
| `docs/ARCHITECTURE.md` | Added "Map Command" subsection under Crawl Pipeline | Documents engine unification |

---

## Commands Executed

```bash
# Final test run
cargo test map -- --nocapture
# Result: 27 passed, 0 failed

# Full verification (Task 5)
cargo fmt --check       # clean
cargo clippy --all-targets --all-features -- -D warnings  # 0 errors
cargo test map -- --nocapture  # 28 passed (before fix), 27 after shadow test deleted

# Compile error diagnosis
cargo check 2>&1 | grep "^error"
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| CLI merge/dedup | `map.rs` called `discover_sitemap_urls_with_robots`, appended, sorted, deduped | Engine owns all merge/sort/dedup |
| `sitemap_urls` field | Computed as `urls.len() - pages_seen` (net-new after dedup) | Raw count returned by `discover_sitemap_urls()` before dedup |
| AutoSwitch | Duplicate condition in both `map_payload` AND `run_map` | Single condition in `map_with_sitemap` engine function |
| `map.rs` size | 172 lines | 85 lines |
| Test coverage | 0 map-specific tests | 3 contract tests (uniqueness, field semantics, JSON schema) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test map_migration_tests -- --nocapture` | 3 pass | 3 pass | ✅ |
| `cargo test map_payload_json_has_expected_fields -- --nocapture` | 1 pass | 1 pass | ✅ |
| `cargo test map -- --nocapture` | all pass | 27 pass, 0 fail | ✅ |
| `cargo check` | 0 errors | 0 errors | ✅ |
| `cargo fmt --check` | clean | clean | ✅ |
| `cargo clippy --all-targets -- -D warnings` | 0 errors | 0 errors | ✅ |
| Full `cargo test` | all pass | 784 pass, 0 fail | ✅ |

---

## Source IDs + Collections Touched

*(Axon embedding recorded below after session file is written)*

---

## Risks and Rollback

- **`sitemap_urls` semantic change**: Callers comparing the new value to the old `mapped_urls - pages_seen` formula will see different numbers when crawler and sitemap URLs overlap. MCP schema and CLI JSON output field name unchanged — no wire format break.
- **Rollback**: `git revert` the commits `e7238085` (fix) + `4fff3661` (docs) + `4eea6b93` (test) + the engine/map commits. The `discover_sitemap_urls_with_robots` function itself was NOT deleted — re-adding the re-exports restores the old behavior.

---

## Decisions Not Taken

- **Rename `sitemap_urls` to `sitemap_net_new_urls`**: Would be more precise for the old semantic but would break the JSON wire format. Chose to fix the semantic instead.
- **Merge `map_payload` and `run_map`**: Would reduce duplication but conflate presentation (spinners, terminal output) with data (JSON payload). Kept separate.
- **Track both raw and net-new counts**: `MapResult` could expose both `sitemap_raw` and `sitemap_net_new`. Decided YAGNI — one count is enough for current consumers.

---

## Open Questions

- The `discover_sitemap_urls_with_robots` function in `crawl/audit/sitemap.rs` and the `discover_sitemap_urls` function in `crawl/engine/sitemap.rs` serve similar purposes but are separate implementations. Are they candidates for consolidation in a future migration step?
- `run_crawl_once` in `engine.rs` is 122 lines — 2 over the 120-line monolith hard limit. Pre-existing issue; not introduced in this session.

---

## Next Steps

- Decide on Option 2 (push + PR) or Option 3 (keep branch) for `feat/sidebar`
- Address the `run_crawl_once` monolith policy violation (pre-existing, 122 lines)
- Consider consolidating the two sitemap URL discovery functions in a future Spider Migration step
