# Session: TypeScript Lexical Pattern Port to Rust

**Date:** 2026-02-20
**Branch:** `perf/command-performance-fixes`
**Duration:** ~30 minutes

---

## Session Overview

User shared the TypeScript `query.ts` from `~/workspace/axon` (the Node.js sibling) and asked whether there were additional lexical patterns in it worth porting to the Rust `ops` implementation. Four distinct patterns were identified and all were shipped in this session.

---

## Timeline

| Time | Activity |
|------|----------|
| T+0  | User shows `query.ts` + `utils/snippet.ts` from TS axon repo |
| T+5  | Identified four porting targets: stop words, phrase boost, snippet quality, best-preview chunk selection |
| T+10 | First commit of session: extended stop words + phrase boost + `get_meaningful_snippet` + 8× overfetch (committed by previous agent turn) |
| T+15 | User asks "any more?" — three remaining gaps identified: word-count threshold, fallback line filtering, `selectBestPreviewItem` |
| T+25 | All three gaps implemented and `cargo check` passed |

---

## Key Findings

- **`is_relevant_sentence` off-by-one**: TS uses `split.length < 5` (requires ≥5 words); Rust had `>= 4` (accepts 4-word sentences TS would discard). `ranking.rs:312`
- **Fallback truncation quality**: When no prose sentences are found, the old Rust fallback took the first 220 chars of the raw cleaned blob — risking mid-word cuts on navigation text. TS filters to prose lines ≥20 chars with alphabetic content first. `ranking.rs:350-360`
- **`selectBestPreviewItem` gap**: The Rust query display always showed the snippet from the *highest-reranked* chunk for each URL. The TS instead scores all chunks for that URL on *prose richness × query relevance* and picks the most readable one — which may be a lower-vector-score chunk. `ranking.rs:177-225`
- **`scoreChunkForPreview` formula**: `relevance_score * 10 + richness`, where richness = `min(sentences, 5) * 2 + min(chars, 500) / 100`. This is intentionally orthogonal to vector score.

---

## Technical Decisions

### Why separate `score_chunk_for_preview` from `rerank_score`
The vector score + lexical/docs boost in `rerank_score` picks *which URL* to surface. The preview score picks *which chunk* of that URL to show as a snippet. A dense API reference block may rank highest vectorially but have terrible prose. A slightly lower-scoring chunk may have five rich explanatory sentences. Keeping these concerns separate avoids conflating retrieval quality with display quality.

### Why scan up to 8 candidates per URL
The TS reference impl caps at 8. It balances coverage (enough chunks to find the best prose) against cost (scoring 8 chunks per URL × O(sentences) per chunk is cheap). The full candidate pool per URL could be much larger on heavily-crawled docs.

### Why `fallback_idx` as a parameter to `select_best_preview_chunk`
The caller (`query.rs`) knows the hit index directly. Passing it as a fallback avoids a second linear scan through the candidates when the URL isn't found (shouldn't happen in practice but keeps the function safe).

---

## Files Modified

| File | Change |
|------|--------|
| `crates/vector/ops/ranking.rs` | `is_relevant_sentence`: word count 4 → 5; fallback prose-line filtering; `score_chunk_for_preview`; `select_best_preview_chunk` |
| `crates/vector/ops/commands/query.rs` | Display loop wired to `select_best_preview_chunk` for snippet source selection |

---

## Changes Previously Made (Same Branch, Prior Turn)

These were landed before this session's Q&A began but are part of the same PR:

| Change | Location |
|--------|----------|
| Extended stop words (+27): "use", "using", "used", "get", "set", "via", "not", "all", "any", "but", "too", "out", "our", "their", "them", "they", "its", "then", "than", "also", "have", "has", "had", "was", "were", "who", "why" | `ranking.rs:12-18` |
| Phrase boost (+0.06) in `rerank_ask_candidates` when joined query tokens appear verbatim in chunk | `ranking.rs:101-112` |
| `get_meaningful_snippet` full port from `snippet.ts` | `ranking.rs:340-427` |
| Over-fetch ratio: 4× capped at 200 → 8× capped at 500 | `query.rs:27` |

---

## Commands Executed

```bash
# All three clean after changes
cargo check --bin axon
# Output: Finished `dev` profile [unoptimized + debuginfo] in 1.86s
```

---

## Behavior Changes (Before / After)

| Aspect | Before | After |
|--------|--------|-------|
| Snippet source | Always from highest-reranked chunk per URL | From chunk with best prose richness × query relevance for that URL |
| Sentence filter | Accepted ≥4-word sentences | Requires ≥5 words (matches TS) |
| Fallback snippet | First 220 chars of cleaned blob | First prose line (≥20 chars, alphabetic) truncated to 220 chars |
| Stop word count | 18 | 45 |
| Phrase boost | None | +0.06 on verbatim phrase hit during reranking |
| Over-fetch | 4× / 200 cap | 8× / 500 cap |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | 0 errors | 0 errors, finished 1.86s | ✅ Pass |

Full `cargo test` and `cargo clippy` not run in this session — branch is under active development.

---

## Source IDs + Collections Touched

None — no embed/retrieve/crawl operations in this session. Code-only changes.

---

## Risks and Rollback

- **Risk**: `select_best_preview_chunk` adds an O(n×8) preview-scoring pass per displayed URL. For typical query limits (10–50 results), this is negligible. At very large limits it adds proportional cost.
- **Risk**: Word count threshold change (4 → 5) makes the sentence filter stricter. Short imperative sentences ("Use this to configure X.") will now be excluded. This is intentional and matches TS behavior.
- **Rollback**: Revert `ranking.rs` changes (all in one file). The `query.rs` change (one block) is trivially reverted by restoring the old `hits` collection pattern.

---

## Decisions Not Taken

- **Port `scoreBandRank` / `compareBySeverityThenScore`**: These are display-layer sorting helpers in the TS, used to sort URL groups by relevance band (high/medium/low). The Rust output sorts by `rerank_score` directly. Not ported — the display logic in Rust is minimal (no grouped/compact/full modes) so the extra abstraction would be unused.
- **Port `groupByBaseUrl` URL-group display**: TS `formatCompact` and `formatGrouped` aggregate chunks under URL headers. The Rust `run_query_native` lists individual results. Not ported — the Rust display layer is intentionally simple; structured output is driven by `--json`.
- **Port `truncateWithMarker`**: TS appends `…` when truncating. Not ported — the Rust fallback is rarely hit and the marker is cosmetic.

---

## Open Questions

- `cargo test` and `cargo clippy` should be run before merge to confirm no regressions from the word-count threshold change.
- The `suggest.rs` command (also modified on this branch) may benefit from the same `select_best_preview_chunk` logic if it surfaces snippets. Not investigated in this session.

---

## Next Steps

1. Run `cargo test` + `cargo clippy` on branch
2. Check `suggest.rs` for any snippet display paths that could use `select_best_preview_chunk`
3. PR ready for review once CI passes
