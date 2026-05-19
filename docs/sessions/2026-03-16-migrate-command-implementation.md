# Session: axon migrate Command Implementation
Date: 2026-03-16
Branch: feat/pulse-shell-and-hybrid-search

## Session Overview

Completed and landed the `axon migrate --from <src> --to <dst>` command. This enables migrating a legacy unnamed-vector Qdrant collection to a named-mode collection (dense + bm42 sparse), unlocking RRF hybrid search without re-crawling or re-embedding. Also ran a Rust code review on the migration code and fixed one quality issue.

---

## Timeline

1. **Session start** — Continued from prior context. Task 3 commit (`crates/cli/commands/migrate.rs`) had been staged but failed pre-commit hooks multiple times.
2. **Diagnosed repeated commit failures** — `qdrant_scroll_pages_with_vectors` in `client.rs` was flagged as dead code. The re-export in `qdrant.rs` generated `unused_imports`. Root cause: `migrate.rs` uses a direct HTTP scroll loop (not the callback-based helper), so the function was genuinely unused even though a test called it — `#[cfg(test)]` usage doesn't count for non-test clippy runs.
3. **Fixed by deleting the unused function** — Removed `qdrant_scroll_pages_with_vectors` from `client.rs`, its re-export from `qdrant.rs`, and the integration test from `tests.rs`. Commit `3d40b219` landed cleanly.
4. **CLAUDE.md update** — Added `migrate` to the commands table and a Gotchas entry. Commit `b717e441`.
5. **Checked actual collection size** — `axon stats` showed **7,063,563 points** in `cortex`. Prior session memory had stale figure of 2.57M.
6. **Code review** (`beagle-rust:rust-code-review`) — Identified one Minor issue: `unwrap_or(0.0)` silently zero-filled malformed vector elements instead of returning an error.
7. **Fixed the issue** — Changed to `ok_or_else(|| format!("vector element {i} is not a number: {v}"))` + `collect::<Result<_,_>>()?`. Commit `67351322`.

---

## Key Findings

- **7,063,563 points** in `cortex` collection (not 2.57M — that figure was stale in memory).
- **`#[cfg(test)]` usage does NOT suppress dead_code lints** when clippy runs without `--tests`. A `pub(crate)` function only called from a test block is dead from clippy's perspective.
- **`qdrant_scroll_pages_with_vectors` was never actually needed** — the design switched to a direct async scroll loop in `run_migrate()` to allow `await`-ing upserts between pages (sync callback can't `await`).
- **New collections auto-create as named-mode** — `ensure_collection()` in `tei/qdrant_store.rs` creates dense + bm42 for any new collection. No special flag needed; just use `--collection <new-name>`.
- **Migration is idempotent** — If destination already exists as named-mode, migration re-upserts with fresh BM42 sparse vectors without error.

---

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Direct scroll loop in `run_migrate()` instead of `qdrant_scroll_pages_with_vectors` callback | Sync `FnMut` callback can't `await` — needed async upsert after each page |
| Delete `qdrant_scroll_pages_with_vectors` entirely | Function was dead code (no consumer outside tests). Keeping it with `#[allow(dead_code)]` is noise; cleaning is better |
| `ok_or_else` on vector element parse | Silent `0.0` substitution could produce subtly wrong embeddings; an error + skip is safer for production data |
| `Box<dyn Error>` throughout | CLI command boundary — consistent with all other command handlers in the codebase |

---

## Files Modified

| File | Change |
|------|--------|
| `crates/cli/commands/migrate.rs` | **Created** — `run_migrate()`, `inspect_source_collection()`, `ensure_named_collection()`, `transform_point()`, `upsert_batch_raw()`, 5 unit tests |
| `crates/cli/commands.rs` | Added `pub mod migrate` + `pub use migrate::run_migrate` |
| `lib.rs` | Added `run_migrate` import + `CommandKind::Migrate => run_migrate(cfg).await?` dispatch arm |
| `crates/core/config/types/enums.rs` | Added `Migrate` variant + `as_str()` arm |
| `crates/core/config/cli.rs` | Added `MigrateArgs { from, to }` struct + `CliCommand::Migrate(MigrateArgs)` variant |
| `crates/core/config/parse/build_config.rs` | Added `CliCommand::Migrate(args) => (CommandKind::Migrate, vec![args.from, args.to])` |
| `crates/vector/ops/qdrant/client.rs` | Removed `qdrant_scroll_pages_with_vectors` (added then removed this session) |
| `crates/vector/ops/qdrant/tests.rs` | Removed `qdrant_scroll_pages_with_vectors_returns_vector_data` test |
| `crates/vector/ops/qdrant.rs` | Removed re-export of deleted function |
| `CLAUDE.md` | Added `migrate` to commands table + Gotchas section |

---

## Commands Executed

```bash
# Check actual point count
axon stats  # → 7,063,563 points in cortex

# Verify compilation
cargo check  # clean

# Run unit tests
cargo test migrate::tests  # 5 tests pass

# Commits
git commit -m "feat(config): add Migrate subcommand to CLI and CommandKind"  # f5152f89
git commit -m "feat(migrate): add axon migrate command for unnamed→named collection migration"  # 3d40b219
git commit -m "docs: document migrate command in CLAUDE.md"  # b717e441
git commit -m "fix(migrate): error on malformed vector elements instead of silent zero-fill"  # 67351322
```

---

## Behavior Changes (Before/After)

| Before | After |
|--------|-------|
| `axon migrate` → "command not yet implemented" | `axon migrate --from cortex --to cortex_v2` scrolls source, computes BM42 locally, upserts to named-mode dest |
| Malformed vector elements silently → `0.0` | Malformed elements → error logged, point skipped |
| `cortex` collection stuck on dense-only `/points/search` | After migration, `cortex_v2` uses `/query` with RRF hybrid search |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | No errors | 0 errors | ✅ |
| `cargo test migrate::tests` | 5 pass | 5 pass, 0 failed | ✅ |
| Pre-commit hooks on `3d40b219` | All pass | All 12 hooks pass | ✅ |
| Pre-commit hooks on `67351322` | All pass | All hooks pass | ✅ |
| `axon migrate --help` | Shows `--from`, `--to` | (not verified — binary not rebuilt yet) | ⏳ |
| `axon migrate --from cortex --to cortex_v2` | Migrates 7M points | Not yet run | ⏳ |

---

## Source IDs + Collections Touched

No Axon embeds or queries were performed during this session. The `cortex` collection was read (stats) but not modified. `cortex_v2` does not exist yet.

---

## Risks and Rollback

| Risk | Mitigation |
|------|-----------|
| Migration fails mid-run | Restart is safe — upserts are idempotent by point ID. Partial destination is valid state. |
| `cortex_v2` has wrong dim | `inspect_source_collection()` reads dim from source schema before creating dest |
| Old code hitting `cortex` after migration | `AXON_COLLECTION` env var stays as `cortex` until manually changed — no auto-switch |
| Named collection detection via JSON pointer brittle | Pointer `/result/config/params/vectors/dense` matches Qdrant v1.13.1 schema; test against actual API before upgrading Qdrant |

**Rollback:** Delete `cortex_v2`, keep `AXON_COLLECTION=cortex`. Migration leaves `cortex` untouched.

---

## Decisions Not Taken

- **Re-use `qdrant_scroll_pages_with_vectors` callback** — Can't `await` inside sync `FnMut`. Would require changing the callback signature to `async` or boxing futures, adding complexity for no gain.
- **Batch-parallel upserts** — Processing pages sequentially is simpler and safer. Parallel upserts would risk ordering issues and complicate error handling for a one-time operation.
- **Progress spinner** — Consistent with other long-running commands; progress goes to log file. User tails `logs/axon.log`.
- **Resume/checkpoint** — Idempotent upserts make restart cheap enough. A checkpoint would require storing state (offset cursor) somewhere persistent.

---

## Open Questions

- **Does hybrid search actually improve result quality?** — Not benchmarked yet. Need to run the test collection experiment (`cortex_hybrid_test`) to compare RRF vs dense-only results.
- **Migration duration for 7M points** — Estimated 4–5 hours at 256 points/page × ~27,500 pages. Not yet validated against real throughput.
- **Qdrant `next_page_offset` behavior at exact collection size** — If total points is a multiple of 256, the last page returns 256 points and a null offset. Tested in unit tests but not at production scale.
- **BM42 quality for code chunks** — `chunk_text` for GitHub-ingested source code may produce low-quality sparse vectors (code tokens don't match BM42's stopword list well). Impact on hybrid search quality unknown.

---

## Next Steps

1. **Build binary and verify `--help`**: `cargo build --release --bin axon && ./target/release/axon migrate --help`
2. **Test hybrid search on a small collection**: `./scripts/axon scrape <url> --collection cortex_hybrid_test --wait true` then query
3. **Run the actual migration**: `./scripts/axon migrate --from cortex --to cortex_v2` (plan for 4–5 hours, tail logs)
4. **After migration**: set `AXON_COLLECTION=cortex_v2` in `.env`, verify with `axon query` and `axon ask`
5. **Update memory files**: `memory/MEMORY.md` still references 2.57M points — update to 7,063,563
