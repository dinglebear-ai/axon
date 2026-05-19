# Session: Query Snippet Extraction Rewrite

**Date:** 2026-02-20
**Branch:** `perf/command-performance-fixes`
**Duration:** ~1 session

---

## Session Overview

Rewrote the query result snippet extraction in the Rust CLI (`axon query`) from a naive first-140-chars approach to a full port of the TypeScript `getMeaningfulSnippet` algorithm from `~/workspace/axon/src/utils/snippet.ts`. The new implementation strips markdown formatting, splits into sentences, scores each sentence against query terms, and reassembles the top 3–5 sentences in original document order. Also fixed a pre-existing test failure caused by module directory refactors.

---

## Timeline

1. **Problem identified** — User ran `axon query "/home/jmagar/workspace/spider"` and got snippets like `[Spider Platform](https://spider.cloud/guides/spider)` and `*   ### [Spider Platform](...)` — raw markdown nav links, useless for humans.
2. **Initial fix (line-scanning)** — Replaced the 140-char truncation with a line scanner that strips leading markdown and finds the first line with ≥20 alphabetic chars. Tests updated.
3. **Pre-existing test failure fixed** — `tests/vector_v2_no_legacy_calls.rs` referenced `commands/ask.rs` and `stats.rs` which had been refactored to `commands/ask/` and `stats/` subdirectories. Updated all `include_str!` paths to match the current directory layout.
4. **Full TypeScript port** — User showed `~/workspace/axon/src/utils/snippet.ts` as the reference. Replaced the line-scanner with a faithful port: `clean_snippet_source`, `is_relevant_sentence`, `split_into_sentences`, `extract_query_terms`, `score_sentence_for_query`, `get_meaningful_snippet`.
5. **Bug fixes post-port** — User ran `axon query "zed docs"` and got `…` snippets. Identified three bugs: (a) `…` always appended even to empty fallback, (b) missing `@#handle` and all-symbols guards in `is_relevant_sentence`, (c) last-resort fallback never tried raw `text` when `cleaned` was empty. All three fixed.

---

## Key Findings

- **Root cause of `…` snippets:** `format!("{}…", &fallback[..end])` was unconditional — when `cleaned` was empty (all-nav content), `fallback` was `""`, producing literal `"…"`. (`utils.rs:370`, now fixed)
- **Missing TS guards in `is_relevant_sentence`:** TypeScript checks `^[@#][a-z0-9_-]+$` (handles/hashtags) and `^[^a-zA-Z0-9]+$` (all-symbols). Both were absent from the initial Rust port.
- **`ask` and `stats` modules now subdirectories:** `commands/ask/` has `mod.rs` + `context.rs`; `stats/` has `mod.rs` + `postgres.rs` + `display.rs`. The no-legacy-calls test was referencing the old flat-file paths.
- **Sentence splitter uses `. ! ?` + whitespace:** Chosen over regex (no `regex` crate in direct deps) and matches the TS `(?<=[.!?])\s+` lookbehind closely enough for snippet quality.
- **`Vec<char>` collection per call:** Acceptable for display-only path; not in any hot embed/search loop.

---

## Technical Decisions

| Decision | Rationale | Alternative Rejected |
|---|---|---|
| Port TS algorithm verbatim | Reference implementation is proven in production TS CLI; consistency across CLIs | Custom Rust-native approach — unnecessary divergence |
| No `regex` crate | Not a direct dep; char-iteration is transparent and avoids dep churn | Add `regex` — would require Cargo.toml change and user approval |
| 25-char / 5-word threshold (matching TS) | Proven to filter nav labels (≤2 words) while keeping prose sentences | Lower threshold — would let short nav fragments through |
| `get_meaningful_snippet` exported as `pub` | Allows direct testing without going through `query_snippet` wrapper | `pub(crate)` — blocked integration tests from using it |
| `…` only on actual truncation | Matches `truncateWithMarker` semantics from TS; avoids confusing empty-ellipsis | Always append — confusing UX when content is short |

---

## Files Modified

| File | Change |
|---|---|
| `crates/vector/ops/qdrant/utils.rs` | Full rewrite of snippet section: added `clean_inline_markdown`, `clean_snippet_source`, `is_relevant_sentence`, `split_into_sentences`, `STOP_WORDS`, `extract_query_terms`, `score_sentence_for_query`, `get_meaningful_snippet`; updated `query_snippet` signature to `(payload, query: Option<&str>)` |
| `crates/vector/ops/qdrant/mod.rs` | Added `get_meaningful_snippet` to public exports |
| `crates/vector/ops/commands/query.rs` | Updated call site: `qdrant::query_snippet(payload, Some(&query))` |
| `tests/vector_v2_qdrant_migration.rs` | Replaced 3 old tests with 4 new tests covering: prose detection, markdown stripping, query scoring, fallback |
| `tests/vector_v2_no_legacy_calls.rs` | Fixed `include_str!` paths for `commands/ask/{mod,context}.rs`, `commands/evaluate.rs`, `stats/{mod,postgres,display}.rs` |

---

## Commands Executed

```bash
# Verify directory layout after module refactors
ls crates/vector/ops/commands/ask/   # → context.rs, mod.rs
ls crates/vector/ops/stats/          # → display.rs, mod.rs, postgres.rs

# Confirm no regex crate in direct deps
grep "regex" Cargo.toml                 # → only as spider feature flag

# Full test suite after each change
cargo test
```

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|---|---|---|
| Nav-heavy chunk (e.g. claude.com redirect) | `…` | Empty string or raw-text line (no fake ellipsis) |
| Chunk starts with `* [Link](url)` | `[Link](url)` (raw markdown) | Skipped; prose sentence chosen instead |
| Query "spider rust" with matching sentence | First 140 chars (often irrelevant) | Sentence containing "spider" and "rust" surfaced |
| Short nav-only content | `…` | Empty string |
| Long prose, no query | First 140 chars | First 3–5 relevant sentences, ≤700 chars |
| All-symbols or `@handle` line | Treated as prose | Filtered by `is_relevant_sentence` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo test` | 107 tests pass | 107 passed, 0 failed | ✅ |
| `cargo test --test vector_v2_no_legacy_calls` | 1 pass | 1 passed | ✅ |
| `cargo test --test vector_v2_qdrant_migration` | 4 pass | 4 passed | ✅ |
| `axon query "zed docs"` | No `…` snippets | Snippet shows actual content for zed.dev result | ✅ (manual) |

---

## Source IDs + Collections Touched

None — this session was pure code changes, no embed/crawl/query operations that modified the vector store.

---

## Risks and Rollback

- **Risk:** Sentence splitter on `. ` may over-split on abbreviations (e.g. `Mr. Smith` → two fragments). Impact is cosmetic (shorter snippets), not correctness. Mitigation: `is_relevant_sentence` filters fragments < 25 chars or < 5 words.
- **Risk:** `clean_snippet_source` strips aggressively — content that is _intentionally_ structured as lists may lose all sentences and fall through to the raw-text last resort. This is acceptable (raw text > empty).
- **Rollback:** `git checkout HEAD -- crates/vector/ops/qdrant/utils.rs crates/vector/ops/commands/query.rs crates/vector/ops/qdrant/mod.rs tests/vector_v2_qdrant_migration.rs tests/vector_v2_no_legacy_calls.rs`

---

## Decisions Not Taken

- **Add `regex` crate** — Would simplify the char-iteration loops in `clean_inline_markdown` and `split_into_sentences`, but adds a direct dependency. TS port works without it.
- **Port `selectBestPreviewItem`** — The TS query command uses this to pick the _best chunk_ from a URL group before extracting a snippet. The Rust command currently iterates all hits individually (no URL grouping), so this isn't needed yet.
- **Score-band coloring** — TS shows `✓ [0.74]` in green vs `⚑ [0.55]` in yellow based on score thresholds. Not ported; Rust CLI uses `completed` status text uniformly.

---

## Open Questions

- Why are `claude.com/redirect/...` URLs scoring 0.74 for `"zed docs"`? Those chunks are likely from a "powered by Claude" badge page that was crawled and embeds text near the query terms semantically. Potentially needs domain filtering or deduplication in the Qdrant search step.
- Should `query_snippet` be deprecated in favor of callers using `get_meaningful_snippet` directly (passing `chunk_text` as a string rather than the full payload)?
- The TS CLI has `selectBestPreviewItem` for multi-chunk URL groups — the Rust CLI doesn't group by URL. Is that a gap worth closing?

---

## Next Steps

- Consider porting `selectBestPreviewItem` (chunk-selection by preview score) if the Rust CLI adds URL grouping to query output.
- Investigate the claude.com redirect URL flooding in "zed docs" results — likely a crawl scope issue, not a snippet issue.
- `cargo clippy` + `cargo fmt` before merging the branch.
