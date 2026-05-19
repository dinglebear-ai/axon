# Session: Simplify Code Review
**Date:** 2026-03-20
**Branch:** feat/pulse-shell-and-hybrid-search
**Triggered by:** `/simplify` skill

---

## Session Overview

Ran a three-agent parallel code review (`/simplify`) over all unstaged changes on the current branch. Three specialized agents reviewed for code reuse, quality, and efficiency. Found and fixed 5 actionable issues: two hot-path blocking patterns, one pre-existing clippy warning, one stale test assertion, and review-artifact comment prefixes in bridge.rs.

---

## Timeline

1. **Identify changes** — `git diff --stat` revealed 60 modified files; full Rust diff (~157KB) saved to `/tmp/rust_diff.txt`
2. **Launch 3 review agents in parallel** — code-reuse, code-quality, efficiency agents ran concurrently against the diff
3. **Aggregate findings** — reviewed all three agent reports, verified by reading the actual source files
4. **Fix 1** — `scrape.rs`: `record_scrape_seed` converted from blocking await to `tokio::spawn` fire-and-forget
5. **Fix 2** — `search.rs`: `record_query_history` converted from blocking await to `tokio::spawn` at both call sites
6. **Fix 3** — `bridge.rs`: removed `FINDING-2:` and `FIX L-1:` review-artifact comment prefixes (4 locations)
7. **Fix 4** — `parse.rs`: `parse_completion_alias_is_rejected` test was asserting wrong behavior after `#[command(alias = "completion")]` was added; renamed and inverted
8. **Fix 5** — `export.rs`: manual `Default` impl replaced with `#[derive(Default)]` per clippy

---

## Key Findings

### Hot-path blocking (High severity)

- **`crates/services/scrape.rs:68`** — `record_scrape_seed(...).await?` was called after a successful scrape. If Postgres was unavailable, the user got an error even though the scrape itself succeeded.
- **`crates/services/search.rs:40-50` and `100-111`** — `record_query_history(...).await?` was called *before* the Tavily search. Worst-case DB retry loop (3 × 5s timeout + backoff) could block search results by up to ~15 seconds.
- Both functions created a fresh `PgPoolOptions` pool per call (no shared pool), compounding latency.

### Pre-existing test failure

- **`crates/core/config/parse.rs:483`** — `parse_completion_alias_is_rejected` asserted `axon completion zsh` returns `Err`, but `#[command(alias = "completion")]` was added to the `Completions` subcommand in this branch, making it valid. Test was stale.

### Review-artifact comments (Low severity)

- **`crates/services/acp/bridge.rs:28,59,152,242`** — Comments prefixed with `FINDING-2:` and `FIX L-1:` left over from a prior code review; these narrate the review process rather than document the code.

### Clippy warning (pre-existing)

- **`crates/services/export.rs:42`** — Manual `Default` impl for `ExportOptions` where `#[derive(Default)]` suffices.

---

## Technical Decisions

### Fire-and-forget via `tokio::spawn`, not `spawn_blocking`

History recording is supplementary — a DB failure should never surface as a user-facing error. `tokio::spawn` was chosen over `spawn_blocking` because the functions are already `async`. The function signatures were changed from `&Config` → owned `Config` (required for `'static` bound on `spawn`), and return types changed from `Result<(), Box<dyn Error>>` to `()` with internal `log_warn` on failure.

### Retaining the retry loop

The 3-attempt retry with 5s timeout inside `record_scrape_seed` and `record_query_history` was kept — it's still useful for transient DB hiccups. The key change is that the caller no longer waits for it.

### `synthesize()` nested runtime left unchanged

The efficiency agent flagged `search.rs:274-283` (creating a new `tokio::runtime::Builder` inside `spawn_blocking`). This pattern is intentional — `acp_llm::complete_text` requires a `current_thread` + `LocalSet` runtime because the ACP bridge uses `RefCell` (`!Send` types). Calling it directly from the multi-threaded Tokio executor would panic. Left as-is.

### Test renamed, not deleted

`parse_completion_alias_is_rejected` was renamed to `parse_completion_alias_is_accepted` rather than deleted, preserving test coverage for the alias behavior.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/services/scrape.rs` | `record_scrape_seed`: changed signature to owned args, return `()`, spawn fire-and-forget |
| `crates/services/search.rs` | `record_query_history`: changed return to `()`, spawn fire-and-forget at 2 call sites |
| `crates/services/acp/bridge.rs` | Removed `FINDING-2:` and `FIX L-1:` comment prefixes at 4 locations |
| `crates/core/config/parse.rs` | Fixed stale test: `parse_completion_alias_is_rejected` → `parse_completion_alias_is_accepted` |
| `crates/services/export.rs` | `ExportOptions`: replaced manual `Default` impl with `#[derive(Default)]` |

---

## Commands Executed

```bash
git diff --stat                          # 60 modified files, ~157KB Rust diff
cargo check --lib                        # confirmed compile errors after initial spawn attempt
cargo clippy --lib                       # 1 warning: derivable_impls in export.rs
cargo fmt --check                        # clean
cargo test --lib                         # 1443 passed, 0 failed (after all fixes)
git stash / git stash pop                # confirmed parse_completion test failure was branch-introduced
```

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| Scrape with Postgres down | `axon scrape <url>` returns error even if scrape succeeded | Scrape returns successfully; DB record failure logged as warning |
| Search with slow Postgres | Tavily search blocked up to ~15s waiting for history record | Search returns immediately; history recorded in background |
| `axon completion zsh` | Test expected this to fail (test was wrong; command worked) | Test correctly asserts command succeeds |
| `ExportOptions::default()` | Manual impl, clippy warning | `#[derive(Default)]`, clippy clean |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --lib` | No errors | No errors | ✓ |
| `cargo clippy --lib` | 0 warnings | 0 warnings | ✓ |
| `cargo fmt --check` | Clean | Clean | ✓ |
| `cargo test --lib` | All pass | 1443 passed, 0 failed | ✓ |

---

## Source IDs + Collections Touched

None — this session contained no `axon embed`, `axon crawl`, or `axon query` operations.

---

## Risks and Rollback

**Fire-and-forget history recording** — If the spawn is dropped before the DB write completes (e.g., process exits immediately), history records may be lost. This is acceptable: history tables are supplementary analytics, not critical state. Rollback: revert `scrape.rs` and `search.rs` to `await?` pattern.

**Test inversion** — The test rename is safe. The underlying command behavior (alias works) was already the actual behavior; only the test assertion was wrong.

---

## Decisions Not Taken

- **Extract shared retry helper** — The duplicate retry-with-timeout loop in `scrape.rs` and `search.rs` could be extracted to a utility. Deferred: the two loops have different table schemas and inner operations, making abstraction add complexity without reducing real duplication.
- **Shared PgPool** — Both history functions create a fresh pool per call. A shared pool (e.g., passed via Arc or LazyLock) would be more efficient. Deferred: these are background tasks that run at low frequency; the overhead is acceptable.
- **Remove `synthesize` nested runtime** — Flagged by efficiency agent but intentional due to ACP's `!Send` constraint; left unchanged.

---

## Open Questions

- Should history recording (scrape seeds, query history) silently drop on failure, or should it be surfaced somewhere (e.g., a dedicated error log, Postgres-unavailable warning on startup)?
- Are there other `await?` calls in the services layer that block primary operations on non-critical supplementary writes?

---

## Next Steps

- Audit remaining service functions for similar "blocking on supplementary write" patterns
- Consider whether the shared retry logic warrants a `crates/services/db_util.rs` helper once a third caller appears
