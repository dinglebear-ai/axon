# Session: Export Monolith Split + ACP Test Fix
Date: 2026-03-20
Branch: feat/pulse-shell-and-hybrid-search

## Session Overview

Completed a two-part task:
1. Split `crates/services/export.rs` (937 non-test lines) into the Rust 2018 file-per-module layout to satisfy the ≤500-line monolith policy enforced by lefthook pre-commit hooks.
2. Fixed a pre-existing test regression (`services_acp_llm_complete_text_requires_adapter_config`) caused by a prior branch change to `Config::default()`.

The commit was pushed to `feat/pulse-shell-and-hybrid-search` as `dd6a7b68`.

---

## Timeline

1. **Context recovery** — Read the previous session summary. The `git commit` had failed mid-session due to the lefthook `monolith` hook blocking on `export.rs` (937 lines, `verify_manifest_value` 161 lines). The `export/` directory had been created but no files written.
2. **Read full export.rs** — Read all 1371 lines in chunks to understand the complete structure before splitting.
3. **Wrote submodule files** — Created `export/helpers.rs`, `export/query.rs`, `export/seeds.rs`, `export/verify.rs` in parallel.
4. **Rewrote export.rs root** — Replaced with ~130-line module root declaring the four submodules.
5. **`cargo check`** — Passed with 4 unused-import warnings; fixed all warnings.
6. **`cargo fmt`** — Clean.
7. **First commit attempt** — Pre-commit hook passed monolith check and 1443 unit tests but failed with exit code 1 (reason unclear from truncated output).
8. **Second commit attempt** — Revealed `services_acp_llm_complete_text_requires_adapter_config` integration test failing.
9. **Root cause analysis** — Branch changed `Config::default()` to set `acp_adapter_cmd: Some("codex-acp")` instead of `None`; test expected `None`.
10. **Fix** — Changed test to use `Config { acp_adapter_cmd: None, ..Config::default() }`.
11. **Third commit attempt** — All hooks passed; committed and pushed as `dd6a7b68`.

---

## Key Findings

- **Monolith violation**: `crates/services/export.rs` was 937 non-test lines (limit 500) with `verify_manifest_value` at 161 lines (hard limit 120).
- **Root of verify_manifest_value bloat**: A 73-line default `ExportManifest` struct literal (the parse-error fallback) lived inline. Extracted to `build_default_failed_manifest()`.
- **Test regression root cause**: `config_impls.rs:90` changed `acp_adapter_cmd: None` → `acp_adapter_cmd: Some("codex-acp")` as part of the branch's ACP prewarm work. This caused `complete_text(&Config::default(), ...)` to attempt to spawn `codex-acp` rather than returning an "AXON_ACP_ADAPTER_CMD not set" error.
- **Rust module visibility**: Child modules can access parent-module private items only if marked `pub(super)` or higher. Used `pub(super)` throughout for all cross-module helper functions.
- **`build_integrity` shared**: Called from both root `export_manifest` (via `helpers::build_integrity`) and `verify_manifest_value` in `verify.rs` (via `super::helpers::build_integrity`). The `pub(super)` visibility in `helpers.rs` serves both paths correctly.
- **Constants visibility**: `EXPORT_SCHEMA_VERSION` and `REQUIRED_TOP_LEVEL_KEYS` marked `pub(super)` in root so `verify.rs` can reference them as `super::EXPORT_SCHEMA_VERSION`.

---

## Technical Decisions

- **Put `QueryHistoryExport`, `ScrapeHistoryExport` in `query.rs`** — These types are returned by the async query functions. Keeping them co-located avoids crossing module boundaries for their definition.
- **Put `RebuildSeedsInput` in `seeds.rs`** — It's the input type for `build_rebuild_seeds`. Root `export_manifest` imports it via `use seeds::RebuildSeedsInput`.
- **Keep tests inline in `export.rs`** — The `#[cfg(test)] mod tests` block uses `super::*` which includes the root-module items. Rather than splitting to a separate file (which would require re-exporting private helpers), tests stay in the root with an explicit `use super::verify::verify_manifest_value` import.
- **No allowlist** — User explicitly said "split all monoliths - no allowlist." All violations resolved by structural split.
- **Function-level splits in `verify.rs`**: `verify_manifest_value` 161→~55 lines by extracting `build_default_failed_manifest()` (73-line default manifest literal) and `check_integrity_mismatches()` (36-line integrity loop).

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/services/export.rs` | Rewritten | Root module: mod declarations, constants, ExportOptions, export_manifest, verify_manifest_json, tests |
| `crates/services/export/helpers.rs` | Created | Utility functions: build_integrity, dedup_*, hash_sorted_strings, status_matches, json_* |
| `crates/services/export/query.rs` | Created | Async DB query functions + QueryHistoryExport, ScrapeHistoryExport structs |
| `crates/services/export/seeds.rs` | Created | build_rebuild_seeds, collect_crawl_seed_urls, build_settings_snapshot, RebuildSeedsInput |
| `crates/services/export/verify.rs` | Created | verify_manifest_value (split into 3 functions), build_default_failed_manifest, check_integrity_mismatches |
| `tests/services_acp_llm.rs:146` | Fixed | Test now uses `Config { acp_adapter_cmd: None, ..Config::default() }` to explicitly test the empty-adapter-cmd path |

---

## Commands Executed

```bash
cargo check
# Result: 0 errors, 4 unused-import warnings → fixed

cargo fmt
# Result: clean

cargo test --lib
# Result: 1443 passed, 0 failed

cargo test --test services_acp_llm
# After fix: 4 passed, 0 failed

git add . && git commit -m "feat(export): split export.rs monolith + fix acp_llm test regression"
# Result: all hooks passed, commit dd6a7b68

git push
# Result: feat/pulse-shell-and-hybrid-search → origin/feat/pulse-shell-and-hybrid-search
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `export.rs` line count | 937 non-test lines (monolith violation) | ~130 lines (compliant) |
| `verify_manifest_value` line count | 161 lines (hard fail) | ~55 lines (compliant) |
| `services_acp_llm` test | FAILING (AXON_ACP_ADAPTER_CMD assertion failed) | PASSING |
| Pre-commit hook | Blocked on monolith + test failure | All hooks pass |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | 0 errors, 0 warnings | 0 errors, 0 warnings | ✅ PASS |
| `cargo test --lib` | all pass | 1443 passed, 0 failed | ✅ PASS |
| `cargo test --test services_acp_llm` | 4 passed | 4 passed | ✅ PASS |
| `git commit` (lefthook hooks) | all pass | monolith ✓, test ✓, check ✓ | ✅ PASS |
| `git push` | pushed to remote | dd6a7b68 pushed | ✅ PASS |

---

## Source IDs + Collections Touched

No Axon crawl/embed/retrieve operations were performed during this session (code-only work).

---

## Risks and Rollback

- **Risk**: The module split is purely structural with no logic changes. Risk is low.
- **Rollback**: `git revert dd6a7b68` or restore original `export.rs` from `ebd54a66` and delete the `crates/services/export/` directory.
- **ACP test fix**: The test change is correct. If `Config::default()` ever reverts `acp_adapter_cmd` back to `None`, the test will still pass (the `None` override becomes redundant but harmless).

---

## Decisions Not Taken

- **Allowlist exception**: User declined. All violations resolved by code split.
- **Move tests to `export/tests.rs`**: Would require re-exporting private helpers through the module tree. Keeping tests inline in root is simpler and the `#[cfg(test)]` block is exempt from the monolith line count.
- **Move `RebuildSeedsInput`/`QueryHistoryExport`/`ScrapeHistoryExport` to root**: These are internal types only used within the export module hierarchy. Keeping them in the child modules they logically belong to is cleaner.

---

## Open Questions

- The branch has 26 security vulnerabilities reported by GitHub Dependabot (9 high, 15 moderate, 2 low). These are pre-existing and not addressed in this session.
- Other monolith warnings (non-blocking) remain: `export_manifest` (85L), `build_rebuild_seeds` (83L), `research_payload` (84L), `retrieve_ask_candidates` (92L), `query_results` (90L). All under the 120-line hard limit.

---

## Next Steps

1. **Migration day**: The main pending task — re-embed all content into `cortex_v2` collection:
   - Stop workers
   - Restart TEI without `--default-prompt` (remove lines 126–127 from `docker-compose.services.yaml`)
   - Deploy new binary
   - Create `cortex_v2` collection and re-embed all indexed content
   - Flip `AXON_COLLECTION=cortex_v2` in `.env`
   - Restart workers
2. Implement Tier 1–4 embedding quality fixes per plans in `docs/superpowers/plans/2026-03-19-*.md`
3. Check if other files on this branch have monolith violations to pre-empt future commit blocks.
