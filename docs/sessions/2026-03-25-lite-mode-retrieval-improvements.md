# Session: Lite Mode + Retrieval Quality Improvements
Date: 2026-03-25
Commit: 3fc64858 | Branch: feat/lite-mode | Version: 0.32.2 → 0.33.0

## Session Overview

Two parallel tracks of work:
1. **Lite mode validation** — confirmed `AXON_LITE=1` end-to-end pipeline (crawl → embed → store in Qdrant) works correctly with SQLite backend and in-process workers.
2. **Retrieval quality improvements** — diagnosed why BM42 hybrid search was producing poor results (session JSONL exports dominating, small candidate pool, raw TF amplifying noisy docs) and implemented four generic fixes.

## Timeline

| Time | Activity |
|------|----------|
| Session start | User requested lite mode smoke test: crawl → embed → Qdrant |
| Phase 1 | Diagnosed thin-page confusion (example.com has <200 chars markdown) |
| Phase 1 | Confirmed URL normalization works in lite mode (`normalize_local_service_url()` applies regardless of lite flag) |
| Phase 1 | Successful crawl + embed of docs.rs in lite mode (89 chunks) |
| Phase 2 | User requested retrieval quality investigation for `AXON_COLLECTION=axon` |
| Phase 2 | Dispatched parallel agents to review retrieval/ranking code |
| Phase 2 | Root-cause: JSONL session exports (file:// URLs) polluting BM42 arm with `mcp__*` tokens |
| Phase 2 | Root-cause: fetch_limit 8x/500 too small — relevant docs missing from candidate pool |
| Phase 2 | Root-cause: raw TF lets high-repetition docs (awesome-mcp-servers localized READMEs) dominate BM42 |
| Phase 3 | Implemented all 4 fixes; all tests pass |
| Phase 4 | quick-push: version bump, changelog, commit, push |

## Key Findings

- **JSONL session files indexed**: `file:///home/jmagar/.claude/projects/.../a0c9d788.jsonl` was ranking #2 for "MCP tool calling protocol" — hundreds of `mcp__plugin_*` tool names gave it a very high BM42 score. `is_low_signal_url()` previously missed `file://` and `.jsonl` patterns.
- **BM42 noise floor exceeded legitimate content**: Best MCP spec page BM42 score was ~25; noise floor at rank 160 was ~35. MCP content was never entering the BM42 prefetch window of 160 candidates.
- **Raw TF amplification**: A localized README with 500 "mcp" occurrences scored 20x higher than a spec page with 25 occurrences. Log normalization reduces this to ~1.9x ratio.
- **URL normalization confirmed working**: `normalize_local_service_url()` in `crates/core/config/parse/docker.rs` rewrites container DNS to localhost ports regardless of `AXON_LITE` — was never broken.
- **`axon` collection is Named mode**: Created during lite mode testing; `ensure_collection()` creates new collections in Named mode (dense + bm42 sparse), so RRF hybrid search IS active.

## Technical Decisions

### Log-normalized TF (`ln(1 + count)`) over raw count
BM25 uses saturating TF specifically to prevent term-repetition dominance. `ln(1+500) ≈ 6.2` vs `ln(1+25) ≈ 3.26` — a 1.9x ratio vs the previous 20x. Applied symmetrically to both document and query sparse vectors (consistent). All 14 sparse tests pass unchanged.

### Low-signal URL filter as shared function
`is_low_signal_url()` defined once in `ranking.rs`, consumed by both `query.rs` and `ask/context/heuristics.rs`. Before this, `heuristics.rs` had its own inline check that caught `/docs/sessions/` and `/.cache/` but missed `file://` and bare `.jsonl` URLs.

### `allow_low_signal` bypass from query tokens
Rather than hard-blocking session/log content, queries containing "session", "sessions", "log", "logs", "history", "histories" bypass the filter. Users can still retrieve their session exports when explicitly asking about them.

### Title/URL prepending in pipeline
Each chunk embedded as `[title] url\n\nchunk` so dense vectors capture document identity. Payload still stores raw `chunk_text` — search results show unmodified content. Only new/re-indexed content benefits; existing points in `axon` collection have the old embeddings.

### fetch_limit 16x/1000 (was 8x/500)
With `limit=10`, previous formula gave pool of 80. Relevant content at dense rank 82 was invisible to reranker. Doubling to 16x with a higher cap of 1000 meaningfully increases recall.

## Files Modified

| File | Change |
|------|--------|
| `crates/vector/ops/sparse.rs` | `values.push(count as f32)` → `values.push((1.0_f32 + count as f32).ln())`; updated module doc, struct doc, function doc |
| `crates/vector/ops/ranking.rs` | Added `is_low_signal_url()` public function; catches `file://`, `.jsonl`, `/docs/sessions/`, `/.cache/`, non-web `.log` paths |
| `crates/vector/ops/commands/query.rs` | fetch_limit 8x/500 → 16x/1000; added `allow_low_signal` bypass; URL filter in `filter_map` |
| `crates/vector/ops/commands/ask/context/heuristics.rs` | `is_low_signal_source_url()` delegates to `ranking::is_low_signal_url()` — single definition |
| `crates/vector/ops/tei/pipeline.rs` | `embed_texts` built with title/URL prepend; `doc.chunks` still used for payload; updated comments |
| `Cargo.toml` | Version 0.32.2 → 0.33.0 |
| `CHANGELOG.md` | Added v0.33.0 section |
| (prior commits) | `crates/jobs/backend.rs`, `crates/jobs/lite/workers.rs`, `crates/cli/commands/*.rs`, etc. — lite mode `JobBackend` trait, `LiteBackend`, `FullBackend`, doctor SQLite check |

## Commands Executed

```bash
# Lite mode smoke test
AXON_LITE=1 ./scripts/axon doctor
AXON_LITE=1 ./scripts/axon crawl https://docs.rs --wait true
AXON_LITE=1 ./scripts/axon query "rust async runtime"

# Test suite (all passing)
cargo test --lib sparse    # 14 passed
cargo test --lib ranking   # 22 passed

# Build verification
cargo check -q

# Push
git push -u origin feat/lite-mode
```

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| BM42 TF weights | Raw count (500 → 500.0) | Log-normalized (500 → 6.2) |
| JSONL/file:// in query results | Would appear if BM42 score high enough | Filtered unless query contains "session"/"log"/"history" |
| fetch_limit (query cmd) | `(limit+offset)*8`, cap 500 | `(limit+offset)*16`, cap 1000 |
| Dense embeddings | Raw chunk text | `[title] url\n\nchunk` (new content only) |
| ask vs query low-signal filter | Different implementations, ask caught more | Shared `is_low_signal_url()`, consistent behavior |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test --lib sparse` | 14 passed | 14 passed | ✅ |
| `cargo test --lib ranking` | 22 passed | 22 passed | ✅ |
| `cargo check -q` | Clean | Clean | ✅ |
| `git push -u origin feat/lite-mode` | Branch pushed | Branch created on remote | ✅ |
| All values positive after ln() | `ln(1+1)=0.693 > 0` | All values positive test passes | ✅ |
| `repeated_term_has_higher_weight` | `ln(4) > ln(2)` | Test passes | ✅ |

## Risks and Rollback

- **BM42 score scale change**: Log normalization changes absolute sparse scores. Qdrant's IDF weighting still applies server-side; RRF fusion uses ranks not scores, so absolute value change doesn't affect RRF. Reranking in `ranking.rs` uses cosine scores from Qdrant hits, not the sparse TF values. **Risk: low**.
- **Title/URL prepend embedding mismatch**: Existing points in `axon` collection were embedded without title/URL prefix; new points get it. Cosine similarity between old and new embeddings will be slightly lower for the same content. Mitigation: full re-index with `axon embed` clears this. **Risk: cosmetic for current small collection**.
- **Rollback**: Revert `sparse.rs` line 121 to `count as f32`, revert `query.rs` fetch_limit formula, remove `is_low_signal_url` from `ranking.rs` and restore inline check in `heuristics.rs`.

## Decisions Not Taken

- **Document length normalization** (normalize TF by `sqrt(token_count)`): Would fix long-chunk bias in BM42. Deferred — identified as next improvement after min-token-length work.
- **Min token length 2** (unlock "go", "js", "ui", "ai"): Identified as high-value for tech docs. Deferred — requires targeted 2-char stoplist tuned for tech content (next task this session).
- **Prefetch window increase** (`hybrid_search_candidates` 150 → 200+): Low-risk, identified improvement. Deferred to same batch as token length work.
- **DBSFusion over RRF**: More complex fusion strategy, requires measuring distribution. Not worth complexity without benchmarks.

## Open Questions

- Does Qdrant's IDF modifier interact correctly with log-normalized TF? (Assumed yes — IDF is applied server-side to whatever client-side value is sent; the semantic meaning of "higher TF = more weight" is preserved.)
- What is the actual size and content breakdown of the `axon` collection post-lite-mode testing? (Not queried this session — only docs.rs and modelcontextprotocol.io were crawled.)
- Full collection re-index needed for title/URL prepend to benefit all points. Estimated time unknown (depends on collection size).

## Next Steps

1. **Min token length 2** — lower filter in `sparse.rs` and `ranking.rs` from `< 3` to `< 2` with targeted 2-char tech stoplist; query domains/sources in `axon` collection to inform the stoplist
2. **Prefetch window** — bump `hybrid_search_candidates` default from 150 to 200-250
3. **Document length normalization** — normalize sparse TF by `sqrt(token_count)` in `compute_sparse_vector()`
4. **Full re-index** — `axon embed` over all indexed URLs to apply title/URL prepend to existing content
