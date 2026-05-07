# axon ask
Last Modified: 2026-03-03

Version: 1.0.0
Last Updated: 20:30:18 | 03/03/2026 EST

RAG-powered Q&A. Retrieves relevant chunks from the local Qdrant knowledge base, reranks them by relevance, builds a context window, and calls the configured LLM to generate a grounded answer.

## Synopsis

```bash
axon ask <question> [FLAGS]
axon ask --query "<question>" [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<question>` | Question to answer (positional, or via `--query`) |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `TEI_URL` | TEI embeddings base URL. Used to embed the question before Qdrant search. |
| `QDRANT_URL` | Qdrant base URL. Searched for relevant chunks. |
| `AXON_HEADLESS_GEMINI_CMD` | Optional Gemini CLI command. Defaults to `gemini`. |
| `AXON_HEADLESS_GEMINI_MODEL` | Optional Gemini model override for answer generation. |

`ask` runs in lite mode by default and does not require Postgres, Redis, or AMQP.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--query <text>` | — | Question text (alternative to positional argument). |
| `--collection <name>` | `cortex` | Qdrant collection to search. |
| `--diagnostics` | `false` | Print retrieval diagnostics (candidate pool, reranked pool, chunks selected, full docs, supplemental, context chars, graph entities, authority ratio, dropped by allowlist, top domains). |
| `--graph` | `false` | Enable graph-enhanced retrieval via Neo4j (requires `AXON_NEO4J_URL`). |
| `--json` | `false` | Machine-readable JSON output. |

Note: `ask` runs synchronously and does not support `--wait`.

`--limit` is a global flag but is not used by `ask` retrieval. Ask retrieval depth is controlled by `AXON_ASK_*` tuning env vars.

## Examples

```bash
# Basic ask
axon ask "how does spider.rs handle JavaScript-heavy sites?"

# Using --query flag
axon ask --query "what is the default chunk size for TEI batch requests?"

# Specific collection
axon ask "list all indexed rust crates" --collection rust-libs

# Debug: show retrieved chunks and scores
axon ask "qdrant HNSW parameters" --diagnostics

# JSON output
axon ask "what is the max crawl depth?" --json
```

## RAG Pipeline

1. Embed the question via TEI
2. Query Qdrant for top `AXON_ASK_CANDIDATE_LIMIT` (default: 150) candidate chunks
3. Apply the score threshold only on cosine/dense paths. `AXON_ASK_MIN_RELEVANCE_SCORE` (default: 0.45) is used for legacy unnamed-vector collections, named dense searches, named-vector collections with hybrid disabled, and named-vector searches whose sparse query is empty.
4. Skip that threshold in hybrid/RRF named-vector mode. RRF scores are rank-fusion outputs rather than cosine scores, so ask keeps the loose topical-overlap gate and uses Qdrant's fused ordering.
5. Rerank by the mode-appropriate score/order; take top `AXON_ASK_CHUNK_LIMIT` (default: 10)
6. For top `AXON_ASK_FULL_DOCS` (default: 4) documents, backfill additional chunks from the same document
7. Assemble context up to `AXON_ASK_MAX_CONTEXT_CHARS` (default: 120,000) characters
8. Call Gemini headless with context + question
9. Apply response-quality gates (citations + policy checks)
10. Print the normalized answer

## RAG Tuning

The retrieval pipeline is tunable via environment variables. See the [Environment section](../../README.md#ask-rag-tuning) in the README for the full table. Short reference:

| Variable | Default | Effect |
|----------|---------|--------|
| `AXON_ASK_MIN_RELEVANCE_SCORE` | `0.45` | Raise to tighten relevance on cosine/dense paths (0.6–0.7 for high-precision); lower if you get "no candidates". Skipped for hybrid/RRF named-vector mode because RRF scores are not cosine scores. |
| `AXON_ASK_CANDIDATE_LIMIT` | `150` | More candidates = better recall, slower reranking |
| `AXON_ASK_CHUNK_LIMIT` | `10` | Chunks in final LLM context |
| `AXON_ASK_MAX_CONTEXT_CHARS` | `120000` | Total context characters; raise for large-context models |
| `AXON_ASK_AUTHORITATIVE_DOMAINS` | `` | Optional comma-separated domains to boost in reranking |
| `AXON_ASK_AUTHORITATIVE_BOOST` | `0.0` | Score boost for authoritative-domain matches |
| `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` | `2` | Minimum unique citations for non-trivial answers |

## Notes

- LLM answer generation goes through Gemini headless. `AXON_HEADLESS_GEMINI_MODEL` is used as the Gemini model override.
- If you get "No candidates met relevance threshold", lower `AXON_ASK_MIN_RELEVANCE_SCORE` or run `axon crawl`/`axon embed` to add more content to the collection. This message comes from cosine/dense retrieval paths; hybrid/RRF named-vector mode skips the cosine threshold.
- `ask` queries the local knowledge base only. To search the live web, use `axon research`.
- For benchmarking RAG quality vs a baseline, use `axon evaluate`.
- `ask` enforces citation-quality gates:
  - Answers must include inline `[S#]` citations from retrieved context.
  - Non-trivial responses must satisfy `AXON_ASK_MIN_CITATIONS_NONTRIVIAL`.
  - Failed gates return structured insufficient-evidence output with next-index suggestions.
