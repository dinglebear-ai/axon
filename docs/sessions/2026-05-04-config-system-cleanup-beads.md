---
date: 2026-05-04 14:04:28 EST
repo: git@github.com:jmagar/axon.git
branch: bd-1d2.1/config-system-cleanup
head: f052ee15
plan: none
agent: Claude (claude-sonnet-4-6)
session id: unknown
transcript: none (no matching .jsonl found)
working directory: /home/jmagar/workspace/axon_rust
pr: "65 — BD-1d2.1: Phase 1 config system cleanup — TOML layer + axon.json removal — https://github.com/jmagar/axon/pull/65"
---

## User Request

Work the remaining open beads in the axon_rust-1d2.1 plan: `1d2.1.5` (priority-chain integration tests), `1d2.1.6` (load_toml_config refactor), `1d2.1.7` (AXON_CONFIG_PATH validation), `1d2.1.8` (HOME absolute path check), `1d2.1.9` (Completions early-return doc).

## Session Overview

Closed all 5 remaining open beads in the Phase 1 config system cleanup epic. Two sessions of `lavra-work-multi` were run — the first pair (1d2.1.5 + 1d2.1.6) then the remaining three (1d2.1.7 + 1d2.1.8 + 1d2.1.9). A lavra-review after the security beads filed a P2 finding (`1d2.1.8.1`) for an incomplete path-traversal guard.

## Sequence of Events

1. Fetched bead details for 1d2.1.5 and 1d2.1.6; both had empty descriptions — used parent bead context and codebase reading to understand scope.
2. **Wave 1 (1d2.1.6):** Refactored `load_toml_config()` and `load_from_path()` to return `Result<TomlConfig, String>` instead of calling `process::exit(1)`. Updated call site in `build_config.rs` with `?`. Added `malformed_toml_returns_err` test; updated 5 existing tests with `.unwrap()`.
3. **Wave 2 (1d2.1.5):** Added 6 priority-chain integration tests to `build_config.rs` covering TOML-beats-default and env-beats-TOML for `ask_chunk_limit`, `ask_candidate_limit`, `ask_min_relevance_score`, and `hybrid_search_enabled`. Tests used `tempfile::Builder::new().suffix(".toml")` after bead 1d2.1.7 introduced extension validation.
4. Pushed first batch; confirmed 1618 tests passing.
5. **Wave 3 (1d2.1.7):** Added `.toml` extension validation to `resolve_config_path()`, changing return type to `Result<Option<PathBuf>, String>`. Fixed clippy lint (`map_or(false, …)` → `is_some_and`). Fixed test tempfile naming bug (NamedTempFile has no .toml suffix → all 6 integration tests failed until Builder was used).
6. **Wave 3 (1d2.1.8):** Added `is_absolute()` check to `axon_home_dir()` in `paths.rs`. Added `axon_home_dir_returns_none_when_home_is_relative` test.
7. **Wave 3 (1d2.1.9):** Added explanatory comment at the Completions early-return in `build_config.rs` (committed together with 1d2.1.7 changes).
8. Ran `lavra-review` (triggered by security labels on 1d2.1.7 + 1d2.1.8). Security sentinel found one P2 gap: `is_absolute()` does not block `..` components.
9. Filed `axon_rust-1d2.1.8.1` (P2 child) for the `..` traversal gap; filed `axon_rust-2z1` (P3 pre-existing) for `axon_data_base_dir()` missing the same guard. Logged LEARNED + MUST-CHECK on 1d2.1.8.
10. Pushed all commits.

## Key Findings

- `NamedTempFile::new()` creates files without extension — any test using `into_config()` + `AXON_CONFIG_PATH` pointing to a `NamedTempFile` will fail once `.toml` extension validation is active. Fix: `tempfile::Builder::new().suffix(".toml").tempfile()`.
- `Path::is_absolute()` returns `true` for `/tmp/../etc`. The `..`-component traversal bypass is not blocked by absoluteness alone. `Component::ParentDir` check required.
- `serial_test::serial` only prevents concurrency between other `#[serial]` tests. Non-serial tests in other modules can still race with it. The priority-chain integration tests needed `#[serial_test::serial]` to be safe against the `axon_config_path_non_toml_extension_returns_err` test setting `AXON_CONFIG_PATH=/etc/passwd`.
- `resolve_config_path()` return type change from `Option<PathBuf>` to `Result<Option<PathBuf>, String>` propagated cleanly to `load_toml_config()` and then via `?` in `into_config()`.

## Technical Decisions

- **`process::exit(1)` removed from `load_toml_config`:** Returning `Result<TomlConfig, String>` makes the function testable for error paths and lets callers (e.g. `into_config()`) propagate via `?`. Hard failures now appear in the `Result::Err` path rather than as unrecoverable exits at the parsing layer.
- **`.toml` extension validation on `AXON_CONFIG_PATH` only:** The default `~/.axon/config.toml` always has the correct extension; validation only applies to the user-supplied override. This avoids rejecting the normal code path.
- **`is_some_and` over `map_or(false, …)`:** Required by Clippy in this codebase (`-D warnings` policy). Used for the extension check predicate.
- **Comment for Completions early-return (1d2.1.9):** Pure documentation. The early return pre-existed; the bead only required a comment explaining why `AXON_CONFIG_PATH` and collection validation are intentionally skipped for shell completions.
- **Lavra-review after security beads:** `testing_scope: "targeted"` means review only runs when a bead has P0/P1 priority or security/architecture labels. Both 1d2.1.7 and 1d2.1.8 had `security` labels, so review ran and was justified.

## Files Modified

| File | Change |
|------|--------|
| `crates/core/config/parse/toml_config.rs` | `load_toml_config` → `Result<TomlConfig, String>`; `load_from_path` → `Result`; `.toml` extension check in `resolve_config_path`; 2 new tests |
| `crates/core/config/parse/build_config.rs` | Call site `load_toml_config()?`; Completions early-return comment; `TempfileBuilder` import; 6 priority-chain tests use `.toml` suffix |
| `crates/core/paths.rs` | `axon_home_dir()` absolute-path guard; 1 new test |

## Commands Executed

```bash
# Verify compilation after each change
cargo check

# Run targeted tests
cargo test -- config::parse::toml_config
cargo test -- config::parse::build_config::tests
cargo test -- crates::core::paths

# Full suite (run before each commit)
cargo test   # 1620 passed, 0 failed

# Commits
git commit -m "refactor(axon_rust-1d2.1.6): load_toml_config returns Result<TomlConfig, String>"
git commit -m "test(axon_rust-1d2.1.5): add priority-chain integration tests for TOML config layer"
git commit -m "fix(axon_rust-1d2.1.7): validate AXON_CONFIG_PATH must have .toml extension"
git commit -m "fix(axon_rust-1d2.1.8): reject non-absolute HOME in axon_home_dir"
# 1d2.1.9 committed together with 1d2.1.7 (build_config.rs already staged)

git push  # pushed all commits to bd-1d2.1/config-system-cleanup
```

## Errors Encountered

| Error | Root Cause | Resolution |
|-------|-----------|------------|
| 13 test failures after 1d2.1.7 | `NamedTempFile::new()` produces files without `.toml` extension; new validation rejected them | Changed all 6 integration tests to `TempfileBuilder::new().suffix(".toml").tempfile()` |
| Clippy error `unnecessary_map_or` | `.map_or(false, …)` rejected by `-D warnings`; `is_some_and` not used | Changed to `.is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))` |
| `unused import: tempfile::NamedTempFile` | Replaced `NamedTempFile` with `TempfileBuilder` but left old import | Changed import to `use tempfile::Builder as TempfileBuilder` |
| `unwrap_err()` on `Result<TomlConfig, _>` failed | `unwrap_err()` requires `T: Debug`; `TomlConfig` doesn't derive `Debug` | Changed test to `.err().unwrap()` which only requires `E: Debug` |

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| Malformed `AXON_CONFIG_PATH` | `process::exit(1)` inside library | `Err(String)` propagated to CLI entry point |
| `AXON_CONFIG_PATH=/etc/passwd` | Attempted to parse as TOML, printed parse error including path | Returns error immediately: "AXON_CONFIG_PATH must point to a .toml file" |
| `HOME=../relative` | `axon_home_dir()` returned `Some(PathBuf::from("../relative/.axon"))` | Returns `None` with warning to stderr |
| `axon completions fish` | No comment; config and validation silently skipped | Comment documents the intentional pre-config early return |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test` (after 1d2.1.6) | 1618 passed | 1618 passed | ✓ |
| `cargo test` (after 1d2.1.5) | 1618 passed | 1618 passed | ✓ |
| `cargo test` (after 1d2.1.7 tempfile fix) | 1620 passed | 1620 passed | ✓ |
| `cargo check` after each change | No warnings | No warnings | ✓ |
| lefthook pre-commit | All hooks pass | All hooks pass (unwrap-warn: warning-only) | ✓ |

## Risks and Rollback

- **`load_toml_config` no longer calls `process::exit`:** If a caller doesn't propagate `Err` correctly, a malformed config would silently succeed instead of aborting. Currently only `into_config()` calls it, and it uses `?`. Risk: low.
- **`.toml` extension requirement is a breaking change for anyone using `AXON_CONFIG_PATH` pointing to a file without `.toml` extension** (e.g., a symlinked config with no extension). Risk: low for CLI users; `AXON_CONFIG_PATH` is documented as pointing to a config file.
- **Rollback:** `git revert` the 3 commits or `git checkout dbe0fca9 -- crates/core/config/parse/toml_config.rs crates/core/config/parse/build_config.rs crates/core/paths.rs`.

## Decisions Not Taken

- **Symlink detection on `AXON_CONFIG_PATH`:** The security review noted that a `.toml`-named symlink to `/etc/shadow` could still leak content via parse errors. Deferred — not in scope for Phase 1; would require `symlink_metadata()` check and is a much narrower attack surface.
- **`canonicalize()` instead of `is_absolute()` + ParentDir check:** Canonicalize resolves `..` but fails if the directory doesn't exist (common in containers). ParentDir component check was preferred as lower-risk.
- **Adding `#[derive(Debug)]` to `TomlConfig` structs:** Not needed — using `.err().unwrap()` in tests instead of `.unwrap_err()` avoids the Debug bound without adding derived impls.

## Open Questions

- Should `axon_data_base_dir()` get the same HOME guard as `axon_home_dir()`? Filed as `axon_rust-2z1` (P3) — currently open.
- Should a shared `validate_home_path()` helper be extracted to DRY both functions?

## Next Steps

**In-progress (filed, not yet worked):**
- `axon_rust-1d2.1.8.1` (P2): Add `Component::ParentDir` rejection to `axon_home_dir()`. One-line fix + test.

**Follow-on (not yet started):**
- `axon_rust-2z1` (P3): Apply same guard to `axon_data_base_dir()`.
- Open a PR review / merge `bd-1d2.1/config-system-cleanup` → `main` (PR #65 is already open).
- Continue to `axon_rust-1d2.2` (web panel, scheduled jobs, next phase of config overhaul).
