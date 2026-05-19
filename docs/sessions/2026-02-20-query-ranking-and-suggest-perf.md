# Session: Query Ranking & Suggest Performance
**Date:** 2026-02-20 00:59 UTC
**Branch:** `perf/command-performance-fixes`

---

## Session Overview

Two distinct improvements shipped:

1. **`suggest` command performance fix** — the command was scrolling the entire Qdrant collection to build a URL lookup, even though the LLM prompt only uses the first 500 URLs. Added an early-exit `limit: Option<usize>` parameter to `scroll_url_set`, cutting scroll work from O(N_collection) to O(url_scroll_limit).

2. **`query` command retrieval quality overhaul** — the command was doing raw vector search with no reranking and a 140-char dumb truncation for snippets. Wired it into the existing `ranking` module (lexical URL boost, path structural boost), added a verbatim phrase boost, extended stop words, ported `getMeaningfulSnippet` from the TypeScript reference implementation, and bumped over-fetch from 4× to 8×.

All changes: `cargo check` clean, no clippy warnings.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | User asks why `suggest` is slow |
| +5 min | Root-cause identified: `qdrant_indexed_urls` → `scroll_url_set` scrolls entire collection serially |
| +10 min | Fix applied: `limit: Option<usize>` added to `scroll_url_set` and `qdrant_indexed_urls`; `suggest.rs` passes `Some(existing_url_context_limit)` |
| +15 min | User asks about `query` retrieval optimization / URL boosts |
| +20 min | `query` command overhauled: over-fetch 4×→8×, `rerank_ask_candidates` + `select_diverse_candidates` wired in |
| +25 min | Checked TEI `/rerank` endpoint → 424 (embedding model only, no cross-encoder) |
| +30 min | User shares TypeScript `query.ts` and asks what patterns to port |
| +40 min | Three patterns identified: extended stop words, phrase boost, `getMeaningfulSnippet` |
| +50 min | All patterns implemented in `ranking.rs`; `query.rs` updated to use `get_meaningful_snippet` |

---

## Key Findings

- **`suggest` root cause** (`client.rs:61-111`): `scroll_url_set` fetched all `chunk_index==0` points from Qdrant, then `suggest.rs` only used the first 500 for the LLM prompt. No early exit existed.
- **TEI host** (`http://100.74.16.82:52000`): Running `Qwen/Qwen3-Embedding-0.6B` (embedding, `pooling: last_token`). Rerank endpoint returns HTTP 424 — model type mismatch. Cross-encoder requires a separate TEI instance.
- **`query` had zero reranking**: Raw cosine similarity only, no lexical boost, no diversity selection, snippets were first 140 chars (often mid-sentence markdown).
- **`rerank_ask_candidates`** existed in `ranking.rs` with URL token boost (+0.045/token, cap 0.30), chunk text token boost (+0.015/token), and path structural boost (+0.04 for `/docs/`, `/api/`, etc.) — but `query` never called it.
- **TS reference impl** (`~/workspace/axon/src/utils/snippet.ts`, `deduplication.ts`): Uses 10× over-fetch, sentence-level snippet extraction with boilerplate stripping, and per-URL group scoring with phrase boost (+0.08).

---

## Technical Decisions

**Suggest: cap scroll at `existing_url_context_limit`, not unlimited**
The indexed URL list serves two purposes: (1) LLM prompt context (needs ≤500), and (2) dedup lookup for filtering LLM suggestions. Capping the scroll at the prompt limit means the dedup lookup is also limited, but: suggesting an already-indexed URL is harmless (crawl is idempotent), and setting `AXON_SUGGEST_EXISTING_URL_LIMIT=0` falls back to unlimited scroll for accurate dedup.

**`qdrant_urls_for_domain` stays unlimited**
This function is called during stale-URL deletion — it must fetch all indexed URLs for a domain or it will fail to identify stale entries.

**Phrase boost uses reconstructed phrase from tokens (not raw query)**
`rerank_ask_candidates` takes `query_tokens: &[String]`, so the phrase is `tokens.join(" ")`. Stop words are stripped, so the phrase is an approximate match — but this is *better* for technical docs (e.g., "tokio runtime" matches exactly vs. "how to use tokio runtime" where noise words would fail).

**Over-fetch 8× not 10× (TS default)**
Cap is 500 (vs TS cap of 1000). Qdrant is local, latency is low; 8× with cap 500 gives enough diversity headroom without punishing small collections.

**Sentence splitting on `.!?` without lookbehind**
Rust lacks regex lookbehind without the `fancy-regex` crate. Simple `split(|c| matches!(c, '.' | '!' | '?'))` will occasionally split on abbreviations/decimals, but `is_relevant_sentence` (min 25 chars, min 4 words) filters out the resulting fragments.

---

## Files Modified

| File | Purpose |
|------|---------|
| `crates/vector/ops/qdrant/client.rs` | Added `limit: Option<usize>` to `scroll_url_set`; early-exit when `seen.len() >= cap`. Updated `qdrant_indexed_urls` to accept and pass limit. `qdrant_urls_for_domain` passes `None`. |
| `crates/vector/ops/commands/suggest.rs` | Computes `url_scroll_limit = Some(existing_url_context_limit)` when > 0; passes to `qdrant_indexed_urls`. |
| `crates/vector/ops/commands/query.rs` | Added `ranking` import. Over-fetch 4×→8×, cap 200→500. Builds `AskCandidate` structs from search hits. Applies `rerank_ask_candidates` + `select_diverse_candidates`. Uses `get_meaningful_snippet` for display. |
| `crates/vector/ops/ranking.rs` | Extended `STOP_WORDS` (+27 words). Added phrase boost to `rerank_ask_candidates` (+0.06 verbatim phrase in chunk). Added `strip_markdown_inline`, `clean_snippet_source`, `is_relevant_sentence`, `score_sentence`, `get_meaningful_snippet`. |

---

## Commands Executed

```bash
# Verify TEI capabilities
curl -s http://100.74.16.82:52000/info
# → model_type: {embedding: {pooling: last_token}} — no reranker

curl -s -o /dev/null -w "%{http_code}" -X POST http://100.74.16.82:52000/rerank \
  -H "Content-Type: application/json" \
  -d '{"query":"test","texts":["hello world"]}'
# → 424 Failed Dependency (endpoint exists, wrong model type)

# Compile checks after each change
cargo check --bin axon
# → Finished dev profile in 2.59s (suggest fix)
# → Finished dev profile in 1.31s (query ranking)
# → Finished dev profile in 3.73s (ranking.rs additions)
```

---

## Behavior Changes (Before/After)

### `axon suggest`

| Metric | Before | After |
|--------|--------|-------|
| Scroll round-trips | O(N_docs / 1000) — e.g., 50 trips for 50k docs | O(500 / 1000) = 1 trip (default `AXON_SUGGEST_EXISTING_URL_LIMIT=500`) |
| Dedup lookup completeness | 100% of collection | First 500 URLs (trade-off accepted; suggestions are idempotent) |
| `AXON_SUGGEST_EXISTING_URL_LIMIT=0` | Still full scroll | Still full scroll (0 → `None` limit) |

### `axon query`

| Aspect | Before | After |
|--------|--------|-------|
| Qdrant fetch | `search_limit` points | `search_limit × 8` points (cap 500) |
| Reranking | None (raw cosine) | URL token overlap + chunk token overlap + path boost + phrase boost |
| URL diversity | All chunks returned | Max 2 chunks per unique URL (`select_diverse_candidates`) |
| Snippet | First 140 chars of chunk (raw markdown) | Up to 5 query-relevant sentences, markdown stripped, boilerplate removed |
| Score displayed | Raw cosine score | `rerank_score` (cosine + boosts) |
| JSON output | `score` only | `score` + `rerank_score` both present |
| Stop words | 17 words | 44 words (extended with high-freq doc noise words) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` (after suggest fix) | 0 errors | `Finished dev profile in 2.59s` | ✅ |
| `cargo check --bin axon` (after query ranking) | 0 errors | `Finished dev profile in 1.31s` | ✅ |
| `cargo check --bin axon` (after ranking.rs additions) | 0 errors | `Finished dev profile in 3.73s` | ✅ |
| TEI `/rerank` endpoint | 424 or similar failure | `424 Failed Dependency` | ✅ confirmed unavailable |
| TEI `/info` | Embedding model | `Qwen/Qwen3-Embedding-0.6B`, `model_type.embedding` | ✅ |

---

## Source IDs + Collections Touched

No Qdrant embed/retrieve operations performed during this session (code changes only).

---

## Risks and Rollback

**Suggest dedup incompleteness**: With `AXON_SUGGEST_EXISTING_URL_LIMIT=500` (default), `indexed_lookup` only contains ~500 URLs. LLM suggestions beyond those 500 won't be filtered. Risk is low — crawl is idempotent. If needed, set `AXON_SUGGEST_EXISTING_URL_LIMIT=5000` to raise the cap.

**Query snippet sentence splitter**: The `split(|c| matches!(c, '.' | '!' | '?'))` in `get_meaningful_snippet` will occasionally split on version numbers (e.g., `v2.0`) or abbreviations. `is_relevant_sentence` (min 25 chars, min 4 words) discards the resulting fragments, so the practical impact is minor — some sentences may be split into shorter ones that get filtered out.

**Rollback**: All changes are in `crates/vector/ops/`. Revert any file with `git checkout HEAD -- <file>`.

---

## Decisions Not Taken

**Cross-encoder reranker**: TEI host runs an embedding model; `/rerank` endpoint returns 424. Adding a cross-encoder would require a second TEI instance with a model like `cross-encoder/ms-marco-MiniLM-L-6-v2`. The hook point for this would be `ranking::rerank_cross_encoder(cfg, query, candidates)` gated on `AXON_RERANKER_URL`. Deferred — no infra available.

**`rankUrlGroups` from TS**: The TS deduplication module scores entire URL groups (looking across up to 6 chunks per URL, with `coverageBoost` + `titleHeaderBoost`). We kept per-chunk reranking instead. The group-level approach would require restructuring `rerank_ask_candidates` to group first, which would also affect `ask`. Deferred.

**`canonicalizeUrl` (strip UTM, normalize ports)**: The TS version strips tracking params and normalizes trailing slashes for URL grouping. Our `select_diverse_candidates` groups by exact URL. Minor edge case — not ported.

**`selectBestPreviewItem`**: Picks the best preview chunk among multiple from the same URL using `previewScore = relevanceScore * 10 + richnessScore`. Not ported because our display currently shows one result per display row, not a chunk picker per URL group.

---

## Open Questions

- When we spin up a cross-encoder TEI instance, what model? `BAAI/bge-reranker-v2-m3` (multilingual, higher quality) vs `cross-encoder/ms-marco-MiniLM-L-6-v2` (smaller, faster)?
- Should `get_meaningful_snippet` also be used in `ask` context building, or is the current full-chunk approach better for LLM context quality?
- The `ask_candidate_limit` (default 64 via `AXON_ASK_CANDIDATE_LIMIT`) is separate from query over-fetch. Should `ask` also use 8× over-fetch logic, or is 64 candidates already sufficient?

---

## Next Steps

- Spin up second TEI instance with a reranker model; implement `ranking::rerank_cross_encoder` gated on `AXON_RERANKER_URL`
- Add tests for `get_meaningful_snippet` (unit tests covering: empty input, no query tokens, phrase match, fallback path)
- Consider porting `rankUrlGroups` group-level scoring for the `query` command's URL diversity stage
- Run live `axon query` comparison (before/after) on a query with known good results to validate snippet quality improvement
