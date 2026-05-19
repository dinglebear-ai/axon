# Session: mcporter Test Suite — All Green (152/152)
Date: 2026-03-27
Branch: feat/lite-mode
Version: 0.33.5 → 0.33.6

## Session Overview

Resumed from a context-compacted prior session. The previous session had fixed the root cause (`AXON_REPO_ROOT` missing from generated mcporter config) that caused all tests to fail with `.env: No such file or directory`. After that fix, 140 tests passed and 12 remained failing across 4 categories. This session diagnosed the remaining 12 failures by reading `run_error_case` and the test expectations, then made targeted fixes to bring the suite to 152/152 PASS.

## Timeline

1. **Read `run_error_case`** — confirmed it checks `.error | type == "string" and contains($expected)` (substring match on the error string).
2. **Diagnosed all 4 failure categories** by reading the test script (lines 179–190, 319–334, 412–431) and handler code.
3. **Fixed `handlers_graph.rs`** — updated lite mode error message to match the expected substring.
4. **Fixed `handlers_system.rs`** (two changes) — added `refresh_schedule` key to help actions map; added lite mode guard to `handle_export`.
5. **Fixed `test-mcp-tools-mcporter.sh`** — wrapped refresh schedule tests in mode conditional (run_error_case in lite, run_json_case in full).
6. **First test run**: 145 PASS, 7 FAIL — graph tests still failing because `cargo check` doesn't rebuild binary.
7. **Rebuilt binary** with `cargo build --bin axon`.
8. **Second test run**: 152 PASS, 0 FAIL.
9. **Bumped version** 0.33.5 → 0.33.6, updated CHANGELOG, committed, pushed.

## Key Findings

- **`run_error_case` does substring matching** — `jq -er --arg expected "$expected" '.error | type == "string" and contains($expected)'`. The `.error` field in mcporter output is the full MCP error string including the code prefix (e.g. `"MCP error -32602: ..."`), so `contains()` works as long as the message substring is present.
- **`cargo check` does not update the binary** — after editing handler code and running `cargo check`, the mcporter test still used the old binary because only `cargo build` writes to `target/debug/axon`. The intermediate test run (145 PASS) was failing only because the old binary was still running.
- **`normalize_discovered_routes` expects a `refresh_schedule` key** — the jq filter in the test (`if $action == "refresh_schedule" then .value[] | "refresh:schedule:\(.)"`) requires `.data.inline.actions` to have a top-level `"refresh_schedule"` key separate from the `"refresh"` key. Previously the help handler only listed `"schedule"` as a value under `"refresh"`.
- **Refresh schedule tests were not mode-conditioned** — unlike export (line 330) and graph (line 421), the 5 refresh schedule tests ran unconditionally for both full and lite suites and expected success. Lite mode correctly rejects them with `-32602`, so the fix was to add the mode conditional in the test (consistent with export/graph handling).

## Technical Decisions

- **Update test, not handler, for refresh schedules** — the handler's `"refresh scheduling is not available in lite mode"` error is correct and intentional. Implementing SQLite-backed refresh schedules for lite mode would be a larger feature. The right fix is to condition the test to use `run_error_case` in lite mode, matching how export and graph are handled.
- **`invalid_params` (not `internal_error`) for export lite guard** — consistent with all other lite-mode unavailability guards in the codebase (`handlers_graph.rs`, `handlers_refresh_status.rs`). Invalid params (-32602) is semantically correct: the operation is not supported with the current parameters/mode.
- **Exact message wording chosen to match test expectations** — the test file is the specification; handler messages were updated to contain the expected substrings, not the other way around.

## Files Modified

| File | Change |
|------|--------|
| `crates/mcp/server/handlers_graph.rs:13-15` | Updated lite mode error message to `"graph is not available in lite mode because it requires Postgres-backed graph storage"` |
| `crates/mcp/server/handlers_system.rs:281` | Added `"refresh_schedule": ["create", "delete", "disable", "enable", "list"]` to help actions map |
| `crates/mcp/server/handlers_system.rs:368-374` | Added lite mode guard to `handle_export` returning `invalid_params("export is not available in lite mode because it requires Postgres-backed history")` |
| `scripts/test-mcp-tools-mcporter.sh:412-430` | Wrapped 5 refresh schedule test cases in `if [[ "$lite_value" == "0" ]]` conditional |
| `Cargo.toml` | Version 0.33.5 → 0.33.6 |
| `Cargo.lock` | Updated checksum for version bump |
| `CHANGELOG.md` | Added v0.33.6 section |

## Commands Executed

```bash
cargo check --bin axon       # verified handler changes compile
cargo build --bin axon       # rebuilt binary so mcporter uses new code
bash scripts/test-mcp-tools-mcporter.sh  # first run: 145/152; second run: 152/152
cargo clippy                 # 0 warnings
git push                     # pushed feat/lite-mode
```

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `action:graph` in lite mode | Returns `"graph requires Neo4j (not available in lite mode)"` (-32602) | Returns `"graph is not available in lite mode because it requires Postgres-backed graph storage"` (-32602) |
| `action:export` in lite mode | Falls through to `export_manifest_for_config` → fails with generic `-32603` error | Returns `invalid_params` (-32602) with descriptive message immediately |
| MCP `action:help` response | `refresh_schedule` subactions absent from `.data.inline.actions` | `"refresh_schedule": ["create","delete","disable","enable","list"]` now present |
| mcporter test suite | 140 PASS, 12 FAIL | 152 PASS, 0 FAIL |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `bash scripts/test-mcp-tools-mcporter.sh` (1st run, old binary) | 152 PASS | 145 PASS, 7 FAIL (old binary) | ⚠️ |
| `cargo build --bin axon` | Success | `Finished dev profile` | ✅ |
| `bash scripts/test-mcp-tools-mcporter.sh` (2nd run, new binary) | 152 PASS | 152 PASS, 0 FAIL | ✅ |
| `cargo clippy` | 0 warnings | 0 warnings | ✅ |
| Pre-commit hook (1574 tests) | All pass | 1574 ok | ✅ |
| `git push` | Accepted | `5265c675..f78cf3ca` | ✅ |

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations were performed during this session (all work was code changes and test runs).

## Risks and Rollback

- **`action:export` in lite mode now returns -32602 instead of attempting to run** — callers relying on the old behavior (falling through to attempt export in lite mode) will now get a clear error earlier. Low risk: export genuinely requires Postgres-backed history; the old behavior was already failing.
- **Graph error message change** — any downstream system that parsed the exact old error string `"graph requires Neo4j (not available in lite mode)"` will need to be updated. Risk is low as this is an MCP error message, not a structured field.
- **Refresh schedule test conditioning** — the 5 refresh schedule tests no longer run in lite mode (they use run_error_case). If lite-mode schedule support is ever implemented, the test conditionals will need to be reverted.

## Decisions Not Taken

- **Implement lite-mode refresh schedules** — would require SQLite-backed schedule persistence; out of scope for this session. Conditioned the test instead.
- **Change export handler to return partial results in lite mode** — no meaningful partial export is possible without Postgres job history; the guard + clear error is the correct behavior.
- **Update mcporter base config to match new help shape** — not needed; the test reads the live help response and normalizes it dynamically.

## Open Questions

- The 3 Dependabot vulnerability alerts (1 high, 2 moderate) on the default branch continue to appear on every push. These are unrelated to this branch and need triage on `main`.
- P2/P3 beads (rs8.10–rs8.17) remain open: SQLite pool size vs worker count, `wait_for_job` timeout, dead code in `poll_sqlite_for_cancels`, watch def FK enforcement.

## Next Steps

- Open PR from `feat/lite-mode` → `main` (branch is feature-complete; all P0/P1 review blockers resolved; mcporter smoke suite 152/152)
- Triage Dependabot alerts on `main`
- Address P2/P3 beads (optional before merge)
