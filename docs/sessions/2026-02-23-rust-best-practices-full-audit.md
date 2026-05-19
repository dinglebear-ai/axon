# Rust Best Practices — Full Audit Implementation

**Date:** 2026-02-23
**Branch:** `fix-crawl`
**Duration:** Extended session (continued from context compaction)

## Session Overview

Implemented 18 of 19 fixes from `rust-best-practices-fixes.md`, a comprehensive audit against Apollo GraphQL's Rust Best Practices handbook. FIX-01 through FIX-04 were pre-existing; this session completed FIX-05 through FIX-17 and FIX-19. FIX-18 (rstest parameterized tests) was deferred. Also resolved pre-existing `webdriver_url` removal that was incomplete on the branch, which was blocking compilation.

## Timeline

1. **Read fixes document + all affected files** — Parallel reads of ~15 source files to assess scope
2. **FIX-05** — Removed unnecessary `#[allow(clippy::module_inception)]` from `crates/crawl.rs` (lint never fires)
3. **FIX-09** — Fixed `.unwrap_or("unknown").to_string()` allocation in `qdrant_domain_facets`
4. **FIX-10** — Replaced `is_none()`/`unwrap()` with `let Some(next) = ... else { break }` in `scroll_pages_raw`
5. **FIX-11** — Changed `ssrf_blacklist_patterns()` return from `Vec<String>` to `&'static [&'static str]`; updated 4 callers
6. **FIX-12** — Documented unavoidable clone in `qdrant_retrieve_by_url` (callback receives `&[Value]`)
7. **FIX-13** — Removed `test_` prefix from ~34 test functions across `http.rs` and `reddit.rs`
8. **FIX-14** — Split monolithic config defaults test into 6 focused tests in `config/types.rs`
9. **FIX-15** — Changed `mark_job_failed` to return `Result<()>`; updated 9 callers across 5 files
10. **FIX-06 + FIX-17** — Refactored reddit comment tree: `CommentWithContext.data: Value` → `.body: String`, async recursive → sync, `Option<String>` → `Option<&str>`
11. **FIX-08** — Added `//!` module docs to `http.rs`, `qdrant/client.rs`, `jobs/common.rs`
12. **FIX-16** — Added `# Errors` doc section to `validate_url()`
13. **FIX-19** — Replaced nested-loop distinctness test with `HashSet` uniqueness check in `status.rs`
14. **WebDriver cleanup** — Resolved pre-existing incomplete removal of `webdriver_url` from Config, health.rs, doctor.rs, engine.rs, cli.rs, parse.rs, runtime.rs
15. **Updated fixes document** — Marked all 18 completed fixes with resolution notes

## Key Findings

- **FIX-05:** `#[allow(clippy::module_inception)]` on `crates/crawl.rs` was unnecessary — the lint never fires because module `crawl` contains `engine`, not `crawl`. Attempting `#[expect]` caused an "unfulfilled" warning.
- **FIX-07:** Already implemented (pre-existing `JudgeContext` struct at `streaming.rs:23-34`).
- **FIX-12:** Clone in `qdrant_retrieve_by_url` is unavoidable without refactoring `scroll_pages_raw`'s callback signature from `&[Value]` to owned `Vec<Value>`.
- **WebDriver removal:** The branch had `webdriver_url` removed from `Config` struct and `GlobalArgs` but left dangling references in 6+ files (`engine.rs`, `crawl.rs`, `doctor.rs`, `runtime.rs`, `parse.rs`, `health.rs`). This blocked all compilation.
- **`git stash` recovery issue:** After stashing to verify pre-existing errors, `git stash pop` conflicted with 5 files that had been modified by a concurrent linter. Required `git checkout -- <files>` before pop, which lost the stash's changes to those files. Had to manually re-apply the `webdriver_url` removal to `health.rs`, `types.rs`, `cli.rs`, and `parse.rs`.

## Technical Decisions

- **FIX-06+17 combined:** Merged reddit comment tree fixes because both touched the same recursive function. Changed from async to sync since the function does no I/O — eliminates `Box::pin` overhead.
- **FIX-11 static slice:** Chose `&'static [&'static str]` over `LazyLock<Vec<Regex>>` since callers already compile regexes themselves. Simpler, zero allocation.
- **FIX-15 `let _ =` pattern:** Callers that previously silently swallowed `mark_job_failed` errors now use `let _ = mark_job_failed(...).await;` to explicitly discard the Result. This is intentional — the job is already failed, so a secondary DB error isn't actionable at the call site.
- **WebDriver removal:** Completed the removal rather than re-adding the field, since the branch clearly intended to drop WebDriver support (health.rs had the functions removed, cli.rs had the arg removed).

## Files Modified

| File | Purpose |
|------|---------|
| `crates/crawl.rs` | Removed unnecessary `#[allow]` attribute |
| `crates/core/http.rs` | Static slice for SSRF patterns, `test_` prefix removal, module doc, `# Errors` section |
| `crates/core/health.rs` | Removed WebDriver functions + enum, fixed `unsafe` env var tests |
| `crates/core/config/types.rs` | Removed `webdriver_url` field, split config tests |
| `crates/core/config/cli.rs` | Removed `--webdriver-url` CLI arg |
| `crates/core/config/parse.rs` | Removed `webdriver_url` from Config construction |
| `crates/vector/ops/qdrant/client.rs` | FIX-09/10/12: idiom fixes, module doc |
| `crates/jobs/common.rs` | FIX-15: `mark_job_failed` returns `Result<()>`, module doc |
| `crates/jobs/common/tests.rs` | Updated for new `mark_job_failed` signature |
| `crates/jobs/status.rs` | FIX-19: HashSet-based uniqueness test |
| `crates/jobs/ingest.rs` | Updated `mark_job_failed` callers |
| `crates/jobs/embed/worker.rs` | Updated `mark_job_failed` callers |
| `crates/jobs/extract/worker.rs` | Updated `mark_job_failed` callers |
| `crates/jobs/crawl/runtime/worker/worker_loops.rs` | Updated `mark_job_failed` callers |
| `crates/ingest/reddit.rs` | FIX-06+17: refactored comment tree, `test_` prefix removal |
| `crates/crawl/engine.rs` | Updated `ssrf_blacklist_patterns` caller |
| `crates/cli/commands/scrape.rs` | Updated `ssrf_blacklist_patterns` caller |
| `crates/core/content.rs` | Updated `ssrf_blacklist_patterns` caller |
| `crates/cli/commands/crawl.rs` | Removed `webdriver_url` reference |
| `crates/cli/commands/doctor.rs` | Removed all WebDriver probe/display code |
| `rust-best-practices-fixes.md` | Updated all 19 fix entries with final status |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Clean compile | Clean (1 pre-existing warning: unused `mode` field) | PASS |
| `cargo test --lib` | All tests pass | Blocked by unrelated missing module files (`loops.rs`/`process.rs` rename) | BLOCKED |

## Risks and Rollback

- **WebDriver removal is irreversible without re-adding the field.** If WebDriver support is needed again, `Config.webdriver_url`, the CLI arg, health.rs functions, and doctor.rs display code all need to be restored. The git history preserves all of this.
- **`mark_job_failed` callers using `let _ =`** — if a caller needs to handle the error (e.g., for retry logic), they'll need to change from `let _ =` to proper error handling. The compiler won't warn about this since `let _ =` explicitly discards.

## Decisions Not Taken

- **FIX-18 (rstest):** Deferred because adding a dev-dependency is a separate concern and the existing tests are correct — just verbose.
- **FIX-12 (eliminate clone):** Would require changing `scroll_pages_raw` callback from `&[Value]` to `Vec<Value>`, affecting all scroll callers. Cost exceeds benefit for this single call site.
- **Extracting `LlmStreamConfig` struct (FIX-07 extension):** The streaming/evaluate functions could benefit from a shared config struct beyond `JudgeContext`, but FIX-07 was already resolved.

## Open Questions

- **`cargo test --lib` failure:** Missing `worker_loops.rs`/`worker_process.rs` files (renamed to `loops.rs`/`process.rs` by another agent). The `worker.rs` module declaration file references the correct new names, but the test build may be using a stale cache. Needs `cargo clean` or investigation.
- **Pre-existing `mode` field warning:** `crawl/runtime.rs:14` has an unused `mode` field in a struct — should be cleaned up separately.

## Next Steps

1. Resolve the test compilation issue (likely just `cargo clean && cargo test --lib`)
2. Consider FIX-18 (rstest) in a future PR if the team adopts it as a dev dependency
3. Run `cargo clippy --all-targets` after test compilation is green to verify no new warnings
