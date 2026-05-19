# Session: Tier 1 Embedding Quality + Hybrid RRF Search Fix
**Date:** 2026-03-21
**Branch:** feat/pulse-shell-and-hybrid-search
**Commits:** `79f8cf2f`, `a8812398`

---

## Session Overview

Completed the Tier 1 embedding quality plan: fixed Qwen3-Embedding-0.6B asymmetric instruction handling, replaced fixed-width chunking with semantic MarkdownSplitter, pruned domain-meaningful stopwords from BM42 sparse vectors, and added an empty-chunk guard. Then discovered and fixed a critical bug where hybrid RRF search was silently failing due to a Qdrant `/points/query` response shape mismatch. Changed the default collection from `cortex` to `axon`, restarted TEI, and verified the full pipeline end-to-end.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Continued from previous context: `chunk_markdown()` written but panicking on `ChunkConfigError(OverlapLargerThanCapacity)` |
| Phase 1 | Fixed overlap config (`200..2000` → `500..2000`), fixed test to use 660-char sections, all 4 `chunk_markdown` tests GREEN |
| Phase 2 | Updated 4 callers to `chunk_markdown`: `prepare.rs`, `url_processor.rs`, `github/wiki.rs`, `github.rs` |
| Phase 3 | Fix 3: removed `use/using/used/get/set` from `STOP_WORDS` in `sparse.rs` with TDD |
| Phase 4 | Fix 4: added empty-chunk `retain` filter in `pipeline.rs:embed_prepared_doc` with TDD |
| Phase 5 | `just verify` — two clippy failures: `items_after_test_module` in `query.rs`, `collapsible_if` in `acp_llm.rs` |
| Phase 6 | Fixed both clippy issues, `just verify` clean: 1451 lib + 617 doc tests, 0 failures |
| Phase 7 | Committed `79f8cf2f` — all Tier 1 fixes |
| Phase 8 | Restarted TEI to drop `--default-prompt`, confirmed args clean |
| Phase 9 | Test scrape → 26 chunks embedded; `axon query` → "error decoding response body" |
| Phase 10 | Diagnosed: `/points/query` returns `{"result":{"points":[...]}}` not `{"result":[...]}` |
| Phase 11 | Fixed `QdrantSearchResponse` → new `QdrantQueryResponse` + `QdrantQueryResult` structs |
| Phase 12 | All 69 qdrant tests pass, live query returns 2 results at 0.908/0.825 score |
| Phase 13 | Committed `a8812398` — hybrid search response shape fix |

---

## Key Findings

- **`/points/query` vs `/points/search` response shape differ**: `/points/search` returns `{"result":[...]}` (flat array); `/points/query` returns `{"result":{"points":[...]}}` (nested object). The hybrid.rs code was using `QdrantSearchResponse` (flat) for the `/points/query` endpoint — causing silent deserialization failure on every named-collection search. This bug existed since hybrid search was first implemented.
- **`ChunkConfig::new(min..max).with_overlap(overlap)` requires `overlap < min`**: Initial config was `200..2000` with overlap 200 (invalid). Fixed to `500..2000` with overlap 200 (matching `chunk_code`).
- **TEI `--default-prompt` was still running**: Even though the docker-compose change was committed, TEI was not restarted. Had to explicitly run `docker compose -f docker-compose.services.yaml up -d axon-tei` to apply the change.
- **Qwen3-Embedding requires strict asymmetry**: Documents must embed as raw text (no prefix); queries need `"Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery: "` prepended. The old `--default-prompt` was corrupting all document embeddings.
- **`items_after_test_module` clippy lint**: Test module placed before `pub async fn query_results` in `query.rs` — Rust/clippy requires test modules at the end of the file, not before public items.

---

## Technical Decisions

**`chunk_markdown` range 500–2000 (not 200–2000):** The `MarkdownSplitter` overlap constraint requires `overlap < min`. Since we want 200-char overlap (matching `chunk_code`), minimum must be > 200. Used 500 to match `chunk_code`'s proven range; this also avoids very small chunks that produce noisy vectors.

**`chunk_markdown` for web crawl + GitHub meta/wiki, not Reddit/YouTube:** Reddit posts and YouTube transcripts are plain text — no markdown headers to split on. `chunk_text` is correct for those paths. GitHub source code already uses tree-sitter `chunk_code`. GitHub READMEs, wikis, and repo metadata use `chunk_markdown`.

**`doc.chunks.retain()` in `embed_prepared_doc` (not at call sites):** Centralizing the filter in the pipeline means it catches empty chunks regardless of which caller produced them. No call site can bypass it.

**`QdrantQueryResponse` as a separate struct (not changing `QdrantSearchResponse`):** The two endpoints have genuinely different response shapes. Keeping separate structs prevents future confusion and makes the code self-documenting about which endpoint is being called.

**Removed `use/using/used/get/set` from stopwords (not just `use`):** All five are high-value in tech documentation: `get/set` distinguish HTTP methods and property accessors; `use/using/used` distinguish import statements, crate usage docs, and applied patterns. Removing all five improves BM42 recall for tech queries.

---

## Files Modified

| File | Purpose |
|------|---------|
| `crates/vector/ops/input.rs` | Added `chunk_markdown()` with `MarkdownSplitter` (500–2000 chars, 200-char overlap); fixed test to use 660-char sections |
| `crates/vector/ops/tei/prepare.rs` | `chunk_text` → `chunk_markdown` for web crawl content |
| `crates/jobs/refresh/url_processor.rs` | `chunk_text` → `chunk_markdown` for URL refresh content |
| `crates/ingest/github.rs` | `chunk_text` → `chunk_markdown` for repo metadata embed |
| `crates/ingest/github/wiki.rs` | `chunk_text` → `chunk_markdown` for wiki page content |
| `crates/vector/ops.rs` | Added `chunk_markdown` to public re-exports |
| `crates/vector/ops/sparse.rs` | Removed `use/using/used/get/set` from `STOP_WORDS`; added TDD test |
| `crates/vector/ops/tei/pipeline.rs` | Added `doc.chunks.retain(|c| !c.trim().is_empty())` guard + 2 TDD tests |
| `crates/vector/ops/commands/query.rs` | Moved test module to after `query_results` (clippy fix) |
| `crates/services/acp_llm.rs` | Collapsed nested `if let` chain (clippy `collapsible_if` fix) |
| `crates/vector/ops/qdrant/types.rs` | Added `QdrantQueryResult` + `QdrantQueryResponse` for `/points/query` shape |
| `crates/vector/ops/qdrant/hybrid.rs` | Switched to `QdrantQueryResponse`; fixed mock test response shape |
| `crates/vector/ops/tei/tei_client.rs` | Added `QUERY_INSTRUCTION` constant (from previous session, confirmed present) |
| `crates/vector/ops/tei.rs` | Re-exported `QUERY_INSTRUCTION` (from previous session, confirmed present) |
| `crates/vector/ops/commands/ask/context/retrieval.rs` | Prepends `QUERY_INSTRUCTION` before `tei_embed` call |
| `crates/vector/ops/commands/evaluate/scoring.rs` | Prepends `QUERY_INSTRUCTION` before `tei_embed` call |
| `crates/vector/CLAUDE.md` | Updated to document QUERY_INSTRUCTION pattern and removal of `--default-prompt` |
| `docker-compose.services.yaml` | Removed `--default-prompt` from `axon-tei` service command |
| `.env.example` | Changed `AXON_COLLECTION=cortex` → `AXON_COLLECTION=axon` |
| `Cargo.toml` | Added `markdown` feature to `text-splitter` dep |

---

## Commands Executed

```bash
# Fix chunk_markdown config and test
cargo test --lib chunk_markdown
# → 4 passed, 0 failed

# Fix 3 TDD cycle
cargo test --lib sparse
# → 13 passed (new: compute_sparse_vector_tech_terms_not_stopwords)

# Fix 4 TDD cycle
cargo test --lib pipeline::tests
# → 2 passed (new: empty_and_whitespace_chunks_are_filtered, all_empty_chunks_produces_no_chunks)

# Full integration gate
just verify
# → 1451 lib + 617 doc tests, 0 failures

# TEI restart
docker compose -f docker-compose.services.yaml up -d axon-tei
# Confirmed args: no --default-prompt

# Test scrape
./scripts/axon scrape https://docs.rs/text-splitter/latest/text_splitter/ --wait true
# → embedded 26 chunks into axon

# Verify collection
curl http://127.0.0.1:53333/collections/axon
# → points: 26, dense: ['dense'], sparse: ['bm42'], status: green

# Debug response shape
curl -X POST http://127.0.0.1:53333/collections/axon/points/query ...
# → {"result": {"points": [...]}}  ← NOT {"result": [...]}

# Live hybrid query (post-fix)
./scripts/axon query "how does markdown splitting work" --limit 3
# → 2 results: 0.908, 0.825 — hybrid RRF working
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Document embedding | All text got query instruction prefix (corrupted vectors) | Raw text only — correct per Qwen3-Embedding spec |
| Query embedding | Raw text — missing instruction (degraded recall) | Instruction prepended in Rust before TEI call |
| Web crawl chunking | Fixed 2000-char windows, hard cuts mid-sentence | Semantic splits at `##`/`###` headers + `\n\n` paragraph breaks |
| BM42 sparse vectors | `use/get/set` filtered as stopwords — missing from index | All 5 domain terms now indexed, improve tech doc recall |
| Empty chunk handling | Empty strings sent to TEI → garbage vectors | Filtered before TEI call, error logged if all chunks empty |
| Named-collection hybrid search | Silent "error decoding response body" on every query | Working — correct `QdrantQueryResponse` struct |
| Default collection | `cortex` (7M-point unnamed/legacy) | `axon` (fresh named-mode: dense + bm42 sparse) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test --lib chunk_markdown` | 4 passed | 4 passed | ✅ |
| `cargo test --lib sparse` | 13 passed | 13 passed | ✅ |
| `cargo test --lib pipeline::tests` | 2 passed | 2 passed | ✅ |
| `just verify` | 0 failures | 1451+617 passed, 0 failed | ✅ |
| `cargo test --lib qdrant` | 69 passed | 69 passed | ✅ |
| `docker inspect axon-tei \| grep default-prompt` | not present | not present | ✅ |
| `curl /collections/axon` | `sparse: ['bm42']`, `dense: ['dense']` | confirmed | ✅ |
| `axon query "markdown splitting"` | 2+ results | 2 results (0.908, 0.825) | ✅ |

---

## Source IDs + Collections Touched

| Collection | Action | Points | Outcome |
|------------|--------|--------|---------|
| `axon` (new named-mode) | Created fresh on first embed | 26 (test scrape) | ✅ Named-mode with dense+bm42 |
| `cortex` (old unnamed) | Untouched — still exists | ~7M | Not modified this session |

Test scrape source: `https://docs.rs/text-splitter/latest/text_splitter/`

---

## Risks and Rollback

**TEI `--default-prompt` removal:** All future document embeds will be raw text. Any existing points in old collections were embedded with the instruction prefix — mixed embeddings would degrade search quality. Fresh `axon` collection avoids this. Rollback: re-add `--default-prompt` to `docker-compose.services.yaml` and restart TEI.

**`chunk_markdown` 500-char minimum:** Shorter markdown sections (< 500 chars) are now combined with adjacent content before splitting. This is better for semantic coherence but means very short pages may produce fewer, larger chunks than before. No rollback needed — upgrade only.

**Hybrid search `/points/query` fix:** The old `QdrantSearchResponse` was silently failing on all named-collection queries — hybrid search was never working. Fix is correct and tested. No regression risk.

---

## Decisions Not Taken

- **`chunk_markdown` with 200..2000 range:** Would have required overlap < 200 (e.g., 100), producing smaller minimum chunks. Chose 500 to match `chunk_code` and avoid tiny fragments that produce low-quality embeddings.
- **`chunk_markdown` for Reddit/YouTube:** Plain text with no markdown structure — MarkdownSplitter would fall back to word boundaries anyway. Left as `chunk_text`.
- **`chunk_markdown` for `github/files.rs` non-code fallback:** Source code files that lack a tree-sitter grammar fall back to `chunk_text`. Could switch to `chunk_markdown` for `.md` files specifically, but the extension check adds complexity for minor benefit given GitHub READMEs are already handled by the module root.
- **Migrating `cortex` → `axon`:** The existing 7M-point `cortex` collection was embedded with the wrong instruction prefix. Rather than running a slow migration (~4-5 hours), the decision was to rebuild fresh from the export.

---

## Open Questions

- Should `github/files.rs` use `chunk_markdown` for `.md` extension files (in the `chunk_code` fallback path)? Currently falls back to `chunk_text` for all non-code files.
- The `axon` collection has 26 test points from the docs.rs scrape — should these be deleted before production reindexing, or left as a baseline?
- `chunk_text("")` still returns `vec![""]` (fast-path returns the input as-is). The `retain` filter in `pipeline.rs` now catches this, but the root behavior is surprising. Consider fixing the fast-path to return `vec![]` for empty input.

---

## Next Steps

1. **Bulk reindex** — use `axon export` output to re-embed all previously indexed content into the `axon` collection with correct embeddings
2. **Verify ask/evaluate paths** — test `axon ask` to confirm `QUERY_INSTRUCTION` is prepended correctly end-to-end (retrieval + scoring)
3. **Consider `chunk_text("")` fast-path fix** — return `vec![]` for empty input instead of `vec![""]`
4. **Monitor chunk quality** — after bulk reindex, run `axon evaluate` on a sample of known questions to compare RAG vs baseline scores and verify quality improvement from semantic chunking
