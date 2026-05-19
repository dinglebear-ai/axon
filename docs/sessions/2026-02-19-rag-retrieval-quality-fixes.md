# RAG Retrieval Quality Fixes
**Date:** 2026-02-19
**Branch:** `perf/command-performance-fixes`

---

## Session Overview

Diagnosed and fixed a three-bug compounding failure causing the `axon ask`/`axon evaluate` RAG pipeline to inject off-topic context and generate misleading, citation-backed answers. The failure mode was demonstrated live via `axon evaluate "tell me how to make an http client in kotlin"`, which returned an answer about implementing Apollo GraphQL's internal `HttpEngine` interface instead of general Kotlin HTTP client usage.

---

## Timeline

| Time (relative) | Activity |
|-----------------|----------|
| 0:00 | User ran `axon evaluate "tell me how to make an http client in kotlin"` — observed poor RAG answer |
| 0:05 | Invoked `superpowers:systematic-debugging` skill; began Phase 1 root cause investigation |
| 0:10 | Read `evaluate.rs`, `ask.rs`, `ranking.rs`, `streaming.rs`, `config/parse.rs` |
| 0:25 | Identified three compounding bugs; formed hypotheses |
| 0:30 | Implemented Fix 1 (system prompt escape hatch) |
| 0:35 | Implemented Fix 2 (stop word list correction) |
| 0:38 | Implemented Fix 3 (raise default relevance threshold) |
| 0:42 | Verified: `cargo check` clean, `cargo test` 101/101 pass, `cargo clippy` no new warnings |

---

## Key Findings

### Bug 1 — System Prompt Mandates Exclusive Source Usage
- **File:** `crates/vector/ops/commands/streaming.rs:161` (streaming) and `:190` (non-streaming)
- The prompt `"You answer questions exclusively from the retrieved source documents"` + `"Do not fill gaps from general knowledge"` made the LLM produce misleading answers when Qdrant returned off-topic docs. The LLM had no mechanism to recognize or report a relevance mismatch — it just synthesized whatever it was given.

### Bug 2 — `"make"` and `"create"` Were Stop Words
- **File:** `crates/vector/ops/ranking.rs:8`
- For query `"tell me how to make an http client in kotlin"`, after stop-word filtering `query_tokens` = `["tell", "http", "client", "kotlin"]`. The verb `"make"` — which encodes the user's intent — was erased. Apollo GraphQL docs then received a `+0.135` lexical boost because they contain `"http"`, `"client"`, and `"kotlin"`, all of which were still query tokens.

### Bug 3 — Default Relevance Threshold Is Zero
- **File:** `crates/core/config/parse.rs:391`, `crates/jobs/common.rs:120`
- `AXON_ASK_MIN_RELEVANCE_SCORE` defaulted to `0.0` — every document Qdrant returned passed the filter unconditionally. No relevance gate existed.

### Failure Chain
```
ask_min_relevance_score = 0.0
 → every Qdrant hit passes
 → "make" stripped as stop word → Apollo docs get max lexical boost
 → off-topic Apollo docs enter context
 → system prompt mandates exclusive source usage
 → LLM forced to generate answer from Apollo/OpenAI SDK internals docs
 → user gets citation-backed answer about HttpEngine interfaces instead of Ktor/OkHttp
```

---

## Technical Decisions

### Decision 1 — Relevance escape hatch rather than harder threshold-only gate
Adding a higher threshold alone would cause "no candidates met relevance threshold" errors for other valid queries where indexed docs exist but score below an arbitrary cutoff. The prompt escape hatch is more robust: it keeps context injection working for relevant docs while allowing the LLM to self-report and fall back when the context is genuinely off-topic.

### Decision 2 — Default threshold raised to 0.45, not higher
`0.45` removes clearly marginal documents without risking "no results" errors for topics that ARE indexed but score in the 0.45–0.60 range. `AXON_ASK_MIN_RELEVANCE_SCORE` env var allows tuning per-deployment.

### Decision 3 — Remove content verbs from stop words entirely
Stop words should be syntactic/structural words (articles, prepositions, auxiliary verbs). Content verbs like `"make"`, `"create"`, `"build"` encode the user's intent and must survive tokenization so the reranker can use them as signals.

### Decision 4 — Did NOT change the reranker's score formula
The lexical boost weights (`+0.045` URL match, `+0.015` chunk match, max `0.30`) are not changed. The stop word fix alone narrows the set of docs that get boosted incorrectly.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/vector/ops/commands/streaming.rs` | Both `ask_llm_streaming` and `ask_llm_non_streaming` system prompts rewritten | Add STEP 1 relevance check and fallback-to-training-knowledge escape hatch |
| `crates/vector/ops/ranking.rs` | Removed `"make"`, `"create"` from `STOP_WORDS` | Restore content verb signals for intent-based reranking |
| `crates/core/config/parse.rs` | `ask_min_relevance_score` default `0.0` → `0.45` | Require minimum vector similarity before injecting into context |
| `crates/jobs/common.rs` | `ask_min_relevance_score: 0.0` → `0.45` | Match parse.rs default in programmatic Config initialization |

---

## Commands Executed

```bash
cargo check --bin axon        # → clean (23s)
cargo test                    # → 101 passed, 0 failed
cargo clippy                  # → 0 new warnings (pre-existing type_complexity in batch.rs)
wc -l streaming.rs            # → 363 lines (well under 500 monolith limit)
```

---

## Behavior Changes (Before/After)

### RAG System Prompt

**Before:**
```
You answer questions exclusively from the retrieved source documents.
...
Do not fill gaps from general knowledge.
```

**After:**
```
STEP 1 — RELEVANCE CHECK: assess whether retrieved docs genuinely address the question (topical overlap, not just keyword overlap).

STEP 2:
  IF RELEVANT: answer from sources with [S#] citations + ## Sources footer
  IF NOT RELEVANT: "The indexed knowledge base does not contain directly relevant information."
                   Then answer in ## Answer (from training knowledge)
```

### Stop Words

**Before:** `["the", "and", "for", "with", "that", "this", "from", "into", "how", "what", "where", "when", "you", "your", "are", "can", "does", "create", "make"]`

**After:** `["the", "and", "for", "with", "that", "this", "from", "into", "how", "what", "where", "when", "you", "your", "are", "can", "does"]`

### Relevance Threshold

**Before:** `AXON_ASK_MIN_RELEVANCE_SCORE = 0.0` (no gate)

**After:** `AXON_ASK_MIN_RELEVANCE_SCORE = 0.45` (overridable via env)

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | Clean | Clean (23s) | ✅ |
| `cargo test` | All pass | 101/101 passed | ✅ |
| `cargo clippy` | No new warnings | No new warnings | ✅ |
| `wc -l streaming.rs` | ≤500 lines | 363 lines | ✅ |
| `tokenize_query_drops_stop_words_and_short_tokens` | `["install", "api", "docs"]` | `["install", "api", "docs"]` | ✅ |

---

## Risks and Rollback

### Risks

1. **Threshold too high for sparse indexes.** If a user's Qdrant collection has few documents and all score below `0.45`, queries that should work will now fail with "no candidates met relevance threshold." Mitigation: env var `AXON_ASK_MIN_RELEVANCE_SCORE=0.0` restores old behavior.

2. **LLM may incorrectly assess context as irrelevant.** If the model is weak or the system prompt is not followed well, the relevance check could produce false negatives. The fallback answer is labeled clearly ("from training knowledge"), so the failure mode is transparent rather than misleading.

3. **`"make"`/`"create"` now tokenize into query terms.** Documents that mention "make" or "create" in unrelated contexts could get marginally higher rerank scores. Impact is bounded by `lexical_boost.min(0.30)`.

### Rollback

```bash
# Revert all three files
git checkout HEAD -- \
  crates/vector/ops/commands/streaming.rs \
  crates/vector/ops/ranking.rs \
  crates/core/config/parse.rs \
  crates/jobs/common.rs

# Or just raise/lower threshold without reverting prompt:
export AXON_ASK_MIN_RELEVANCE_SCORE=0.0
```

---

## Decisions Not Taken

| Alternative | Why Rejected |
|-------------|-------------|
| **Hard context suppression** — If top score < threshold, inject NO context | Too blunt; valid queries with moderate scores would get the no-context baseline silently |
| **Score metadata in prompt** — Tell LLM the top similarity score explicitly | Adds complexity; the relevance check in the prompt achieves the same goal more cleanly |
| **Change reranker formula weights** | Root cause is stop words + threshold, not the weight values; changing weights would be tuning against a symptom |
| **Query expansion before embedding** — Rewrite query to "kotlin http library tutorial" before TEI | Over-engineering; the prompt escape hatch covers the failure mode without semantic manipulation |

---

## Open Questions

1. **What is the actual cosine similarity score for Apollo docs against "how to make an http client in kotlin"?** Diagnostics mode (`--ask-diagnostics` flag) would show this, but was not run. Knowing whether the score is 0.50 vs 0.70 would help calibrate the `0.45` threshold choice.

2. **Does the prompt escape hatch work with smaller/weaker LLMs?** The relevance check relies on the LLM following multi-step instructions. Smaller models may not reliably execute STEP 1 before STEP 2.

3. **Should `"how"` stay a stop word?** It's a question word that encodes query intent (similar to "make"/"create"). Worth reviewing.

---

## Next Steps

1. Run `axon evaluate "tell me how to make an http client in kotlin"` again after deployment to verify the fix works end-to-end in the live environment.
2. Consider adding a test that verifies the relevance escape hatch text appears in the context when top scores are low (requires a mock Qdrant).
3. Re-examine the stop word list holistically — `"how"`, `"what"`, `"where"`, `"when"` are question words that may also encode intent and warrant review.
4. Consider exposing `--ask-diagnostics` as a default-on option for `evaluate` so similarity scores are always visible for quality assessment.
