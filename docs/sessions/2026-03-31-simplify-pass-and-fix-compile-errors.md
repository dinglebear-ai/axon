# Session: Simplify Pass + Compile Error Fixes (feat/lite-mode)

**Date:** 2026-03-31
**Branch:** `feat/lite-mode`
**Commits:** `035a7695` (fix), preceded by simplification pass from prior context

---

## Session Overview

Two-part session:
1. **Simplification pass** — ran `/simplify` with 8 parallel agents across all 31 files touched in the `feat/lite-mode` PR. Agents reviewed for code reuse, quality, and efficiency, then applied fixes directly.
2. **Compile error fixes** — resolved three blocking issues introduced by the simplification agents before successfully committing and pushing.

---

## Timeline

| Step | Activity |
|------|----------|
| 1 | Launched 8 parallel simplification agents covering all changed files |
| 2 | Agent 5 changed `build_graph_context` return type to `Box<dyn Error + Send + Sync>` |
| 3 | Agent 7 introduced `macro_rules! impl_noop_runtime_for!` in `crawl.rs` test mocks |
| 4 | Agents applied various quality fixes (see Files Modified) |
| 5 | Pre-commit hook caught rustfmt failure in `runners.rs` (two-line `.map_err`) |
| 6 | Pre-commit hook caught `#[async_trait]` / `macro_rules!` expansion error in `crawl.rs` |
| 7 | Pre-commit hook caught E0277 in `graph/context.rs:107` — `Taxonomy::resolve` type mismatch |
| 8 | Pre-commit hook caught E0277 in `graph/worker.rs:277` — inverse Send+Sync conversion |
| 9 | Pre-commit hook caught dead_code clippy warning for `get_watch_run` in `watch_lite.rs` |
| 10 | All errors fixed; commit `035a7695` succeeded; pushed to `origin/feat/lite-mode` |

---

## Key Findings

- **`#[async_trait]` + `macro_rules!`**: The proc macro receives token streams before inner `macro_rules!` invocations are resolved. A macro that generates async trait methods must wrap the entire `#[async_trait] impl` block — you cannot apply `#[async_trait]` as an outer attribute on an `impl` that delegates to an inner macro. Fix: `impl_noop_runtime_for!` macro emits the `#[async_trait]` attribute itself. (`crates/services/crawl.rs`)
- **`Box<dyn Error>` → `Box<dyn Error + Send + Sync>`**: No `From` impl exists for this direction. Use `.map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })`. (`graph/context.rs:107`)
- **`Box<dyn Error + Send + Sync>` → `Box<dyn Error>`**: Reverse direction also has no `From`. Use `.map_err(|e| -> Box<dyn Error> { e.to_string().into() })`. (`graph/worker.rs:277`)
- **`get_watch_run` dead code**: The `_with_pool` variant is called directly everywhere; the config-pool wrapper was never wired to a public API. Removed cleanly.

---

## Technical Decisions

- **Stringify errors across bound gap**: Using `e.to_string().into()` loses the error chain but is the pragmatic fix since the calling function has a different bound. The alternative (changing all callers to `Send+Sync`) would have cascading changes across the full-mode graph worker chain.
- **Remove `get_watch_run` wrapper entirely**: Rather than `#[allow(dead_code)]`, removal is correct — the function was a pool-opening wrapper never called by any public or test path. Consistent with the codebase's policy of not keeping dead abstractions.
- **Version bumped to `0.34.1`**: Patch bump for the simplification + fix pass (no new features or breaking changes).

---

## Files Modified

| File | Change |
|------|--------|
| `crates/jobs/graph/context.rs` | `.map_err` on `Taxonomy::resolve` to convert `Box<dyn Error>` → `Box<dyn Error + Send + Sync>` |
| `crates/jobs/graph/worker.rs` | `.map_err` on `process_graph_url` call to convert `Box<dyn Error + Send + Sync>` → `Box<dyn Error>` |
| `crates/jobs/watch_lite.rs` | Removed unused `get_watch_run` pool-opening wrapper |
| `crates/services/crawl.rs` | Restructured test mock macro to wrap complete `#[async_trait] impl` block |
| `crates/jobs/lite/workers/runners.rs` | Fixed two-line `.map_err(lift_err)?` to single line (rustfmt) |
| `crates/core/config/parse/build_config.rs` | `read_env()` helper extracted; `resolve_service_url()` extracted |
| `crates/core/health/doctor.rs` + `doctor/lite.rs` | `build_browser_runtime` made `pub(super)`, shared across doctor variants |
| `crates/jobs/lite/query.rs` | `service_select_from()` SQL fragment extracted |
| `crates/mcp/server.rs` | Redundant `"ui/resourceUri"` flat key removed; `unwrap_or_default()` on Option fixed |
| `crates/mcp/server/handlers_embed_ingest.rs` | `ingest_count()` bypass logic removed |
| `crates/vector/ops/ranking.rs` | `query_wants_low_signal_sources()` extracted as public function |
| `crates/vector/ops/commands/ask/context/heuristics.rs` | Wrappers replaced with direct `ranking::` calls |
| `Cargo.toml` | Version `0.34.0` → `0.34.1` |
| `CHANGELOG.md` | Added `[0.34.1]` section documenting simplification pass |

---

## Commands Executed

```bash
# Cargo check after fixes
rtk cargo check        # 0 errors, 1 warning (dead_code)
rtk cargo clippy       # 0 errors after removing get_watch_run

# Commit + push
rtk git add crates/jobs/graph/context.rs crates/jobs/graph/worker.rs
rtk git add crates/jobs/watch_lite.rs
rtk git commit -m "fix: resolve Box<dyn Error> Send+Sync bounds..."
# Result: 035a7695 — all 1692 tests passed, all hooks green
rtk git push           # ok feat/lite-mode
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Build | Compile error in `graph/context.rs` and `graph/worker.rs` | Clean compile, 0 errors |
| Clippy | `dead_code` warning for `get_watch_run` | No warnings |
| Code size | Duplicated error-conversion boilerplate in 2 places | Explicit `.map_err` one-liners |
| Test mock impls | Two near-identical `impl ServiceJobRuntime` blocks | Single `impl_noop_runtime_for!` macro invocation |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `rtk cargo check` | 0 errors | 0 errors, 1 warning (dead_code) | ✅ |
| `rtk cargo clippy` (after watch_lite fix) | 0 issues | No issues found | ✅ |
| Pre-commit `cargo check` | Pass | Pass | ✅ |
| Pre-commit `cargo test` (1692 tests) | All pass | All pass | ✅ |
| Pre-commit `rustfmt` | Pass | Pass | ✅ |
| Pre-commit `monolith` | Pass (warnings ok) | 3 warnings, passed | ✅ |
| `rtk git push` | `ok feat/lite-mode` | `ok feat/lite-mode` | ✅ |

---

## Source IDs + Collections Touched

None — this session involved code fixes and commits only. No embed/retrieve/RAG operations.

---

## Risks and Rollback

- **Risk**: Stringifying errors across `Box<dyn Error>` bounds loses the error chain. If graph job failures need to be inspected with `source()`, the cause is gone after `to_string()`.
- **Rollback**: `git revert 035a7695` restores previous state. The underlying compile error would need an alternative fix (e.g., changing the outer function's return type).
- **No user-visible behavior change** — this is purely a compile fix; runtime error messages may differ slightly in graph failure scenarios.

---

## Decisions Not Taken

- **Change all callers to `Box<dyn Error + Send + Sync>`**: Would propagate the bound change through `process_claimed_graph_job` and the full AMQP worker loop — too large a change for a compile fix.
- **`#[allow(dead_code)]` on `get_watch_run`**: Hiding the warning is worse than removing it — the function is genuinely unused.
- **Keep `Box<dyn Error>` return on `build_graph_context`**: Would require reverting Agent 5's change, losing the Send+Sync guarantee that enables cross-thread use.

---

## Open Questions

- The `get_watch_run_with_pool` function (kept) — is it actually called anywhere outside of `watch_lite.rs`? Should verify no external callers depend on it before the PR merges.
- Three monolith warnings remain (functions near the 80-line soft limit): `doctor/lite.rs:build()`, `retrieval.rs:retrieve_ask_candidates()`, `query.rs:query_results()`. Within policy but worth tracking.

---

## Next Steps

- Merge `feat/lite-mode` → `main` (PR ready, all hooks green)
- Address graph job error-chain loss if detailed error reporting becomes needed
- Consider splitting `retrieve_ask_candidates()` and `query_results()` before they exceed the 120-line hard limit
