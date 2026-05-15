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
| `AXON_SERVER_URL` | Optional generic server endpoint. If set, server-mode capable commands use `axon serve`; `ask` uses the same server URL. |

`ask` uses Qdrant + TEI retrieval and Gemini headless synthesis.

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--query <text>` | — | Question text (alternative to positional argument). |
| `--collection <name>` | `cortex` | Qdrant collection to search. |
| `--diagnostics` | `false` | Print retrieval diagnostics (candidate pool, reranked pool, chunks selected, full docs, supplemental, context chars, authority ratio, dropped by allowlist, top domains). |
| `--explain` | `false` | Emit a per-candidate ranking/context trace. Implies diagnostics and skips LLM synthesis; use with `--json` for the full payload. |
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

# Explain ranking/context decisions without calling the LLM
axon ask "claude marketplace plugins" --explain --json

# JSON output
axon ask "what is the max crawl depth?" --json

# Ask through a running server
AXON_SERVER_URL=http://127.0.0.1:8001 axon ask "what changed in server mode?"
```

## RAG Pipeline

1. Embed the question via TEI
2. Query Qdrant for top `ask.candidate-limit` (default: 150) candidate chunks
3. Apply the score threshold only on cosine/dense paths. `ask.min-relevance-score` (default: 0.45) is used for legacy unnamed-vector collections, named dense searches, named-vector collections with hybrid disabled, and named-vector searches whose sparse query is empty.
4. Skip that threshold in hybrid/RRF named-vector mode. RRF scores are rank-fusion outputs rather than cosine scores, so ask keeps the loose topical-overlap gate and uses Qdrant's fused ordering.
5. Rerank by the mode-appropriate score/order; take top `ask.chunk-limit` (default: 10)
6. For top `AXON_ASK_FULL_DOCS` (default: 4) documents, backfill additional chunks from the same document
7. Assemble context up to `AXON_ASK_MAX_CONTEXT_CHARS` (default: 120,000) characters
8. Call Gemini headless with context + question
9. Apply response-quality gates (citations + policy checks)
10. Print the normalized answer

## Explain Trace

Use `--diagnostics` for aggregate health counters. Use `--explain --json` when a ranking result looks wrong and you need the per-candidate math and context decisions. Explain mode returns the normal `AskResult` shape with `answer: ""`, `timing_ms.llm: 0`, `explain.llm_skipped: true`, and no Gemini call.

Compact example:

```json
{
  "query": "claude marketplace plugins",
  "answer": "",
  "diagnostics": { "candidate_pool": 15, "reranked_pool": 12 },
  "explain": {
    "mode": "explain_only",
    "retrieval": {
      "score_kind": "cosine",
      "vector_mode": "unnamed",
      "hybrid_search_enabled": true
    },
    "candidates": [
      {
        "id": "candidate-1",
        "url": "https://code.claude.com/docs/en/plugins",
        "retrieval_score": 0.17,
        "rerank_score": 0.62,
        "score_components": [
          { "name": "retrieval_score", "value": 0.17, "status": "applied" },
          { "name": "authority_boost", "value": 0.35, "status": "applied" }
        ],
        "filter_decisions": [{ "kind": "kept" }],
        "selection_decisions": [{ "kind": "selected_top_chunk" }]
      }
    ],
    "context": {
      "planned_full_doc_urls": [],
      "full_doc_fetch_skipped": true,
      "full_doc_fetch_mode": "cosine",
      "final_source_order": [
        { "source_id": "S1", "url": "https://code.claude.com/docs/en/plugins", "tier": "top_chunk" }
      ],
      "truncated_by_budget": false
    },
    "llm_skipped": true
  },
  "timing_ms": { "retrieval": 21, "context_build": 3, "graph": 0, "llm": 0, "total": 24 }
}
```

`retrieval_score` scale depends on retrieval mode. Cosine/dense paths use cosine-like scores and may apply `ask.min-relevance-score`; RRF paths use rank-fusion scores, mark additive rerank components as `skipped`, and do not apply the cosine threshold.

## RAG Tuning

The core retrieval-selection knobs live in `~/.axon/config.toml` under `[ask]`.
Env vars with the same names are compatibility overrides, not the normal place
to store these values.

| TOML key | Env override | Default | Effect |
|----------|--------------|---------|--------|
| `ask.min-relevance-score` | `AXON_ASK_MIN_RELEVANCE_SCORE` | `0.45` | Raise to tighten relevance on cosine/dense paths (0.6-0.7 for high-precision); lower if you get "no candidates". Skipped for hybrid/RRF named-vector mode because RRF scores are not cosine scores. |
| `ask.candidate-limit` | `AXON_ASK_CANDIDATE_LIMIT` | `150` | More candidates = better recall, slower reranking |
| `ask.chunk-limit` | `AXON_ASK_CHUNK_LIMIT` | `10` | Chunks in final LLM context |

Remaining runtime ask controls are still env-only until typed TOML fields exist:

| Variable | Default | Effect |
|----------|---------|--------|
| `AXON_ASK_MAX_CONTEXT_CHARS` | `120000` | Total context characters; raise for large-context models |
| `AXON_ASK_AUTHORITATIVE_DOMAINS` | `` | Optional comma-separated domains to boost in reranking |
| `AXON_ASK_AUTHORITATIVE_BOOST` | `0.0` | Score boost for authoritative-domain matches |
| `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` | `2` | Minimum unique citations for non-trivial answers |

## Notes

- LLM answer generation goes through Gemini headless. `AXON_HEADLESS_GEMINI_MODEL` is used as the Gemini model override.
- If you get "No candidates met relevance threshold", lower `ask.min-relevance-score` in `~/.axon/config.toml` or run `axon crawl`/`axon embed` to add more content to the collection. This message comes from cosine/dense retrieval paths; hybrid/RRF named-vector mode skips the cosine threshold.
- `ask` queries the local knowledge base only. To search the live web, use `axon research`.
- For benchmarking RAG quality vs a baseline, use `axon evaluate`.
- `ask` enforces citation-quality gates:
  - Answers must include inline `[S#]` citations from retrieved context.
  - Non-trivial responses must satisfy `AXON_ASK_MIN_CITATIONS_NONTRIVIAL`.
  - Failed gates return structured insufficient-evidence output with next-index suggestions.
