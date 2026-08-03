---
title: "Context Injection Pipeline"
created: 2026-03-04
updated: 2026-08-02
---

# Context Injection Pipeline

This guide describes the live retrieval and prompt-composition path used by
`ask` and the RAG arm of `evaluate`.

## Current Flow

```text
question
  -> one query embedding
  -> Qdrant dense + BM42 sparse search with RRF fusion
     (or named-dense search when hybrid is disabled)
  -> strict prefix of ranked chunks, bounded by chunk count and context size
  -> `Sources:` context containing `Top Chunk` entries
  -> configured LLM backend
  -> citation normalization and one repair attempt when needed
```

The implementation seam is
`crates/axon-services/src/query/ask_retrieval.rs`. It calls
`axon_retrieval::run_query`, renders the returned hits with
`build_ask_context_from_hits`, and hands the resulting `AskContext` to the
synthesis code under `crates/axon-services/src/query/synthesis/`.

The current path does **not** run a separate lexical reranker, authority-domain
boost, minimum cosine-score gate, dual natural-language/keyword search,
full-document fetch, per-URL diversity pass, or supplemental-chunk backfill.
Those stages belonged to the retired ask implementation.

## 1. Embed the Question

`axon_retrieval::run_query` constructs the retrieval engine with the active
vector store and embedding provider. The engine sends one plain-text query
item through `EmbeddingProvider::embed` and verifies the returned provider id,
model, and dimensions against the provider capabilities.

There is no second keyword-shaped embedding and no query rewrite at this
boundary. When hybrid retrieval is enabled, Axon separately computes the BM42
sparse query vector from the original question.

`ask` requires configured Qdrant and embedding endpoints. The CLI reports an
error before retrieval when either `QDRANT_URL` or `TEI_URL` is empty.

## 2. Retrieve Ranked Chunks

`retrieval_ask_context_with_hits` requests:

```text
max(ask_hybrid_candidates, ask_chunk_limit, 1)
```

hits from the configured collection. The retrieval engine applies these
payload filters:

- visibility is `public`, `internal`, or `derived`;
- `redaction_status` is `clean`;
- `document_status` is `published`;
- `embedded_at` respects `--since` and `--before` when supplied.

Plain `query` and `ask` also exclude memory records by `source_kind`; memory
retrieval has its own explicit path.

### Hybrid mode

Hybrid retrieval is enabled by default. Axon sends named `dense` and `bm42`
prefetches to Qdrant and asks Qdrant to fuse them with reciprocal-rank fusion
(RRF). The returned score is the fused ranking score. Axon does not add a
second rerank score after Qdrant returns the hits.

Hybrid mode requires a collection with the configured sparse vector namespace.
It does not silently fall back when the collection lacks sparse-vector support.
Use `--no-hybrid-search` for an intentional named-dense query, or recreate the
collection through the clean-break reset flow described in
[Re-indexing](reindexing.md).

### Dense-only mode

`--no-hybrid-search` (or the equivalent transport override) skips BM42 and RRF
and issues one named-dense Qdrant query. The context builder still consumes the
hits in the order returned by Qdrant; it does not apply the retired cosine
threshold or lexical reranker.

## 3. Build the `Sources:` Context

`build_ask_context_from_hits` walks the ranked hit list in order and accepts a
strict prefix, stopping when either:

- `ask_chunk_limit` entries have been considered; or
- the next complete entry would exceed `ask_max_context_chars`.

The configured context limit is enforced against the UTF-8 byte length of the
rendered string, despite the historical `chars` name. Entries are never
partially truncated. If the first entry does not fit, the context contains only
the `Sources:` prefix.

Every selected hit is rendered as:

```text
## Top Chunk [S1]: example.com

<retrieved_content trust="evidence_only">
<chunk text>
</retrieved_content>
```

Entries are separated by `\n\n---\n\n`. URL sources display their hostname in
the prompt header; non-URL canonical URIs display the raw value. The canonical
citation retained in the result still carries the complete source, document,
chunk, generation, job, URI, range, and redaction lineage.

Indexed text is untrusted evidence. Before insertion, Axon defangs source
headers and `[S#]` patterns inside chunk bodies so a stored document cannot
forge prompt structure or citation identifiers.

The builder does not fetch complete documents, suppress duplicate URLs, or add
supplemental entries. Diagnostic fields retained for wire compatibility report
zero selected full documents and zero supplemental chunks on this path.

## 4. Inject Context and Synthesize

The completion user message is built in
`crates/axon-services/src/query/synthesis/completion.rs`:

```text
Question: {query}

Context:
{sources_context}
```

The system prompt is the embedded `rag-synthesize` contract. It directs the
model to treat retrieved text as untrusted evidence, answer only from that
evidence, cite factual statements with `[S#]`, and identify gaps rather than
inventing unsupported facts.

The active backend is selected by `AXON_LLM_BACKEND` and can be Gemini
headless, OpenAI-compatible, or Codex app-server. Interactive CLI answers
stream by default; `--no-stream`, JSON output, and explain mode use buffered or
non-synthesis paths as appropriate.

After completion, Axon:

1. parses the source map from the rendered context;
2. rejects missing or unmapped citations;
3. requires the configured number of unique citations for non-trivial answers;
4. canonicalizes and renumbers cited sources in the final `## Sources` list;
5. retries synthesis once with a citation-repair prompt when validation fails.

If the model reports insufficient evidence, Axon returns an explicit
insufficient-evidence response with suggested indexing targets. If citation
repair still fails, the normalized answer retains structured validation
failure details instead of being presented as fully grounded.

## Follow-up Sessions

For `--follow-up` and `--resume`, the CLI rewrites the retrieval question using
bounded prior turns and appends a separately delimited history source to the
synthesis context. Retrieved facts still require citations. Ask-session JSONL
files live under `$AXON_DATA_DIR/ask-sessions/`.

## Explain Mode

`axon ask --explain --json` runs the same retrieval pass and skips only the LLM
call. Its trace reflects the current engine:

- `retrieval_score` and `rerank_score` are equal because there is no separate
  reranker;
- selected hits are `top_chunk`; remaining hits are `not_selected`;
- full-document planning and fetch lists are empty;
- `llm_skipped` is `true`.

Raw rendered context is omitted from the default explain payload. Use the
candidate rows, final source order, and byte/character budget counters to debug
retrieval and context selection.

## Active Controls

| Control | Runtime effect |
|---|---|
| `providers.vector.hybrid-enabled` / `AXON_HYBRID_SEARCH` | Select hybrid RRF or named-dense retrieval. |
| `retrieval.ask-hybrid-candidates` / `AXON_ASK_HYBRID_CANDIDATES` | Requested hit count before context selection. |
| `ask.chunk-limit` / `AXON_ASK_CHUNK_LIMIT` | Maximum top chunks admitted to context. |
| `ask.max-context-chars` / `AXON_ASK_MAX_CONTEXT_CHARS` | Rendered-context byte budget on the current path. |
| `ask.min-citations-nontrivial` / `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` | Minimum unique citations for a non-trivial answer. |
| `--collection`, `--since`, `--before` | Per-request collection and time bounds. |
| `--no-hybrid-search` | Per-request dense-only override. |

The config and transport DTOs still accept several fields from the retired
pipeline (`candidate-limit`, `full-docs`, document-fetch/backfill controls,
`min-relevance-score`, and authority-domain controls). The current
retrieval-engine ask path does not consult those values, so they must not be
used to explain current ranking or context behavior.

## Data Flow

```text
question
  |
  v
EmbeddingProvider::embed (one query input)
  |                         \
  |                          +-- BM42 sparse query when hybrid=true
  v
VectorStore::search
  |
  +-- Qdrant dense + sparse prefetch -> RRF
  |   or named-dense query
  v
ranked QueryServiceHit values
  |
  v
strict prefix by chunk limit + context budget
  |
  v
Sources:\n## Top Chunk [S#] ...
  |
  v
LLM completion -> citation normalization/repair -> AskResult
```

See also [the `ask` action reference](../reference/actions/ask.md) and the
[vector payload contract](../reference/sources/vector-payload.md).
