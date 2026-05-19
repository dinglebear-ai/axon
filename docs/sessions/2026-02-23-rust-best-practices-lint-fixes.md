# Rust Best Practices Audit & Lint Fix Session

**Date:** 2026-02-23
**Branch:** `fix-crawl`
**Commit:** `9129cf4`

---

## Session Overview

Applied results of a full codebase audit against Apollo GraphQL's [Rust Best Practices handbook](https://github.com/apollographql/rust-best-practices) (9 chapters). The session covered two parts:

1. **Prior session** (context-compacted): Invoked `/rust-best-practices` skill, read all 9 chapters, reviewed the codebase, generated `rust-best-practices-fixes.md` with 19 prioritised fixes, and started implementing FIX-01 through FIX-04.
2. **This session**: Completed the remaining lint work, resolved all pre-commit hook failures, and pushed clean to `fix-crawl`.

Final state: `cargo check` clean, `cargo clippy --all-targets -- -D warnings` clean, 338 tests passing.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Resumed from prior context; added `#[allow(unsafe_code)]` to 3 test functions in `config/parse.rs` and `worker_lane.rs` |
| +5 min | `cargo test --lib` → 338 passing, 0 failed |
| +7 min | Marked FIX-01 through FIX-04 complete in `rust-best-practices-fixes.md` |
| +8 min | `/quick-push` invoked → pre-commit hook (lefthook + clippy) caught 18 more `unused_qualifications` in `search.rs`, `health.rs`, `engine/tests.rs` |
| +12 min | Fixed all 18 remaining warnings; `cargo clippy --all-targets -- -D warnings` clean |
| +14 min | Commit `9129cf4` passed all hooks; pushed to remote |

---

## Key Findings

- **`unused_qualifications` lint scope includes test targets**: The `[lints]` section added to `Cargo.toml` applies to ALL compilation targets including `--all-targets`. `cargo check` / `cargo test --lib` only checked lib targets; the pre-commit hook ran `clippy --all-targets` and caught 18 additional warnings in test-only code (`search.rs` test fn, `health.rs` test module, `engine/tests.rs`).
- **`std::env::set_var`/`remove_var` became `unsafe fn` in Rust 1.86**: Any test function with explicit `unsafe { env::set_var(...) }` blocks triggers `unsafe_code = "deny"`. The fix is `#[allow(unsafe_code)]` on the specific test function, not on the whole module.
- **`replace_all=true` on type names can corrupt import lines**: Earlier in the session, `replace_all` on `serde_json::Value` accidentally rewrote the `use serde_json::Value;` import to `use Value;`. Stale build cache masked this during intermediate checks — always run a fresh `cargo check` after broad replacements.
- **`spider::website::Website` is in scope via `use super::*`**: `engine/tests.rs` uses `use super::*` which imports `Website` from `engine.rs`'s `use spider::website::Website;`. The fully-qualified `spider::website::Website::new(...)` in test bodies was redundant.
- **`health.rs` `env::` calls are NOT in `unsafe {}` blocks** — they compile without `#[allow(unsafe_code)]` because the current toolchain does not yet require `unsafe {}` for bare `env::set_var` calls in non-concurrent test contexts (or the toolchain version predates 1.86 strict unsafe).

---

## Technical Decisions

- **`#[allow(unsafe_code)]` per-function, not per-module**: Placed immediately before `#[test]`, not on the `mod tests` block. Keeps `unsafe_code = "deny"` enforced on all non-test code in the same module.
- **`env::` not `std::env::` in health.rs**: `use std::env;` is already imported at line 2 of `health.rs`, so `env::remove_var` is the correct unqualified form — not `remove_var` directly.
- **Did not add `unsafe {}` to health.rs test calls**: The calls are syntactically safe (no explicit `unsafe {}` block present). Adding `unsafe {}` would introduce a new lint violation; only removing the `std::` prefix was needed.
- **`rust-best-practices-fixes.md` stays in project root** (not `docs/`): It is an active working document for ongoing fixes, not a completed session log. Moving it to `docs/` would imply it's archived.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `Cargo.toml` | Added `[lints]` section | Enforce `unsafe_code=deny`, `unused_qualifications=warn`, `clippy::all=warn` workspace-wide |
| `crates/core/http.rs` | `format!(...).into()` → `anyhow!(...)`, `.ok_or("literal")` → `.ok_or_else(|| anyhow!(...))` | FIX-03: align error types with `anyhow::Error` return |
| `crates/vector/ops/qdrant/client.rs` | All `Box<dyn Error>` → `anyhow::Result` | FIX-02: consistent error type |
| `crates/core/config/parse.rs` | Add `#[allow(unsafe_code)]` to `test_tavily_api_key_read_from_env` (line 486) | Fix `unsafe_code = "deny"` failure |
| `crates/jobs/worker_lane.rs` | Add `#[allow(unsafe_code)]` to `validate_env_vars_passes_when_all_set` (line 576) and `validate_env_vars_requires_canonical_names` (line 599) | Fix `unsafe_code = "deny"` failure |
| `crates/cli/commands/status/metrics.rs` | `serde_json::Value` → `Value` in 4 function signatures | `unused_qualifications` fix |
| `crates/core/content.rs` | `spider::url::Url::parse` → `Url::parse`; `spider::tokio::sync::broadcast::` → `tokio::sync::broadcast::` | `unused_qualifications` fix |
| `crates/core/logging.rs` | Added `use std::path::PathBuf;`; `std::path::PathBuf` → `PathBuf`; `std::io::stderr` → `io::stderr` | `unused_qualifications` fix |
| `crates/jobs/crawl/runtime/worker/worker_loops.rs` | `std::time::Duration::from_secs(5)` → `Duration::from_secs(5)` | `unused_qualifications` fix |
| `crates/jobs/crawl/runtime/worker/worker_process.rs` | `std::path::PathBuf::from(d)` → `PathBuf::from(d)`; `std::collections::HashSet<String>` → `HashSet<String>` | `unused_qualifications` fix |
| `crates/jobs/extract/worker.rs` | `std::sync::Arc::new(...)` → `Arc::new(...)` | `unused_qualifications` fix |
| `crates/vector/ops/tei.rs` | `spider::url::Url::parse` → `Url::parse`; `chrono::Utc::now()` → `Utc::now()`; `uuid::Uuid::new_v5` → `Uuid::new_v5`; `uuid::Uuid::NAMESPACE_URL` → `Uuid::NAMESPACE_URL` | `unused_qualifications` fix |
| `lib.rs` | `self::crates::core::config::Config` → `Config` (3 occurrences in job-subcommand helpers) | `unused_qualifications` fix |
| `crates/cli/commands/search.rs` | `std::collections::HashSet<String>` → `HashSet<String>` in test fn | `unused_qualifications` fix (caught by pre-commit hook) |
| `crates/core/health.rs` | `std::env::` → `env::` (8 occurrences in test module) | `unused_qualifications` fix (caught by pre-commit hook) |
| `crates/crawl/engine/tests.rs` | `spider::website::Website::` → `Website::` (7 occurrences) | `unused_qualifications` fix (caught by pre-commit hook) |
| `rust-best-practices-fixes.md` | Created; FIX-01 through FIX-04 marked ✅ | Tracks 19 audit findings from full best-practices review |

---

## Commands Executed

| Command | Result |
|---------|--------|
| `cargo test --lib` (before unsafe fixes) | 332 passing, 6 errors (`unsafe_code = "deny"`) |
| `cargo test --lib` (after fixes) | **338 passing, 0 failed** |
| `cargo check` | 0 errors, 0 warnings |
| `cargo clippy --all-targets -- -D warnings` | **Finished clean** |
| `git add . && git commit ...` (first attempt) | Pre-commit hook failed: 18 `unused_qualifications` in `search.rs`, `health.rs`, `engine/tests.rs` |
| `cargo clippy --all-targets -- -D warnings` (after 18 fixes) | Clean |
| `git add . && git commit ...` (second attempt) | All hooks passed; commit `9129cf4` created |
| `git push` | Pushed `fix-crawl` → `origin/fix-crawl` (`faf3f56..9129cf4`) |

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| Workspace lint enforcement | No `[lints]` — clippy was a manual one-off | `[lints]` in `Cargo.toml` enforces lints on every `cargo clippy` run including CI |
| `unsafe_code` | Allowed silently everywhere | Denied workspace-wide; test fns with `unsafe {}` require explicit `#[allow(unsafe_code)]` |
| `crates/core/http.rs` error type | `validate_url` / `check_ip` returned `format!(...).into()` which boxed strings | Returns `anyhow::Error` with structured `anyhow!()` messages |
| `qdrant/client.rs` error type | All functions returned `Box<dyn Error>` | All functions return `anyhow::Result`; `?` chains and `.context()` are now available |
| `unused_qualifications` | 38+ redundant path prefixes silently present | All cleaned; future additions will fail clippy |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | 0 errors, 0 warnings | `Finished dev profile` | ✅ |
| `cargo test --lib` | 338 passing, 0 failed | `test result: ok. 338 passed` | ✅ |
| `cargo clippy --all-targets -- -D warnings` | Clean finish | `Finished dev profile` | ✅ |
| lefthook pre-commit: monolith | Pass (warning for `collect_page_results` 82L) | `✔️ monolith` | ✅ |
| lefthook pre-commit: rustfmt | Clean | `✔️ rustfmt` | ✅ |
| lefthook pre-commit: clippy | Clean | `✔️ clippy` | ✅ |
| `git push` | Remote updated | `faf3f56..9129cf4 fix-crawl -> fix-crawl` | ✅ |

---

## Source IDs + Collections Touched

None — this session contained no Axon embed/retrieve operations. No Qdrant collections were read or written.

---

## Risks and Rollback

- **Risk**: `unsafe_code = "deny"` may break future code that legitimately needs `unsafe {}` (e.g., FFI, raw pointer manipulation). Mitigation: use `#[expect(unsafe_code, reason = "...")]` (preferred over `allow`) with a justification comment.
- **Risk**: `unused_qualifications = "warn"` will surface in any future file that uses fully-qualified paths where an import exists. This is low-risk but requires developers to remove the prefix rather than use the fully-qualified form.
- **Rollback**: `git revert 9129cf4` would revert all changes. The `[lints]` section can also be removed from `Cargo.toml` independently without affecting logic.

---

## Decisions Not Taken

- **Did not apply FIX-05 through FIX-19**: The remaining 15 fixes in `rust-best-practices-fixes.md` (module naming, iterator improvements, `Cow<'_, str>` patterns, etc.) were not addressed — they are lower priority and require more invasive changes. Deferred to future sessions.
- **Did not add `pedantic` to `[lints.clippy]`**: The initial audit's `Cargo.toml` snippet included `pedantic = "warn"`, but it was omitted from the final `[lints]` to avoid hundreds of pedantic warnings that would need to be suppressed or fixed before the commit could land.
- **Did not change health.rs to use `unsafe {}`**: The test calls `env::set_var` without explicit `unsafe {}`. Adding them would be correct for Rust 1.86+ semantics but would also trigger the `unsafe_code = "deny"` lint, requiring `#[allow(unsafe_code)]`. Deferred pending confirmation of toolchain version.

---

## Open Questions

- **Toolchain version**: Does the current `rust-toolchain.toml` specify a version < 1.86? If yes, `env::set_var` is not yet `unsafe fn` and `health.rs` is fine as-is. If ≥ 1.86, `health.rs` test calls need `unsafe {}` blocks and `#[allow(unsafe_code)]`. (Was not checked this session.)
- **`collect_page_results` 82-line warning**: Monolith hook warns at 80 lines for this function in `content.rs:287`. It is below the hard limit (120) but worth splitting in a future session.
- **FIX-05 through FIX-19 priority**: The remaining 15 fixes in `rust-best-practices-fixes.md` are medium/low priority. No timeline set.

---

## Next Steps

1. Check `rust-toolchain.toml` for pinned Rust version; if ≥ 1.86, add `unsafe {}` + `#[allow(unsafe_code)]` to `health.rs` test functions.
2. Work through remaining items in `rust-best-practices-fixes.md` (FIX-05 onward) as a follow-up session.
3. Open PR to merge `fix-crawl` → `main` once crawl-related fixes are complete.
