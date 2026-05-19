# Cross-Crate DRY Refactors

**Date:** 2026-03-18
**Branch:** `feat/pulse-shell-and-hybrid-search`
**Version:** 0.27.0

## Session Overview

Extracted 4 shared utilities to eliminate cross-crate code duplication discovered during the `/simplify` review of the ACP prewarm observability work. Created `crates/core/paths.rs` for path resolution, promoted `elapsed_ms()` to `pub(crate)`, added `#[derive(Default)]` to `AcpPromptTurnRequest`, and updated all consumer sites across 12+ files.

## Timeline

1. **Context recovery** — Resumed from a prior session that identified 4 DRY violations during `/simplify` review
2. **Investigation** — Read all duplication sites across 8+ files to understand pattern variations
3. **Implementation** — Created shared utilities and updated all consumer sites
4. **Verification** — `cargo check` clean, all 1406 tests pass

## Key Findings

- **`AXON_DATA_DIR` resolution duplicated in 8 files** — Two variants: (a) `Option<PathBuf>` for sites that have their own fallback, (b) full fallback chain (`AXON_DATA_DIR` → `$HOME/.local/share` → `/tmp`) for prewarm/assistant mode
- **`elapsed_ms()` in `doctor.rs:18`** — Already had u64 saturation safety (`u128::from(u64::MAX)` check); 4 other sites used raw `as u64` casts without saturation
- **`path_basename()` lifetime constraint** — Works well when source string outlives the call (e.g., `&adapter.program`), but doesn't work in `map` closures where the string is a temporary owned value (`help.rs`, `metrics.rs`)
- **`AcpPromptTurnRequest`** — All fields are `Option` or `Vec`, so `#[derive(Default)]` works directly despite `AcpMcpServerConfig` being an enum (it's wrapped in `Vec<>` which defaults to empty)

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Two path functions (`axon_data_dir` + `axon_data_base_dir`) | Sites differ: some need `Option<PathBuf>` to chain their own fallback, others need the full HOME fallback chain |
| `pub(crate)` visibility for all utilities | These are internal helpers, not part of the public API |
| Keep `elapsed_ms()` in `doctor.rs` (promote, don't move) | Minimal change; re-export through `health` module preserves existing import paths |
| Skip `path_basename` for `help.rs`/`metrics.rs` | Lifetime issue — temporary `String` from iterator drops before the borrow can be used. Inline pattern is correct there. |
| `..Default::default()` in test sites | Cleaner than 6 explicit `None`/`vec![]` fields; highlights only the fields that matter for each test case |

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/core/paths.rs` | Created | `axon_data_dir()`, `axon_data_base_dir()`, `path_basename()` |
| `crates/core.rs` | Modified | Added `pub mod paths;` |
| `crates/core/health/doctor.rs:18` | Modified | `fn elapsed_ms` → `pub(crate) fn elapsed_ms` |
| `crates/core/health.rs:3` | Modified | Added `pub(crate) use doctor::elapsed_ms;` re-export |
| `crates/services/types/acp.rs:69` | Modified | Added `Default` to derive list on `AcpPromptTurnRequest` |
| `crates/services/acp/mapping/validation.rs:333-379` | Modified | 4 test sites → `..Default::default()` |
| `crates/web/execute/sync_mode/prewarm.rs` | Modified | Uses `elapsed_ms()`, `path_basename()`, `axon_data_base_dir()`, `Default` |
| `crates/web/execute/sync_mode/pulse_chat.rs:262-269` | Modified | Uses `axon_data_base_dir()` |
| `crates/web/execute/sync_mode/subprocess.rs:167` | Modified | Uses `elapsed_ms()` |
| `crates/web/execute/sync_mode.rs:106,119` | Modified | Uses `elapsed_ms()` |
| `crates/web/execute/async_mode.rs:212,216` | Modified | Uses `elapsed_ms()` |
| `crates/web/execute/files.rs:44-48` | Modified | Uses `axon_data_dir()` |
| `crates/web/execute/mcp_config.rs:136` | Modified | Uses `axon_data_dir()` |
| `crates/mcp/server/artifacts/path.rs:47-51` | Modified | Uses `axon_data_dir()` |
| `crates/core/logging.rs:355-360` | Modified | Uses `axon_data_dir()` |
| `crates/core/health.rs:72-77` | Modified | Uses `axon_data_dir()` |
| `crates/core/config/parse/build_config.rs:582-588` | Modified | Uses `axon_data_dir()` |

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| `elapsed_ms` in 4 web/execute files | Raw `as u64` cast (wraps on >u64::MAX) | Saturating cast via shared `elapsed_ms()` |
| `AXON_DATA_DIR` resolution | 8 inline copies with subtle trim/filter variations | Single canonical `axon_data_dir()` with consistent trim+filter |
| `AcpPromptTurnRequest` construction | 6-field struct literals everywhere | `..Default::default()` for most fields |
| Test readability | Boilerplate hides test intent | Only test-relevant fields visible |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Compiles | `Finished dev profile in 9.04s` | PASS |
| `cargo test --lib` | All pass | `1406 passed; 0 failed; 11 ignored` | PASS |

## Risks and Rollback

- **Low risk** — Pure refactoring, no behavior changes. All existing tests pass.
- **`to_string_lossy()` in health.rs/logging.rs** — Used when converting `PathBuf` back to `String` for sites that expect `String` not `PathBuf`. Non-UTF-8 paths would get replacement characters. This matches the prior behavior (env vars are always UTF-8 on Linux).
- **Rollback:** Revert the commit containing these changes.

## Decisions Not Taken

- **Moving `elapsed_ms` to `paths.rs`** — Doesn't fit semantically; keeping in `doctor.rs` with re-export is minimal and clear
- **Creating a general `crates/core/util.rs`** — Too vague a name; `paths.rs` is focused and discoverable
- **Updating `help.rs`/`metrics.rs` to use `path_basename`** — Lifetime constraints make it awkward in map closures with owned temporaries

## Open Questions

- Should `axon_data_dir()` cache its result (it reads env vars on every call)?
- Should `axon_data_base_dir()` log a warning when falling back to `/tmp` (currently silent)?

## Next Steps

- Commit these changes (unstaged)
- Consider adding `axon_data_dir()` to the remaining site in `build_config.rs` that uses `let-else` syntax
- Monitor for any additional `AXON_DATA_DIR` duplication in new code
