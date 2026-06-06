# Context Injection Pipeline

How Axon retrieves, ranks, and assembles the `Context:` block that is injected into the RAG LLM prompt for `ask` and `evaluate`.

---

## Overview

Every `ask` and `evaluate` command goes through the same five-stage pipeline before any LLM call is made:

```
Query
  └─► 1. Embed          — TEI converts query text to one or two dense vectors
  └─► 2. Retrieve       — Qdrant ANN search returns up to N candidate chunks
  └─► 3. Filter         — Low-signal and allowlist guards narrow the pool
  └─► 4. Rerank         — Lexical + domain boosts re-order by combined score
  └─► 5. Build context  — Chunks and full docs assembled into a string
                              └─► injected as "Context:\n..." into the LLM prompt
```

The assembled `context` string is passed directly to the LLM as part of the user message:

```
Question: {query}

Context:
{context}
```

---

## Stage 1 — Embed the Query

`retrieval.rs → retrieve_ask_candidates`

Query vectors are encoded with `QUERY_INSTRUCTION` from `src/vector/ops/tei/tei_client.rs`:

```
Instruct: Given a web search query, retrieve relevant passages that answer the query
Query: {query}
```

This prefix is applied only to query-side embeddings. Document chunks are embedded as raw text.

`retrieve_ask_candidates` embeds the full user query and, when keyword extraction produces a distinct keyword query, embeds that keyword query too. The full user query uses `tei::EmbedInput::query(...)`, which prepends the query instruction. The keyword form uses `tei::EmbedInput::document(...)` because it is already document-shaped text. The additional keyword vector improves recall for exact terms, identifiers, API names, and short domain phrases.

```
"how does axon crawl work?"
        ↓ prepend QUERY_INSTRUCTION
        ↓ TEI
[0.023, -0.441, 0.118, ...]   (1024-dim or model-dependent)
```

---

## Stage 2 — Retrieve Candidates from Qdrant

`retrieval.rs → retrieve_ask_candidates`

`qdrant::dispatch_vector_search` searches Qdrant using the dense query vector. Named hybrid collections use the dense vector plus BM42 sparse search with RRF fusion; legacy unnamed collections fall back to dense-only search. When a keyword query vector exists, Axon runs a second search and merges/deduplicates the candidate pool.

The number of candidates fetched is controlled by `cfg.ask_candidate_limit` (env: `AXON_ASK_CANDIDATE_LIMIT`, default `250`).

Each result comes back with:
- `score` — retrieval score from Qdrant. Dense-only paths return cosine-style scores; hybrid RRF paths return unitless rank-fusion scores.
- `url` — source page URL stored in the Qdrant payload
- `chunk_text` — the raw text of that chunk

Chunks with fewer than 40 characters are dropped immediately.

---

## Stage 3 — Filter

`retrieval.rs → retrieve_ask_candidates`

Two guards run after retrieval:

### Low-signal filter

URLs matching these patterns are dropped unless the query itself is about sessions/logs:

| Pattern | Rationale |
|---------|-----------|
| `/docs/sessions/` | AI session export files — noise for most queries |
| `/.cache/` | Build artefacts, not documentation |
| `/logs/` (local file path only) | Log files |
| `.log` (local file path only) | Log files |

The query is allowed to opt-in: if the query contains tokens like `session`, `log`, `history`, or the substring `docs/sessions`, the low-signal filter is bypassed.

### Authoritative domain boost

If `AXON_ASK_AUTHORITATIVE_DOMAINS` is set (comma-separated domains), matching chunks receive the configured `AXON_ASK_AUTHORITATIVE_BOOST`. Matches use exact host or subdomain checks.

When the domain list is empty (the default), no authority boost is applied.

---

## Stage 4 — Rerank

`retrieval.rs → apply_mode_aware_rerank`

Cosine/dense-only retrieval paths are re-scored using a combined formula:

```
rerank_score = retrieval_score
             + lexical_boost    (capped at 0.30)
             + docs_boost       (0.04 if path has /docs/, /guides/, /api/, /reference/)
             + authority_boost  (cfg.ask_authoritative_boost if domain is in authoritative list)
             + phrase_boost     (0.06 if joined query tokens appear verbatim in chunk text)
```

**Lexical boost** details:
- `+0.045` for each query token found in the chunk's URL path tokens
- `+0.015` for each query token found in the chunk text tokens

Tokens are lowercased, split on non-alphanumeric characters, and stop-words are stripped (`the`, `and`, `for`, `how`, `what`, etc.).

After scoring on cosine/dense-only paths, two post-rerank gates remove candidates that don't meet the bar:

1. `rerank_score < cfg.ask_min_relevance_score` (env: `AXON_ASK_MIN_RELEVANCE_SCORE`, default 0.45) → dropped
2. `candidate_has_topical_overlap` → dropped if the candidate shares too few tokens with the query

On hybrid RRF paths, Qdrant's fusion order is already the ranking signal. Axon sets `rerank_score = score`, skips the cosine-calibrated minimum relevance threshold, and keeps the topical-overlap guard.

**Topical overlap thresholds:**

| Query token count | Minimum overlap required |
|-------------------|--------------------------|
| 1–2 tokens | ≥ 1 token match |
| 3–4 tokens | ≥ 1 token match, OR coverage ≥ 50% |
| 5+ tokens | ≥ 2 matches AND coverage ≥ 34% |

---

## Stage 5 — Build the Context String

`build.rs → build_context_from_candidates`

The reranked pool is now assembled into three tiers, in order. Each entry is separated by `\n\n---\n\n`. A running `context_char_count` is maintained; once the count would exceed `cfg.ask_max_context_chars` (env: `AXON_ASK_MAX_CONTEXT_CHARS`), no further entries are added.

### Tier 1 — Top Chunks

`select_diverse_candidates(reranked, cfg.ask_chunk_limit, 1)`

Selects up to `ask_chunk_limit` (env: `AXON_ASK_CHUNK_LIMIT`) chunks from the reranked list, enforcing a diversity constraint of at most 1 chunk per unique URL per selection pass. Each selected chunk is formatted as:

```
## Top Chunk [S1]: example.com/guide/crawl

<chunk text>
```

### Tier 2 — Full Documents

`select_diverse_candidates(reranked, cfg.ask_full_docs, 1)` → fetched concurrently from Qdrant

For up to `ask_full_docs` (env: `AXON_ASK_FULL_DOCS`) URLs, all stored chunks for that URL are fetched from Qdrant via `qdrant_retrieve_by_url`, capped at `cfg.ask_doc_chunk_limit` chunks per document. Fetches run concurrently up to `cfg.ask_doc_fetch_concurrency` (env: `AXON_ASK_DOC_FETCH_CONCURRENCY`) at a time, and results are re-sorted by original rank order before insertion.

This only runs if `context_char_count < max_context_chars`. Each full doc is formatted as:

```
## Source Document [S2]: example.com/api/reference

<all chunks concatenated>
```

### Tier 3 — Supplemental Chunks (backfill)

This tier fires only when **both** conditions hold:

1. Context is under 85% of `max_context_chars`
2. Either no full docs were selected, **or** fewer than 6 top chunks were selected

Supplemental candidates are those remaining in the reranked pool that were not already inserted as full docs. On cosine/dense-only paths, they must satisfy `rerank_score >= ask_min_relevance_score + SUPPLEMENTAL_RELEVANCE_BONUS`; the current bonus is `0.0`, so this matches the normal relevance floor. On hybrid RRF paths, there is no score floor because RRF scores are unitless. Up to `cfg.ask_backfill_chunks` (env: `AXON_ASK_BACKFILL_CHUNKS`) are selected with the same per-URL diversity pass. Each is formatted as:

```
## Supplemental Chunk [S3]: example.com/changelog

<chunk text>
```

### Final assembly

```rust
format!("Sources:\n{}", context_entries.join("\n\n---\n\n"))
```

This string is the `context` that flows into the LLM prompt.

---

## How Context Is Injected into the LLM

`streaming.rs → ask_llm_streaming / ask_llm_streaming_tagged`

The final user message sent to the LLM completion backend (Gemini headless by default, or an OpenAI-compatible endpoint when `AXON_LLM_BACKEND=openai-compat`, or the OpenAI Codex CLI when `AXON_LLM_BACKEND=codex-app-server`) is:

```
Question: {query}

Context:
{context}
```

The system prompt (`ASK_RAG_SYSTEM_PROMPT`) instructs the model:

- Answer **only** from the retrieved context. No unstated prior knowledge.
- Perform a relevance check first (keyword overlap ≠ topical alignment).
- If relevant context exists: answer with inline citations like `[S1]`, `[S4]`.
- If no relevant context: say so and suggest what to index — **do not hallucinate**.
- End with a single `## Sources` section.

Temperature is fixed at `0.1` for both RAG and baseline calls, keeping outputs deterministic.

---

## Evaluate — Differences from Ask

`evaluate.rs` reuses `build_ask_context` for the RAG arm but adds:

- **Baseline arm**: runs the exact same question with no context (baseline system prompt tells the LLM to use its training knowledge).
- **Concurrent streaming**: both arms (`with_context` and `without_context`) stream simultaneously via a shared `mpsc::unbounded_channel::<TaggedToken>`, dispatched with `tokio::select!`.
- **Judge reference**: a second independent retrieval runs after both answers complete (`build_judge_reference`), fetching up to 8 diverse chunks for the judge LLM to use as ground truth. This is separate from the RAG context so the judge has an unbiased reference.
- **Judge prompt**: the judge receives both answers, timing info, source list, and the reference chunks. It scores each answer on Accuracy, Relevance, Completeness, and Specificity (each X/5), then issues a verdict.
- **Auto-suggest**: if RAG scores below baseline, `discover_crawl_suggestions` is called automatically and the suggested URLs plus reasons are returned as data. Evaluate does not enqueue crawl jobs by default.

---

## Configuration Reference

| Env var | What it controls | Typical default |
|---------|-----------------|-----------------|
| `AXON_ASK_CANDIDATE_LIMIT` | Qdrant candidate count per search arm | 250 |
| `AXON_ASK_HYBRID_CANDIDATES` | Hybrid dense/sparse prefetch window per arm | 150 |
| `AXON_ASK_MIN_RELEVANCE_SCORE` | Minimum rerank score to keep a candidate | 0.45 |
| `AXON_ASK_CHUNK_LIMIT` | Max top chunks (Tier 1) | 20 |
| `AXON_ASK_FULL_DOCS` | Max full-document fetches (Tier 2) | 6 |
| `AXON_ASK_DOC_CHUNK_LIMIT` | Max chunks per full-doc fetch | 96 |
| `AXON_ASK_DOC_FETCH_CONCURRENCY` | Concurrent Qdrant fetches for full docs | 4 |
| `AXON_ASK_BACKFILL_CHUNKS` | Max supplemental chunks (Tier 3) | 5 |
| `AXON_ASK_MAX_CONTEXT_CHARS` | Hard cap on assembled context length | 300000 |
| `AXON_ASK_AUTHORITATIVE_DOMAINS` | Comma-separated domains that receive an authority boost | (empty) |
| `AXON_ASK_AUTHORITATIVE_BOOST` | Score boost for authoritative domains | 0.0 |
| `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` | Minimum unique citations for non-trivial answers | 2 |

Defaults in this table are owned by `src/core/config/parse/build_config.rs` and `src/core/config/types/config_impls.rs`. Re-check both when changing defaults because tests and direct `Config::default()` callers can differ from CLI/env construction.

---

## Data Flow Diagram

```
User query string
       │
       ▼
  tei_embed()  ──────────────────────────────────►  Dense vector
       │
       ▼
  qdrant_search(vector, ask_candidate_limit)  ────►  Vec<ScoredPoint>
       │
       ▼  (filter: chunk_text.len() >= 40, low-signal, allowlist)
  candidates: Vec<AskCandidate>
       │
       ▼  rerank_ask_candidates()
            ├─ lexical boost (url_tokens + chunk_tokens)
            ├─ docs path boost
            ├─ authority domain boost
            └─ verbatim phrase boost
       │
       ▼  filter: rerank_score >= min_relevance AND topical_overlap
  reranked: Vec<AskCandidate>
       │
       ├──► Tier 1: select_diverse_candidates → top N chunks
       │              └─ format "## Top Chunk [Sx]: url\n\ntext"
       │
       ├──► Tier 2: qdrant_retrieve_by_url (concurrent) → full docs
       │              └─ format "## Source Document [Sx]: url\n\ntext"
       │
       └──► Tier 3: supplemental backfill (if under 85% budget)
                      └─ format "## Supplemental Chunk [Sx]: url\n\ntext"
       │
       ▼
  context = "Sources:\n" + entries.join("\n\n---\n\n")
       │
       ▼
  LLM user message:
    "Question: {query}\n\nContext:\n{context}"
```
