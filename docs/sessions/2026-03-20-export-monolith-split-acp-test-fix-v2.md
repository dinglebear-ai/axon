# Session: Export Monolith Split + ACP Test Fix + Graph Worker
Date: 2026-03-20
Branch: feat/pulse-shell-and-hybrid-search
Prior session doc: docs/sessions/2026-03-20-export-monolith-split-acp-test-fix.md

## Session Overview

This session continuation covers the same branch work plus one operational finding:

1. **Export monolith split** — `crates/services/export.rs` (937 non-test lines) split into 5 files via Rust 2018 file-per-module layout. All monolith policy violations resolved.
2. **ACP test fix** — `services_acp_llm_complete_text_requires_adapter_config` was failing because `Config::default()` was changed on this branch to set `acp_adapter_cmd: Some("codex-acp")`. Fixed at `tests/services_acp_llm.rs:146`.
3. **Graph worker not running** — Discovered 4 pending graph jobs with no worker consuming them. Started worker; all jobs picked up immediately.

Commit: `dd6a7b68` pushed to `feat/pulse-shell-and-hybrid-search`.

---

## Timeline

1. **Context recovery** — Previous session left mid-split after creating `crates/services/export/` dir but writing no files.
2. **Read full export.rs** — Read all 1371 lines in chunks to map the complete structure.
3. **Wrote 4 submodule files in parallel** — `helpers.rs`, `query.rs`, `seeds.rs`, `verify.rs`.
4. **Rewrote export.rs root** — ~130 lines, declares 4 submodules.
5. **`cargo check`** — 4 unused-import warnings; fixed all.
6. **First commit attempt** — Passed monolith + 1443 unit tests; failed with exit code 1 (truncated output, reason unclear).
7. **Second commit attempt** — Exposed `services_acp_llm_complete_text_requires_adapter_config` integration test failure.
8. **Root cause analysis** — `config_impls.rs:90` changed `acp_adapter_cmd: None → Some("codex-acp")` as ACP prewarm work; test assumed `None`.
9. **Fix + third commit** — `Config { acp_adapter_cmd: None, ..Config::default() }` in test. All hooks passed. Committed `dd6a7b68` and pushed.
10. **Session doc saved** — `docs/sessions/2026-03-20-export-monolith-split-acp-test-fix.md`, embedded into `cortex` (5 chunks, job `3a4214c6`).
11. **Graph worker check** — User asked if graph worker was running. `pgrep` confirmed no process. `axon graph status` showed 4 pending, 77 failed, 87 completed.
12. **Graph worker started** — `cargo run --bin axon -- graph worker &`. Within 8 seconds: 0 pending, 3 running, 1 additional completed.

---

## Key Findings

- **Export monolith**: `export.rs` was 937 non-test lines (limit 500); `verify_manifest_value` was 161 lines (hard limit 120). Both violations resolved by structural split.
- **`verify_manifest_value` bloat source**: A 73-line default `ExportManifest` struct literal lived inline as the parse-error fallback. Extracted to `build_default_failed_manifest()`.
- **ACP test regression**: `config_impls.rs:90` — default changed from `None` to `Some("codex-acp")` as part of ACP prewarm feature on this branch. Tests that call `Config::default()` and expect no adapter must now explicitly set `acp_adapter_cmd: None`.
- **Graph worker state**: 4 jobs pending with no worker. Worker is a local process (`cargo run --bin axon -- graph worker`), not managed by Docker on local dev. Must be started manually per the local dev workflow.
- **77 failed graph jobs**: Pre-existing failures, not caused by this session. Likely from prior runs when Neo4j was unavailable or the graph LLM wasn't configured.

---

## Technical Decisions

- **`pub(super)` throughout submodules**: All helper functions and types that need to cross module boundaries use `pub(super)`. This keeps them invisible outside the `export` module hierarchy while allowing full access within it.
- **`build_integrity` in helpers.rs**: Called from both root `export_manifest` and `verify.rs::verify_manifest_value`. Putting it in helpers.rs as `pub(super)` serves both call sites cleanly.
- **Tests stay inline in export.rs root**: `#[cfg(test)] mod tests` uses `super::*` + explicit `use super::verify::verify_manifest_value`. Avoids the complexity of re-exporting private helpers to a separate test file. The `#[cfg(test)]` block is exempt from the monolith line count.
- **`EXPORT_SCHEMA_VERSION` and `REQUIRED_TOP_LEVEL_KEYS` marked `pub(super)`**: Allows `verify.rs` to reference them as `super::EXPORT_SCHEMA_VERSION` without making them part of the public API.

---

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/services/export.rs` | Rewritten (~130L) | Root: mod declarations, constants, `ExportOptions`, `export_manifest`, `verify_manifest_json`, tests |
| `crates/services/export/helpers.rs` | Created (~130L) | `build_integrity`, `dedup_*`, `hash_sorted_strings`, `status_matches`, `json_*` |
| `crates/services/export/query.rs` | Created (~310L) | All async DB query fns + `QueryHistoryExport`, `ScrapeHistoryExport` |
| `crates/services/export/seeds.rs` | Created (~140L) | `build_rebuild_seeds`, `build_settings_snapshot`, `RebuildSeedsInput` |
| `crates/services/export/verify.rs` | Created (~165L) | `verify_manifest_value` (3 fns after split) |
| `tests/services_acp_llm.rs:146` | Fixed | Explicit `acp_adapter_cmd: None` so test targets the empty-adapter error path |

---

## Commands Executed

```bash
# Compilation check
cargo check
# → 0 errors, 0 warnings (after removing 4 unused imports)

cargo fmt
# → clean

cargo test --lib
# → 1443 passed, 0 failed

cargo test --test services_acp_llm
# → 4 passed, 0 failed (after fix)

git add . && git commit -m "feat(export): split export.rs monolith + fix acp_llm test regression"
# → dd6a7b68, all lefthook hooks passed

git push
# → feat/pulse-shell-and-hybrid-search pushed to origin

# Graph worker
pgrep -af "graph worker"
# → exit 1 (not running)

axon graph status
# → completed: 87, running: 0, pending: 4, failed: 77

cargo run --bin axon -- graph worker &
# → PID 2646053

sleep 8 && axon graph status
# → completed: 88, running: 3, pending: 0, failed: 77
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `export.rs` line count | 937 non-test lines (monolith violation) | ~130 lines (compliant) |
| `verify_manifest_value` line count | 161 lines (hard fail at 120) | ~55 lines (compliant) |
| `services_acp_llm` integration test | FAILING | PASSING |
| Pre-commit hook | Blocked | All hooks pass |
| Graph worker | Not running, 4 jobs pending | Running, 0 pending, 3 active |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | 0 errors, 0 warnings | 0 errors, 0 warnings | ✅ PASS |
| `cargo test --lib` | all pass | 1443 passed, 0 failed | ✅ PASS |
| `cargo test --test services_acp_llm` | 4 passed | 4 passed | ✅ PASS |
| `git commit` lefthook hooks | all pass | all pass | ✅ PASS |
| `git push` | pushed | `dd6a7b68` pushed | ✅ PASS |
| `axon embed` session doc | job queued | job `3a4214c6` completed, 5 chunks | ✅ PASS |
| `axon retrieve` session doc | chunks returned | 5 chunks from `cortex` | ✅ PASS |
| `axon graph status` after worker start | pending → running | 0 pending, 3 running | ✅ PASS |

---

## Source IDs + Collections Touched

| Source ID | Collection | Operation | Outcome |
|-----------|------------|-----------|---------|
| `docs/sessions/2026-03-20-export-monolith-split-acp-test-fix.md` | `cortex` | embed + retrieve | ✅ 5 chunks, job `3a4214c6-9bae-4092-86b9-2dbfb9c451de` |

---

## Risks and Rollback

- **Export split**: Purely structural, no logic changes. Rollback: `git revert dd6a7b68` or restore original `export.rs` from `ebd54a66` and `rm -rf crates/services/export/`.
- **ACP test fix**: Correct and safe. If `Config::default()` ever reverts `acp_adapter_cmd` to `None`, the explicit override becomes harmless.
- **Graph worker**: Started as background process on the local machine. Will not survive a terminal/shell restart. For persistent operation, use the Docker worker or a process manager.

---

## Decisions Not Taken

- **Allowlist exception for export.rs**: User explicitly rejected. All violations resolved by structural split.
- **Move tests to `export/tests.rs`**: Would require re-exporting private helpers. Tests stay inline in root where `#[cfg(test)]` exempts them from the monolith line count.
- **Docker graph worker**: User runs workers as local processes in local dev mode per `CLAUDE.md` quick start.

---

## Open Questions

- **77 failed graph jobs**: Pre-existing. Root cause unknown — likely Neo4j unavailability or missing `AXON_GRAPH_LLM_MODEL` config at time of queuing. Worth investigating if graph extraction is critical.
- **26 Dependabot vulnerabilities** on the repo (9 high, 15 moderate, 2 low) — not addressed this session.
- **Other monolith function warnings** (non-blocking): `export_manifest` 85L, `build_rebuild_seeds` 83L, `research_payload` 84L, `retrieve_ask_candidates` 92L, `query_results` 90L. All under 120-line hard limit.

---

## Next Steps

1. **Migration day** — Re-embed into `cortex_v2`:
   - Stop workers
   - Remove `--default-prompt` lines 126–127 from `docker-compose.services.yaml`
   - Deploy binary with Tier 1–4 embedding quality fixes
   - `axon migrate --from cortex --to cortex_v2` (~4–5 hours)
   - Set `AXON_COLLECTION=cortex_v2` in `.env`
   - Restart workers
2. Implement Tier 1–4 plans from `docs/superpowers/plans/2026-03-19-*.md`
3. Investigate 77 failed graph jobs — determine if retriable or root cause needs fixing
