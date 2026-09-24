---
title: "Context Injection Pipeline"
created: 2026-03-04
updated: 2026-09-18
---

# Context Injection Pipeline

How Axon retrieves, ranks, and assembles the `Context:` block that is injected into the RAG LLM prompt for `ask` and `evaluate`.

---

## Overview

Every `ask` and `evaluate` command goes through the same five-stage pipeline before any LLM call is made:

```
Query
  └─► 1. Embed          — the unified retrieval engine embeds the user query
  └─► 2. Retrieve       — dense or dense+BM42/RRF Qdrant search returns candidates
  └─► 3. Filter         — low-signal, dense-relevance, and named-product identity guards
  └─► 4. Rank           — mode-aware policy preserves RRF scale or reranks dense scores
  └─► 5. Build context  — diverse, bounded chunks become evidence-only source blocks
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

`crates/axon-retrieval/src/engine.rs`

The unified retrieval engine embeds the user query once through the configured embedder. The old ask-only dual-embedding/keyword-query path is no longer part of the active `ask` pipeline. The resulting dense vector is paired with a client-side BM42 sparse query vector when hybrid retrieval is enabled.

```
"how does axon crawl work?"
        ↓ query embedding
Dense vector
        └─► optional BM42 sparse query vector
```

---

## Stage 2 — Retrieve Candidates from Qdrant

`crates/axon-services/src/query/ask_retrieval.rs → axon-retrieval → axon-vectors`

`ask` calls the shared retrieval service with a result limit of `max(ask_candidate_limit, ask_chunk_limit, 1)`. Hybrid mode sends both the dense embedding and a BM42 sparse vector to Qdrant's `/points/query` RRF path. `ask_hybrid_candidates` controls the dense and sparse prefetch window per arm; dense-only mode omits the sparse arm.

Each result carries:
- `score` — cosine-style on dense-only paths; unitless RRF fusion score on hybrid paths
- `canonical_uri` — canonical source identity used for trust, diversity, and citations
- `text` — retrieved chunk text
- canonical citation metadata from the retrieval boundary

The shared retrieval engine also enforces source/generation filters before the ask-specific ranking policy runs.

---

## Stage 3 — Filter

`crates/axon-services/src/query/ask_retrieval/ranking.rs`

Ask applies three post-retrieval gates:

1. **Low-signal source gate.** Session URIs, local `file://` sources, JSONL/session exports, cache paths, and local log paths are suppressed for ordinary product/documentation questions. Queries explicitly asking for session, transcript, log, or history evidence opt back in.
2. **Dense relevance floor.** Dense-only retrieval drops candidates whose raw cosine-style score is below `ask.min-relevance-score`. Hybrid RRF does not apply this threshold because RRF scores are not cosine similarities.
3. **Named-product identity guard.** When a query explicitly names a product in Axon's small registered product map, a candidate must carry that product identity or a registered alias in its URL/text. Generic terms such as `docs`, `config`, `plugin`, and `api` are not used as a hard lexical relevance gate; dense embeddings and hybrid RRF may preserve semantically relevant synonym matches.

Dropped candidates remain visible in `ask --explain` with explicit filter decisions.

---

## Stage 4 — Mode-Aware Ranking

`crates/axon-services/src/query/ask_retrieval/ranking.rs`

Dense/cosine results can safely absorb additive relevance deltas:

```
rerank_score = retrieval_score
             + lexical_url_and_text_boost   (combined cap 0.30)
             + docs_path_boost              (0.04)
             + configured_authority_boost
             + verified_product_authority   (0.35)
             + phrase_match_boost           (0.06)
```

Configured authority comes from `ask.authoritative-domains` / `AXON_ASK_AUTHORITATIVE_DOMAINS` and uses exact-host-or-subdomain matching. Built-in product authority is deliberately fail-closed to Axon's small official-domain registry; a docs-looking untrusted host cannot gain trust merely by putting a product name in its hostname or path.

Hybrid RRF uses a different score scale. Axon preserves Qdrant's fused relevance signal: lexical URL/text, docs-path, and phrase-match components are emitted as `skipped` and contribute zero. Explicit configured authority and fail-closed verified product authority may raise an RRF candidate, but their combined delta is capped at that candidate's original fused score, so trust can improve an RRF score by at most 2×.

---

## Stage 5 — Build the Context String

`crates/axon-services/src/query/ask_retrieval.rs`

The active unified ask path assembles context directly from ranked chunks. Legacy full-document fetch and supplemental-backfill controls remain compatibility fields, but they are not executed by this path.

Before selection, Axon derives an effective budget from the configured synthesis model tier and coarse query complexity (simple, complex, or exhaustive). The effective chunk count and character budget never exceed the configured `ask_chunk_limit` / `ask_max_context_chars` ceilings.

Selection is document-diverse: Axon first takes at most one ranked chunk per canonical source, then considers repeat chunks only if capacity remains. Each chunk also has a per-chunk character cap. Evidence shorter than the minimum useful body threshold is skipped.

Retrieved text is treated as untrusted evidence. Structural source markers and citation-like `[S#]` text are defanged, unsafe control characters are removed, source labels are forced onto one line, case-insensitive retrieved_content boundary tokens are defanged, and every body is wrapped in an evidence-only boundary:

```
## Top Chunk [S1]: example.com/guide/crawl

<retrieved_content trust="evidence_only">
<defanged chunk text>
</retrieved_content>
```

The final `Sources:` context is bounded in Unicode scalar values, not UTF-8 bytes. Explain/diagnostic output reports the active character budget and actual character/byte usage separately.

---

## How Context Is Injected into the LLM

`streaming.rs → ask_llm_streaming / ask_llm_streaming_tagged`

The final user message sent to the LLM completion backend (Gemini headless by default, or an OpenAI-compatible endpoint when `AXON_LLM_BACKEND=openai-compat`) is:

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
| `AXON_ASK_CANDIDATE_LIMIT` | Ask candidate pool ceiling before ranking | Model-tiered: 250 large, 150 GPT/Codex, 120 local Gemma, 60 unknown |
| `AXON_ASK_HYBRID_CANDIDATES` | Hybrid dense/sparse prefetch window per arm | Model-tiered: 200 large, 120 GPT/Codex, 100 local Gemma, 60 unknown |
| `AXON_ASK_MIN_RELEVANCE_SCORE` | Raw dense/cosine relevance floor; not applied to RRF | 0.45 |
| `AXON_ASK_CHUNK_LIMIT` | Configured selected-chunk ceiling; adaptive runtime budget may be lower | Model-tiered: 50 large, 28 GPT/Codex, 20 local Gemma, 10 unknown |
| `AXON_ASK_FULL_DOCS` | Compatibility-only legacy full-document control on unified ask | No active full-doc fetch on unified ask |
| `AXON_ASK_DOC_CHUNK_LIMIT` | Compatibility-only legacy full-document chunk control | Not executed by unified ask |
| `AXON_ASK_DOC_FETCH_CONCURRENCY` | Compatibility-only legacy full-document concurrency | Not executed by unified ask |
| `AXON_ASK_BACKFILL_CHUNKS` | Compatibility-only legacy supplemental-backfill control | Not executed by unified ask |
| `AXON_ASK_MAX_CONTEXT_CHARS` | Configured Unicode-character ceiling; adaptive runtime budget may be lower | Model-tiered: 1,000,000 large, 400,000 GPT/Codex, 128,000 local Gemma, 40,000 unknown |
| `AXON_ASK_AUTHORITATIVE_DOMAINS` | Comma-separated domains that receive an authority boost | (empty) |
| `AXON_ASK_AUTHORITATIVE_BOOST` | Score boost for authoritative domains | 0.0 |
| `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` | Minimum unique citations for non-trivial answers | 2 |

Model-tiered ask defaults are resolved in `crates/axon-core/src/config/parse/tuning.rs` from `SynthesisModelProfile`. `AXON_SYNTHESIS_HIGH_CONTEXT` is resolved before those defaults so an explicit override participates in the tier selection.

---

## Data Flow Diagram

```
User query string
       │
       ▼
  unified retrieval engine ───────────────────────► Dense embedding
       │                                               + optional BM42 sparse vector
       ▼
  Qdrant dense search or dense+BM42 RRF ─────────► candidate hits
       │
       ▼
  ask ranking policy
       ├─ low-signal / dense-score / named-product identity filters
       ├─ dense: lexical + docs + phrase + trust boosts
       └─ RRF: preserve fused scale; bounded combined trust delta
       │
       ▼
  adaptive model/query budget
       │
       ▼
  document-diverse chunk selection
       ├─ per-chunk character cap
       ├─ defang source/citation/boundary markers and unsafe controls
       └─ wrap body as retrieved_content trust=evidence_only
       │
       ▼
  bounded Sources context with canonical citations
       │
       ▼
  LLM user message:
    "Question: {query}\n\nContext:\n{context}"
```
